//! Windows Core Audio backend: per-app audio sessions on every active output
//! device, plus the default microphone.

#![cfg(windows)]

mod process;

use std::collections::HashMap;

use mixer_core::{AudioBackend, CaptureInfo, SessionInfo};
use windows::{
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::S_OK,
        Media::Audio::{
            AudioSessionStateExpired, DEVICE_STATE_ACTIVE,
            Endpoints::{IAudioEndpointVolume, IAudioMeterInformation},
            IAudioSessionControl2, IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator,
            ISimpleAudioVolume, MMDeviceEnumerator, eCapture, eConsole, eRender,
        },
        System::Com::{
            CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
            CoUninitialize, STGM_READ,
        },
    },
    core::{Interface, PWSTR, Result},
};

use crate::process::ProcessCache;

struct Session {
    volume: ISimpleAudioVolume,
    meter: IAudioMeterInformation,
}

struct Device {
    name: String,
    sessions: IAudioSessionManager2,
}

struct Capture {
    volume: IAudioEndpointVolume,
    meter: IAudioMeterInformation,
}

/// Must be created and used on one thread (the mixer host thread).
pub struct WasapiBackend {
    // Field order matters: COM objects must drop before CoUninitialize in Drop.
    enumerator: IMMDeviceEnumerator,
    devices: HashMap<String, Device>,
    sessions: HashMap<String, Session>,
    capture: Option<(String, Capture)>,
    processes: ProcessCache,
    com_initialized: bool,
}

impl WasapiBackend {
    pub fn new() -> Result<Self> {
        // S_FALSE (already initialized on this thread) is fine too.
        let com_initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        Ok(Self {
            enumerator,
            devices: HashMap::new(),
            sessions: HashMap::new(),
            capture: None,
            processes: ProcessCache::default(),
            com_initialized,
        })
    }

    /// Keeps one session manager per active output device; devices that
    /// disappear (headset unplugged) are dropped.
    fn refresh_devices(&mut self) -> Result<()> {
        let collection = unsafe {
            self.enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?
        };
        let count = unsafe { collection.GetCount()? };
        let mut live = HashMap::with_capacity(count as usize);
        for i in 0..count {
            let device = unsafe { collection.Item(i)? };
            let id = device_id(&device)?;
            let entry = match self.devices.remove(&id) {
                Some(existing) => existing,
                None => Device {
                    name: friendly_name(&device),
                    sessions: unsafe {
                        device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)?
                    },
                },
            };
            live.insert(id, entry);
        }
        self.devices = live;
        Ok(())
    }

    fn scan_device(
        &mut self,
        device: &Device,
        out: &mut Vec<SessionInfo>,
        handles: &mut HashMap<String, Session>,
    ) -> Result<()> {
        let list = unsafe { device.sessions.GetSessionEnumerator()? };
        for i in 0..unsafe { list.GetCount()? } {
            let control = unsafe { list.GetSession(i)? };
            if unsafe { control.GetState()? } == AudioSessionStateExpired {
                continue;
            }
            let control: IAudioSessionControl2 = control.cast()?;
            let id = unsafe { take_pwstr(control.GetSessionInstanceIdentifier()?) };
            let pid = unsafe { control.GetProcessId() }.unwrap_or(0);
            let is_system_sounds = unsafe { control.IsSystemSoundsSession() } == S_OK;
            let session_name = unsafe { control.GetDisplayName() }
                .map(|p| unsafe { take_pwstr(p) })
                .unwrap_or_default();

            let process = self.processes.get(pid);
            let display_name = if is_system_sounds {
                "Systemlyder".to_string()
            } else {
                process
                    .description
                    .clone()
                    .filter(|d| !d.is_empty())
                    .or_else(|| Some(session_name).filter(|n| !n.is_empty() && !n.starts_with('@')))
                    .unwrap_or_else(|| process.name.trim_end_matches(".exe").to_string())
            };

            handles.insert(
                id.clone(),
                Session {
                    volume: control.cast()?,
                    meter: control.cast()?,
                },
            );
            out.push(SessionInfo {
                id,
                pid,
                process_name: process.name.clone(),
                exe_path: process.path.clone(),
                display_name,
                is_system_sounds,
                device_name: device.name.clone(),
            });
        }
        Ok(())
    }

    fn capture_endpoints(&self) -> Result<Vec<(IMMDevice, String)>> {
        let collection = unsafe {
            self.enumerator
                .EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?
        };
        let mut out = Vec::new();
        for i in 0..unsafe { collection.GetCount()? } {
            let device = unsafe { collection.Item(i)? };
            let name = friendly_name(&device);
            out.push((device, name));
        }
        Ok(out)
    }

    fn open_capture(&mut self, preferred: Option<&str>) -> Result<CaptureInfo> {
        let wanted = preferred.map(str::to_lowercase);
        let chosen = match &wanted {
            Some(w) => self
                .capture_endpoints()?
                .into_iter()
                .find(|(_, name)| name.to_lowercase().contains(w.as_str())),
            None => None,
        };
        let device = match chosen {
            Some((device, _)) => device,
            None => unsafe {
                self.enumerator
                    .GetDefaultAudioEndpoint(eCapture, eConsole)?
            },
        };
        let id = device_id(&device)?;
        if self
            .capture
            .as_ref()
            .is_none_or(|(existing, _)| *existing != id)
        {
            let capture = Capture {
                volume: unsafe { device.Activate(CLSCTX_ALL, None)? },
                meter: unsafe { device.Activate(CLSCTX_ALL, None)? },
            };
            self.capture = Some((id.clone(), capture));
        }
        let (_, capture) = self.capture.as_ref().expect("set above");
        Ok(CaptureInfo {
            device_id: id,
            device_name: friendly_name(&device),
            volume: unsafe { capture.volume.GetMasterVolumeLevelScalar()? },
            muted: unsafe { capture.volume.GetMute()? }.as_bool(),
        })
    }
}

impl AudioBackend for WasapiBackend {
    fn scan_sessions(&mut self) -> Vec<SessionInfo> {
        if let Err(e) = self.refresh_devices() {
            log::warn!("enumerating output devices: {e}");
        }
        let devices = std::mem::take(&mut self.devices);
        let mut out = Vec::new();
        let mut handles = HashMap::new();
        for device in devices.values() {
            if let Err(e) = self.scan_device(device, &mut out, &mut handles) {
                log::warn!("scanning sessions on {}: {e}", device.name);
            }
        }
        self.devices = devices;
        self.sessions = handles;
        self.processes.retain(out.iter().map(|s| s.pid));
        out
    }

    fn session_peak(&mut self, session_id: &str) -> f32 {
        self.sessions
            .get(session_id)
            .and_then(|s| unsafe { s.meter.GetPeakValue() }.ok())
            .unwrap_or(0.0)
    }

    fn set_session_volume(&mut self, session_id: &str, volume: f32) {
        if let Some(s) = self.sessions.get(session_id)
            && let Err(e) = unsafe { s.volume.SetMasterVolume(volume, std::ptr::null()) }
        {
            log::debug!("set volume on {session_id}: {e}");
        }
    }

    fn set_session_mute(&mut self, session_id: &str, muted: bool) {
        if let Some(s) = self.sessions.get(session_id)
            && let Err(e) = unsafe { s.volume.SetMute(muted, std::ptr::null()) }
        {
            log::debug!("set mute on {session_id}: {e}");
        }
    }

    fn scan_capture(&mut self, preferred: Option<&str>) -> Option<CaptureInfo> {
        match self.open_capture(preferred) {
            Ok(info) => Some(info),
            Err(_) => {
                self.capture = None;
                None
            }
        }
    }

    fn capture_devices(&mut self) -> Vec<String> {
        self.capture_endpoints()
            .map(|list| list.into_iter().map(|(_, name)| name).collect())
            .unwrap_or_default()
    }

    fn capture_peak(&mut self) -> f32 {
        self.capture
            .as_ref()
            .and_then(|(_, c)| unsafe { c.meter.GetPeakValue() }.ok())
            .unwrap_or(0.0)
    }

    fn set_capture_volume(&mut self, volume: f32) {
        if let Some((_, c)) = &self.capture
            && let Err(e) = unsafe {
                c.volume
                    .SetMasterVolumeLevelScalar(volume, std::ptr::null())
            }
        {
            log::warn!("set mic volume: {e}");
        }
    }

    fn set_capture_mute(&mut self, muted: bool) {
        if let Some((_, c)) = &self.capture
            && let Err(e) = unsafe { c.volume.SetMute(muted, std::ptr::null()) }
        {
            log::warn!("set mic mute: {e}");
        }
    }
}

impl Drop for WasapiBackend {
    fn drop(&mut self) {
        self.sessions.clear();
        self.devices.clear();
        self.capture = None;
        if self.com_initialized {
            unsafe { CoUninitialize() };
        }
    }
}

fn device_id(device: &IMMDevice) -> Result<String> {
    Ok(unsafe { take_pwstr(device.GetId()?) })
}

fn friendly_name(device: &IMMDevice) -> String {
    let name = || -> Result<String> {
        let store = unsafe { device.OpenPropertyStore(STGM_READ)? };
        let value = unsafe { store.GetValue(&PKEY_Device_FriendlyName)? };
        Ok(value.to_string())
    };
    name().unwrap_or_else(|_| "Ukjent enhet".into())
}

/// Copies a COM-allocated string and frees it.
unsafe fn take_pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as _)) };
    s
}

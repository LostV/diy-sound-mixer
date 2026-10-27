//! Windows Core Audio backend: per-app audio sessions on every active output
//! device, plus the default microphone.

#![cfg(windows)]

mod icon;
mod loopback;
mod policy;
mod process;
mod writer;

use std::collections::{HashMap, HashSet};

use mixer_core::{AudioBackend, CaptureInfo, DeviceInfo, SessionInfo};
use windows::{
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::S_OK,
        Media::Audio::{
            AudioSessionStateActive, AudioSessionStateExpired, DEVICE_STATE_ACTIVE,
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

pub use crate::icon::{icon_png, icon_rgba};
use crate::{
    loopback::ProcessMeter, policy::PolicyConfig, process::ProcessCache, writer::SessionWriter,
};

struct Session {
    pid: u32,
    active: bool,
    /// Not playing right now. Volume and mute for it wait in `deferred`
    /// until it plays: every write costs Windows ~0.5 s and, on some
    /// systems, ~60 MB of kernel memory, and idle sessions are most of them.
    /// System sounds never wait; their sounds are over before a rescan.
    idle: bool,
    volume: ISimpleAudioVolume,
    /// Windows' own session meter. Only a fallback: on some devices it reports
    /// the whole device's level, see `loopback`.
    meter: IAudioMeterInformation,
}

enum Meter {
    Running(ProcessMeter),
    /// Loopback capture was refused for this process; don't retry every scan.
    Failed,
}

/// Volume and mute waiting for an idle session to start playing.
#[derive(Default)]
struct Deferred {
    volume: Option<f32>,
    muted: Option<bool>,
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
    /// Writes held back for idle sessions, keyed by session id.
    deferred: HashMap<String, Deferred>,
    capture: Option<(String, Capture)>,
    /// Per-process level meters, keyed by pid.
    meters: HashMap<u32, Meter>,
    /// `None` when this Windows has no per-app routing we recognise.
    policy: Option<PolicyConfig>,
    processes: ProcessCache,
    /// Applies session volume and mute off this thread; Windows can be slow.
    writer: SessionWriter,
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
            deferred: HashMap::new(),
            capture: None,
            meters: HashMap::new(),
            policy: PolicyConfig::new(),
            processes: ProcessCache::default(),
            writer: SessionWriter::new(),
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
            let state = unsafe { control.GetState()? };
            if state == AudioSessionStateExpired {
                continue;
            }
            let control: IAudioSessionControl2 = control.cast()?;
            let id = unsafe { take_pwstr(control.GetSessionInstanceIdentifier()?) };
            let pid = unsafe { control.GetProcessId() }.unwrap_or(0);
            // The mixer's own window has an audio session too; it is not an app to mix.
            if pid == std::process::id() {
                continue;
            }
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
                    pid,
                    active: state == AudioSessionStateActive && !is_system_sounds,
                    idle: state != AudioSessionStateActive && !is_system_sounds,
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

    /// The output Windows has stored for `pid` (a device interface path), for
    /// diagnostics.
    pub fn app_output(&self, pid: u32) -> Option<String> {
        self.policy.as_ref()?.app_output(pid).ok().flatten()
    }

    /// Starts a loopback meter for every process with an active session and
    /// stops meters for processes that no longer have any session. Idle apps
    /// keep their meter, so pausing and resuming music doesn't restart it.
    fn refresh_meters(&mut self) {
        let with_session: HashSet<u32> = self.sessions.values().map(|s| s.pid).collect();
        self.meters.retain(|pid, _| with_session.contains(pid));

        let active: HashSet<u32> = self
            .sessions
            .values()
            .filter(|s| s.active && s.pid != 0)
            .map(|s| s.pid)
            .collect();
        for pid in active {
            self.meters
                .entry(pid)
                .or_insert_with(|| match ProcessMeter::start(pid) {
                    Ok(meter) => Meter::Running(meter),
                    Err(e) => {
                        log::warn!(
                            "loopback meter for pid {pid} unavailable, using session meter: {e}"
                        );
                        Meter::Failed
                    }
                });
        }
    }

    /// Hands held-back writes to the writer for sessions that started
    /// playing (or for every session with `all`, on exit, so nothing is left
    /// at an old volume), and forgets those for sessions that are gone.
    fn flush_deferred(&mut self, all: bool) {
        let sessions = &self.sessions;
        let writer = &self.writer;
        self.deferred.retain(|id, deferred| {
            let Some(session) = sessions.get(id) else {
                return false;
            };
            if session.idle && !all {
                return true;
            }
            if let Some(volume) = deferred.volume {
                writer.set_volume(id, &session.volume, volume);
            }
            if let Some(muted) = deferred.muted {
                writer.set_mute(id, &session.volume, muted);
            }
            false
        });
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
        // Let go of last scan's sessions first: enumerating while still
        // holding any of them leaks ~1 KB per held session per scan in
        // Windows' audio client code (and memory in Windows Audio with it).
        self.sessions.clear();
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
        self.flush_deferred(false);
        self.processes.retain(out.iter().map(|s| s.pid));
        self.refresh_meters();
        out
    }

    fn session_peak(&mut self, session_id: &str) -> f32 {
        let Some(session) = self.sessions.get(session_id) else {
            return 0.0;
        };
        match self.meters.get(&session.pid) {
            Some(Meter::Running(meter)) => meter.peak(),
            _ => unsafe { session.meter.GetPeakValue() }.unwrap_or(0.0),
        }
    }

    fn set_session_volume(&mut self, session_id: &str, volume: f32) {
        match self.sessions.get(session_id) {
            Some(s) if s.idle => {
                self.deferred
                    .entry(session_id.to_owned())
                    .or_default()
                    .volume = Some(volume);
            }
            Some(s) => self.writer.set_volume(session_id, &s.volume, volume),
            None => {}
        }
    }

    fn set_session_mute(&mut self, session_id: &str, muted: bool) {
        match self.sessions.get(session_id) {
            Some(s) if s.idle => {
                self.deferred
                    .entry(session_id.to_owned())
                    .or_default()
                    .muted = Some(muted);
            }
            Some(s) => self.writer.set_mute(session_id, &s.volume, muted),
            None => {}
        }
    }

    fn output_devices(&mut self) -> Vec<DeviceInfo> {
        let mut devices: Vec<DeviceInfo> = self
            .devices
            .iter()
            .map(|(id, d)| DeviceInfo {
                id: id.clone(),
                name: d.name.clone(),
            })
            .collect();
        devices.sort_by(|a, b| a.name.cmp(&b.name));
        devices
    }

    fn routing_supported(&mut self) -> bool {
        self.policy.is_some()
    }

    fn set_app_output(&mut self, pid: u32, device_id: Option<&str>) -> bool {
        let Some(policy) = &self.policy else {
            return false;
        };
        match policy.set_app_output(pid, device_id) {
            Ok(()) => true,
            Err(e) => {
                log::warn!("set output for pid {pid}: {e}");
                false
            }
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
        self.flush_deferred(true);
        self.policy = None;
        self.meters.clear();
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

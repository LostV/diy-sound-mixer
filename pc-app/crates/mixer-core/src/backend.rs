/// One audio session (one app playing on one output device).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionInfo {
    /// Unique per session instance; stable while the session lives.
    pub id: String,
    pub pid: u32,
    /// File name of the executable, e.g. `discord.exe`.
    pub process_name: String,
    pub exe_path: Option<String>,
    /// Human friendly name, e.g. `Discord`.
    pub display_name: String,
    pub is_system_sounds: bool,
    pub device_name: String,
}

/// The microphone (default capture device).
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureInfo {
    pub device_id: String,
    pub device_name: String,
    /// 0.0..=1.0
    pub volume: f32,
    pub muted: bool,
}

/// An active output device.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    /// The OS endpoint id. Can change after driver updates.
    pub id: String,
    pub name: String,
}

/// Access to the operating system's audio sessions and microphone.
///
/// All methods are called from the mixer host thread only. Implementations
/// should swallow transient errors (a session vanishing mid-call is normal)
/// and log them instead of failing.
pub trait AudioBackend {
    /// Full rescan of all active playback sessions on all output devices.
    fn scan_sessions(&mut self) -> Vec<SessionInfo>;
    /// Current peak level 0.0..=1.0 of a session found by the last scan.
    fn session_peak(&mut self, session_id: &str) -> f32;
    fn set_session_volume(&mut self, session_id: &str, volume: f32);
    fn set_session_mute(&mut self, session_id: &str, muted: bool);

    /// Active output devices as of the last [`Self::scan_sessions`].
    fn output_devices(&mut self) -> Vec<DeviceInfo>;
    /// Whether [`Self::set_app_output`] can work on this system.
    fn routing_supported(&mut self) -> bool;
    /// Sends a process's audio to `device_id`, or back to the Windows default
    /// with `None`. Returns false if the OS refused.
    fn set_app_output(&mut self, pid: u32, device_id: Option<&str>) -> bool;

    /// Rescan the microphone: the first active capture device whose name
    /// contains `preferred` (case insensitive), else the Windows default.
    /// `None` if there is no microphone at all.
    fn scan_capture(&mut self, preferred: Option<&str>) -> Option<CaptureInfo>;
    /// Names of all active capture devices, for picking the mic in a UI.
    fn capture_devices(&mut self) -> Vec<String>;
    fn capture_peak(&mut self) -> f32;
    fn set_capture_volume(&mut self, volume: f32);
    fn set_capture_mute(&mut self, muted: bool);
}

impl<B: AudioBackend + ?Sized> AudioBackend for Box<B> {
    fn scan_sessions(&mut self) -> Vec<SessionInfo> {
        (**self).scan_sessions()
    }
    fn session_peak(&mut self, session_id: &str) -> f32 {
        (**self).session_peak(session_id)
    }
    fn set_session_volume(&mut self, session_id: &str, volume: f32) {
        (**self).set_session_volume(session_id, volume)
    }
    fn set_session_mute(&mut self, session_id: &str, muted: bool) {
        (**self).set_session_mute(session_id, muted)
    }
    fn output_devices(&mut self) -> Vec<DeviceInfo> {
        (**self).output_devices()
    }
    fn routing_supported(&mut self) -> bool {
        (**self).routing_supported()
    }
    fn set_app_output(&mut self, pid: u32, device_id: Option<&str>) -> bool {
        (**self).set_app_output(pid, device_id)
    }
    fn scan_capture(&mut self, preferred: Option<&str>) -> Option<CaptureInfo> {
        (**self).scan_capture(preferred)
    }
    fn capture_devices(&mut self) -> Vec<String> {
        (**self).capture_devices()
    }
    fn capture_peak(&mut self) -> f32 {
        (**self).capture_peak()
    }
    fn set_capture_volume(&mut self, volume: f32) {
        (**self).set_capture_volume(volume)
    }
    fn set_capture_mute(&mut self, muted: bool) {
        (**self).set_capture_mute(muted)
    }
}

/// A backend with no audio at all: keeps the app usable when the real one
/// fails to start, and on platforms without one.
pub struct NullBackend;

impl AudioBackend for NullBackend {
    fn scan_sessions(&mut self) -> Vec<SessionInfo> {
        Vec::new()
    }
    fn session_peak(&mut self, _: &str) -> f32 {
        0.0
    }
    fn set_session_volume(&mut self, _: &str, _: f32) {}
    fn set_session_mute(&mut self, _: &str, _: bool) {}
    fn output_devices(&mut self) -> Vec<DeviceInfo> {
        Vec::new()
    }
    fn routing_supported(&mut self) -> bool {
        false
    }
    fn set_app_output(&mut self, _: u32, _: Option<&str>) -> bool {
        false
    }
    fn scan_capture(&mut self, _: Option<&str>) -> Option<CaptureInfo> {
        None
    }
    fn capture_devices(&mut self) -> Vec<String> {
        Vec::new()
    }
    fn capture_peak(&mut self) -> f32 {
        0.0
    }
    fn set_capture_volume(&mut self, _: f32) {}
    fn set_capture_mute(&mut self, _: bool) {}
}

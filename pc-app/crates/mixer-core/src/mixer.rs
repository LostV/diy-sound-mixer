use std::collections::{BTreeSet, HashMap};

use crate::{
    backend::{AudioBackend, CaptureInfo, SessionInfo},
    config::{AppRule, ChannelKind, MixerConfig, RuleMatch},
    rules::{self, normalize_process_name},
    snapshot::{ChannelSnapshot, MixerSnapshot, SessionSnapshot},
};

/// How the user wants an app handled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assignment {
    /// No explicit rule: path rules and the default channel decide.
    Default,
    /// Leave the app alone.
    Uncontrolled,
    Channel(String),
}

impl Assignment {
    /// Parses the wire format used by UIs: `"default"`, `"none"` or a channel id.
    pub fn parse(value: &str) -> Self {
        match value {
            "default" => Self::Default,
            "none" => Self::Uncontrolled,
            id => Self::Channel(id.to_string()),
        }
    }
}

struct Tracked {
    info: SessionInfo,
    channel: Option<String>,
}

/// The mixer state machine. Single threaded; [`crate::MixerHost`] drives it.
///
/// Playback channels own their state: their volume/mute is pushed to every app
/// in the channel. The capture channel mirrors the microphone, so a mute done
/// elsewhere (headset button, Windows settings) shows up here.
pub struct Mixer<B: AudioBackend> {
    backend: B,
    config: MixerConfig,
    sessions: HashMap<String, Tracked>,
    peaks: HashMap<String, f32>,
    capture: Option<CaptureInfo>,
    capture_peak: f32,
    capture_devices: Vec<String>,
    sessions_version: u64,
    dirty: bool,
}

impl<B: AudioBackend> Mixer<B> {
    pub fn new(backend: B, config: MixerConfig) -> Self {
        Self {
            backend,
            config,
            sessions: HashMap::new(),
            peaks: HashMap::new(),
            capture: None,
            capture_peak: 0.0,
            capture_devices: Vec::new(),
            sessions_version: 0,
            dirty: false,
        }
    }

    pub fn config(&self) -> &MixerConfig {
        &self.config
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// True once since the last call if the persisted config changed.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Picks up new and vanished sessions and re-reads the microphone.
    pub fn refresh(&mut self) {
        let scanned = self.backend.scan_sessions();
        let mut changed = false;

        let live: BTreeSet<&str> = scanned.iter().map(|s| s.id.as_str()).collect();
        let before = self.sessions.len();
        self.sessions.retain(|id, _| live.contains(id.as_str()));
        self.peaks.retain(|id, _| live.contains(id.as_str()));
        changed |= self.sessions.len() != before;

        for info in scanned {
            if let Some(tracked) = self.sessions.get_mut(&info.id) {
                tracked.info = info;
                continue;
            }
            let channel = self.resolve(&info);
            let id = info.id.clone();
            self.sessions.insert(id.clone(), Tracked { info, channel });
            self.apply_session(&id);
            changed = true;
        }

        if changed {
            self.sessions_version += 1;
        }
        self.refresh_capture();
    }

    fn refresh_capture(&mut self) {
        self.capture_devices = self.backend.capture_devices();
        self.capture = self
            .backend
            .scan_capture(self.config.capture_device.as_deref());
        let Some(capture) = &self.capture else { return };
        let volume = to_percent(capture.volume);
        let muted = capture.muted;
        for channel in self
            .config
            .channels
            .iter_mut()
            .filter(|c| c.kind == ChannelKind::Capture)
        {
            channel.volume = volume;
            channel.muted = muted;
        }
    }

    pub fn update_meters(&mut self) {
        for id in self.sessions.keys() {
            let peak = self.backend.session_peak(id);
            self.peaks.insert(id.clone(), peak);
        }
        self.capture_peak = if self.capture.is_some() {
            self.backend.capture_peak()
        } else {
            0.0
        };
    }

    pub fn set_volume(&mut self, channel_id: &str, volume: u8) {
        let volume = volume.min(100);
        let Some(channel) = self.config.channel_mut(channel_id) else {
            return;
        };
        if channel.volume == volume {
            return;
        }
        channel.volume = volume;
        let kind = channel.kind;
        match kind {
            ChannelKind::Playback => {
                self.dirty = true;
                for id in self.session_ids_in(channel_id) {
                    self.backend.set_session_volume(&id, from_percent(volume));
                }
            }
            ChannelKind::Capture => self.backend.set_capture_volume(from_percent(volume)),
        }
    }

    /// One encoder step is `delta * volume_step` percent.
    pub fn adjust_volume(&mut self, channel_id: &str, steps: i32) {
        let Some(channel) = self.config.channel(channel_id) else {
            return;
        };
        let target = i32::from(channel.volume) + steps * i32::from(self.config.volume_step);
        self.set_volume(channel_id, target.clamp(0, 100) as u8);
    }

    pub fn set_mute(&mut self, channel_id: &str, muted: bool) {
        let Some(channel) = self.config.channel_mut(channel_id) else {
            return;
        };
        if channel.muted == muted {
            return;
        }
        channel.muted = muted;
        let kind = channel.kind;
        match kind {
            ChannelKind::Playback => {
                self.dirty = true;
                for id in self.session_ids_in(channel_id) {
                    self.backend.set_session_mute(&id, muted);
                }
            }
            ChannelKind::Capture => self.backend.set_capture_mute(muted),
        }
    }

    pub fn toggle_mute(&mut self, channel_id: &str) {
        if let Some(muted) = self.config.channel(channel_id).map(|c| c.muted) {
            self.set_mute(channel_id, !muted);
        }
    }

    /// `None` follows the Windows default microphone.
    pub fn set_capture_device(&mut self, name: Option<String>) {
        self.config.capture_device = name.filter(|n| !n.trim().is_empty());
        self.dirty = true;
        self.refresh_capture();
    }

    /// Stores an explicit rule for the app and moves its sessions right away.
    pub fn assign_app(&mut self, process_name: &str, assignment: Assignment) {
        let pattern = normalize_process_name(process_name);
        self.config.rules.retain(|r| {
            !(r.match_kind == RuleMatch::ProcessName
                && normalize_process_name(&r.pattern) == pattern)
        });
        let channel = match assignment {
            Assignment::Default => None,
            Assignment::Uncontrolled => Some(None),
            Assignment::Channel(id) => Some(Some(id)),
        };
        if let Some(channel) = channel {
            // Explicit rules go first so they win over the defaults' order.
            self.config.rules.insert(
                0,
                AppRule {
                    match_kind: RuleMatch::ProcessName,
                    pattern,
                    channel,
                },
            );
        }
        self.dirty = true;
        self.reassign_all();
    }

    /// Re-runs the rules for every session, e.g. after the rules changed.
    pub fn reassign_all(&mut self) {
        let ids: Vec<String> = self.sessions.keys().cloned().collect();
        let mut changed = false;
        for id in ids {
            let new_channel = self.resolve(&self.sessions[&id].info);
            let tracked = self.sessions.get_mut(&id).expect("id from keys");
            if tracked.channel == new_channel {
                continue;
            }
            tracked.channel = new_channel;
            changed = true;
            self.apply_session(&id);
        }
        if changed {
            self.sessions_version += 1;
        }
    }

    pub fn snapshot(&self) -> MixerSnapshot {
        let channels = self
            .config
            .channels
            .iter()
            .map(|c| {
                let (peak, available, apps) = match c.kind {
                    ChannelKind::Playback => {
                        let in_channel = self
                            .sessions
                            .values()
                            .filter(|t| t.channel.as_deref() == Some(&c.id));
                        let mut apps = BTreeSet::new();
                        let mut peak = 0.0f32;
                        for t in in_channel {
                            apps.insert(t.info.display_name.clone());
                            peak = peak.max(self.peaks.get(&t.info.id).copied().unwrap_or(0.0));
                        }
                        (peak, true, apps.into_iter().collect())
                    }
                    ChannelKind::Capture => match &self.capture {
                        Some(cap) => (self.capture_peak, true, vec![cap.device_name.clone()]),
                        None => (0.0, false, Vec::new()),
                    },
                };
                ChannelSnapshot {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    color: c.color.clone(),
                    kind: c.kind,
                    volume: c.volume,
                    muted: c.muted,
                    peak,
                    available,
                    apps,
                }
            })
            .collect();

        let mut sessions: Vec<SessionSnapshot> = self
            .sessions
            .values()
            .map(|t| SessionSnapshot {
                id: t.info.id.clone(),
                process_name: t.info.process_name.clone(),
                display_name: t.info.display_name.clone(),
                exe_path: t.info.exe_path.clone(),
                device_name: t.info.device_name.clone(),
                channel: t.channel.clone(),
                assignment: self.assignment_of(&t.info.process_name),
            })
            .collect();
        sessions.sort_by(|a, b| {
            a.display_name
                .to_lowercase()
                .cmp(&b.display_name.to_lowercase())
        });

        MixerSnapshot {
            channels,
            sessions,
            sessions_version: self.sessions_version,
            capture_devices: self.capture_devices.clone(),
            capture_device: self.config.capture_device.clone(),
        }
    }

    fn assignment_of(&self, process_name: &str) -> String {
        let name = normalize_process_name(process_name);
        self.config
            .rules
            .iter()
            .find(|r| {
                r.match_kind == RuleMatch::ProcessName && normalize_process_name(&r.pattern) == name
            })
            .map(|r| r.channel.clone().unwrap_or_else(|| "none".into()))
            .unwrap_or_else(|| "default".into())
    }

    fn resolve(&self, info: &SessionInfo) -> Option<String> {
        rules::resolve(
            &self.config,
            &info.process_name,
            info.exe_path.as_deref(),
            info.is_system_sounds,
        )
        .map(str::to_string)
    }

    fn session_ids_in(&self, channel_id: &str) -> Vec<String> {
        self.sessions
            .values()
            .filter(|t| t.channel.as_deref() == Some(channel_id))
            .map(|t| t.info.id.clone())
            .collect()
    }

    /// Pushes the channel state to one session. Sessions that leave the mixer
    /// get full volume back so they are never stuck quiet or muted.
    fn apply_session(&mut self, session_id: &str) {
        let Some(tracked) = self.sessions.get(session_id) else {
            return;
        };
        let (volume, muted) = match tracked
            .channel
            .as_deref()
            .and_then(|id| self.config.channel(id))
        {
            Some(channel) => (from_percent(channel.volume), channel.muted),
            None => (1.0, false),
        };
        self.backend.set_session_volume(session_id, volume);
        self.backend.set_session_mute(session_id, muted);
    }
}

fn from_percent(volume: u8) -> f32 {
    f32::from(volume.min(100)) / 100.0
}

fn to_percent(volume: f32) -> u8 {
    (volume.clamp(0.0, 1.0) * 100.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeBackend {
        sessions: Vec<SessionInfo>,
        volumes: HashMap<String, f32>,
        mutes: HashMap<String, bool>,
        capture: Option<CaptureInfo>,
        preferred_seen: Option<String>,
    }

    impl AudioBackend for FakeBackend {
        fn scan_sessions(&mut self) -> Vec<SessionInfo> {
            self.sessions.clone()
        }
        fn session_peak(&mut self, _: &str) -> f32 {
            0.5
        }
        fn set_session_volume(&mut self, id: &str, volume: f32) {
            self.volumes.insert(id.into(), volume);
        }
        fn set_session_mute(&mut self, id: &str, muted: bool) {
            self.mutes.insert(id.into(), muted);
        }
        fn scan_capture(&mut self, preferred: Option<&str>) -> Option<CaptureInfo> {
            self.preferred_seen = preferred.map(str::to_string);
            self.capture.clone()
        }
        fn capture_devices(&mut self) -> Vec<String> {
            self.capture.iter().map(|c| c.device_name.clone()).collect()
        }
        fn capture_peak(&mut self) -> f32 {
            0.25
        }
        fn set_capture_volume(&mut self, volume: f32) {
            if let Some(c) = &mut self.capture {
                c.volume = volume;
            }
        }
        fn set_capture_mute(&mut self, muted: bool) {
            if let Some(c) = &mut self.capture {
                c.muted = muted;
            }
        }
    }

    fn session(id: &str, process: &str) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            pid: 1,
            process_name: process.into(),
            exe_path: None,
            display_name: process.trim_end_matches(".exe").into(),
            is_system_sounds: false,
            device_name: "Headset".into(),
        }
    }

    fn mixer_with(sessions: Vec<SessionInfo>) -> Mixer<FakeBackend> {
        let backend = FakeBackend {
            sessions,
            ..Default::default()
        };
        let mut mixer = Mixer::new(backend, MixerConfig::default());
        mixer.set_volume("chat", 80);
        mixer.refresh();
        mixer
    }

    #[test]
    fn new_session_gets_channel_volume() {
        let mut m = mixer_with(vec![session("d", "discord.exe")]);
        assert_eq!(m.backend_mut().volumes["d"], 0.8);
        assert!(!m.backend_mut().mutes["d"]);
    }

    #[test]
    fn set_volume_only_touches_apps_in_channel() {
        let mut m = mixer_with(vec![
            session("d", "discord.exe"),
            session("s", "spotify.exe"),
        ]);
        m.set_volume("chat", 30);
        assert_eq!(m.backend_mut().volumes["d"], 0.3);
        assert_eq!(m.backend_mut().volumes["s"], 1.0);
    }

    #[test]
    fn adjust_volume_steps_and_clamps() {
        let mut m = mixer_with(vec![session("d", "discord.exe")]);
        m.adjust_volume("chat", 3);
        assert_eq!(m.config().channel("chat").unwrap().volume, 86);
        m.adjust_volume("chat", 100);
        assert_eq!(m.config().channel("chat").unwrap().volume, 100);
        m.adjust_volume("chat", -100);
        assert_eq!(m.config().channel("chat").unwrap().volume, 0);
    }

    #[test]
    fn toggle_mute_mutes_apps_in_channel() {
        let mut m = mixer_with(vec![session("d", "discord.exe")]);
        m.toggle_mute("chat");
        assert!(m.backend_mut().mutes["d"]);
        assert!(m.take_dirty());
    }

    #[test]
    fn assign_app_moves_session_and_applies_new_volume() {
        let mut m = mixer_with(vec![session("c", "chrome.exe")]);
        m.set_volume("game", 40);
        m.assign_app("chrome.exe", Assignment::Channel("game".into()));
        assert_eq!(m.backend_mut().volumes["c"], 0.4);
        let snap = m.snapshot();
        assert_eq!(snap.sessions[0].channel.as_deref(), Some("game"));
        assert_eq!(snap.sessions[0].assignment, "game");
    }

    #[test]
    fn uncontrolled_app_gets_full_volume_back() {
        let mut m = mixer_with(vec![session("d", "discord.exe")]);
        m.set_mute("chat", true);
        m.assign_app("discord.exe", Assignment::Uncontrolled);
        assert_eq!(m.backend_mut().volumes["d"], 1.0);
        assert!(!m.backend_mut().mutes["d"]);
        assert_eq!(m.snapshot().sessions[0].assignment, "none");
    }

    #[test]
    fn assign_default_removes_explicit_rule() {
        let mut m = mixer_with(vec![session("d", "discord.exe")]);
        m.assign_app("discord.exe", Assignment::Default);
        // The built-in discord rule is gone too, so it falls back to Media.
        assert_eq!(m.snapshot().sessions[0].channel.as_deref(), Some("media"));
    }

    #[test]
    fn vanished_sessions_are_dropped_and_version_bumps() {
        let mut m = mixer_with(vec![session("d", "discord.exe")]);
        let v = m.snapshot().sessions_version;
        m.backend_mut().sessions.clear();
        m.refresh();
        let snap = m.snapshot();
        assert!(snap.sessions.is_empty());
        assert!(snap.sessions_version > v);
    }

    #[test]
    fn channel_snapshot_lists_apps_and_peak() {
        let mut m = mixer_with(vec![session("d", "discord.exe")]);
        m.update_meters();
        let chat = m
            .snapshot()
            .channels
            .into_iter()
            .find(|c| c.id == "chat")
            .unwrap();
        assert_eq!(chat.apps, vec!["discord".to_string()]);
        assert_eq!(chat.peak, 0.5);
    }

    #[test]
    fn mic_channel_mirrors_device() {
        let mut m = mixer_with(vec![]);
        m.backend_mut().capture = Some(CaptureInfo {
            device_id: "m".into(),
            device_name: "A50 Mic".into(),
            volume: 0.7,
            muted: true,
        });
        m.refresh();
        let mic = m.config().channel("mic").unwrap();
        assert_eq!((mic.volume, mic.muted), (70, true));

        m.toggle_mute("mic");
        assert!(!m.backend_mut().capture.as_ref().unwrap().muted);
        m.set_volume("mic", 50);
        assert_eq!(m.backend_mut().capture.as_ref().unwrap().volume, 0.5);
    }

    #[test]
    fn preferred_mic_is_passed_to_backend_and_saved() {
        let mut m = mixer_with(vec![]);
        m.set_capture_device(Some("A50".into()));
        assert_eq!(m.backend_mut().preferred_seen.as_deref(), Some("A50"));
        assert_eq!(m.snapshot().capture_device.as_deref(), Some("A50"));
        assert!(m.take_dirty());

        m.set_capture_device(Some("  ".into()));
        assert_eq!(m.config().capture_device, None);
    }

    #[test]
    fn missing_mic_marks_channel_unavailable() {
        let m = mixer_with(vec![]);
        let mic = m
            .snapshot()
            .channels
            .into_iter()
            .find(|c| c.id == "mic")
            .unwrap();
        assert!(!mic.available);
    }
}

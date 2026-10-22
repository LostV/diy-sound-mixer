use std::collections::{BTreeSet, HashMap};

use crate::{
    backend::{AudioBackend, CaptureInfo, DeviceInfo, SessionInfo},
    config::{AppRule, ChannelKind, Language, MixerConfig, OutputDevice, RuleMatch},
    devices::find_device,
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
    /// Output device id we sent this session's process to, if any.
    routed_to: Option<String>,
}

/// Where a session's audio should go.
enum Route {
    /// The Windows default; undo any routing of ours.
    Default,
    Device(String),
    /// The chosen device is not connected: leave things as they are, so the
    /// app snaps back when it returns. Windows plays to the default meanwhile.
    Missing,
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
    output_devices: Vec<DeviceInfo>,
    routing_supported: bool,
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
            output_devices: Vec::new(),
            routing_supported: false,
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

    /// Picks up new and vanished sessions, re-applies routing where needed
    /// and re-reads the microphone.
    pub fn refresh(&mut self) {
        let scanned = self.backend.scan_sessions();
        self.output_devices = self.backend.output_devices();
        self.routing_supported = self.backend.routing_supported();
        self.update_output_ids();
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
            self.sessions.insert(
                id.clone(),
                Tracked {
                    info,
                    channel,
                    routed_to: None,
                },
            );
            self.apply_session(&id);
            changed = true;
        }

        if changed {
            self.sessions_version += 1;
        }
        // Cheap when nothing changed: only sessions whose target differs act.
        self.apply_routing_all();
        self.refresh_capture();
    }

    /// Driver updates can give a device a new id; keep the stored one current.
    fn update_output_ids(&mut self) {
        for channel in &mut self.config.channels {
            let Some(output) = &mut channel.output else {
                continue;
            };
            if let Some(found) = find_device(&self.output_devices, output)
                && output.id.as_deref() != Some(found.id.as_str())
            {
                output.id = Some(found.id.clone());
                self.dirty = true;
            }
        }
    }

    /// Sends the channel's apps to `device` (by name), or back to the
    /// Windows default with `None`.
    pub fn set_channel_output(&mut self, channel_id: &str, device: Option<String>) {
        let found_id = device
            .as_deref()
            .and_then(|name| self.output_devices.iter().find(|d| d.name == name))
            .map(|d| d.id.clone());
        let Some(channel) = self.config.channel_mut(channel_id) else {
            return;
        };
        if channel.kind != ChannelKind::Playback {
            return;
        }
        channel.output = device
            .filter(|n| !n.trim().is_empty())
            .map(|name| OutputDevice { name, id: found_id });
        self.dirty = true;
        self.apply_routing_all();
    }

    fn apply_routing_all(&mut self) {
        let ids: Vec<String> = self.sessions.keys().cloned().collect();
        for id in ids {
            self.apply_routing(&id);
        }
    }

    fn route_for(&self, tracked: &Tracked) -> Route {
        let output = tracked
            .channel
            .as_deref()
            .and_then(|id| self.config.channel(id))
            .and_then(|c| c.output.as_ref());
        match output {
            None => Route::Default,
            Some(wanted) => match find_device(&self.output_devices, wanted) {
                Some(device) => Route::Device(device.id.clone()),
                None => Route::Missing,
            },
        }
    }

    /// Makes Windows play the session's process where its channel wants.
    /// Only calls the OS when the target actually changed.
    fn apply_routing(&mut self, session_id: &str) {
        if !self.routing_supported {
            return;
        }
        let Some(tracked) = self.sessions.get(session_id) else {
            return;
        };
        if tracked.info.pid == 0 || tracked.info.is_system_sounds {
            return;
        }
        let route = self.route_for(tracked);
        let pid = tracked.info.pid;
        let process = normalize_process_name(&tracked.info.process_name);
        let routed_before =
            tracked.routed_to.is_some() || self.config.routed_processes.contains(&process);

        let new_target = match route {
            Route::Missing => return,
            Route::Device(id) if tracked.routed_to.as_deref() == Some(id.as_str()) => return,
            Route::Device(id) => Some(id),
            Route::Default if !routed_before => return,
            Route::Default => None,
        };

        if !self.backend.set_app_output(pid, new_target.as_deref()) {
            log::warn!("routing {process} (pid {pid}) failed");
        }
        // Recorded even on failure so a refusing OS isn't asked twice a second.
        let routed = new_target.is_some();
        if let Some(tracked) = self.sessions.get_mut(session_id) {
            tracked.routed_to = new_target;
        }
        let listed = self.config.routed_processes.contains(&process);
        if routed && !listed {
            self.config.routed_processes.push(process);
            self.dirty = true;
        } else if !routed && listed {
            self.config.routed_processes.retain(|p| *p != process);
            self.dirty = true;
        }
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
            self.apply_routing(&id);
        }
        if changed {
            self.sessions_version += 1;
        }
    }

    pub fn language(&self) -> Language {
        self.config.language.unwrap_or_default()
    }

    pub fn set_language(&mut self, language: Language) {
        if self.config.language != Some(language) {
            self.config.language = Some(language);
            self.dirty = true;
        }
    }

    pub fn snapshot(&self) -> MixerSnapshot {
        let language = self.language();
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
                    name: c.display_name(language),
                    color: c.color.clone(),
                    kind: c.kind,
                    volume: c.volume,
                    muted: c.muted,
                    peak,
                    available,
                    apps,
                    output: c.output.as_ref().map(|o| o.name.clone()),
                    output_available: c
                        .output
                        .as_ref()
                        .is_none_or(|o| find_device(&self.output_devices, o).is_some()),
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
            output_devices: self.output_devices.iter().map(|d| d.name.clone()).collect(),
            routing_supported: self.routing_supported,
            language,
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
        outputs: Vec<DeviceInfo>,
        /// Every `set_app_output` call, in order.
        routes: Vec<(u32, Option<String>)>,
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
        fn output_devices(&mut self) -> Vec<DeviceInfo> {
            self.outputs.clone()
        }
        fn routing_supported(&mut self) -> bool {
            true
        }
        fn set_app_output(&mut self, pid: u32, device_id: Option<&str>) -> bool {
            self.routes.push((pid, device_id.map(str::to_string)));
            true
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

    fn output(id: &str, name: &str) -> DeviceInfo {
        DeviceInfo {
            id: id.into(),
            name: name.into(),
        }
    }

    fn routed_mixer() -> Mixer<FakeBackend> {
        let mut discord = session("d", "discord.exe");
        discord.pid = 42;
        let mut m = mixer_with(vec![discord]);
        m.backend_mut().outputs = vec![
            output("game-id", "Headphones (A50 Game)"),
            output("voice-id", "Headset Earphone (A50 Voice)"),
        ];
        m.refresh();
        m
    }

    #[test]
    fn nothing_is_routed_until_a_channel_has_an_output() {
        let mut m = routed_mixer();
        assert!(m.backend_mut().routes.is_empty());
    }

    #[test]
    fn channel_output_routes_its_apps_once() {
        let mut m = routed_mixer();
        m.set_channel_output("chat", Some("Headset Earphone (A50 Voice)".into()));
        m.refresh();
        m.refresh();
        assert_eq!(
            m.backend_mut().routes,
            vec![(42, Some("voice-id".to_string()))]
        );
        assert_eq!(m.config().routed_processes, vec!["discord.exe".to_string()]);
        let chat = m
            .snapshot()
            .channels
            .into_iter()
            .find(|c| c.id == "chat")
            .unwrap();
        assert_eq!(chat.output.as_deref(), Some("Headset Earphone (A50 Voice)"));
        assert!(chat.output_available);
    }

    #[test]
    fn back_to_default_undoes_routing() {
        let mut m = routed_mixer();
        m.set_channel_output("chat", Some("Headset Earphone (A50 Voice)".into()));
        m.set_channel_output("chat", None);
        assert_eq!(m.backend_mut().routes.last(), Some(&(42, None)));
        assert!(m.config().routed_processes.is_empty());
    }

    #[test]
    fn moving_app_to_unrouted_channel_undoes_routing() {
        let mut m = routed_mixer();
        m.set_channel_output("chat", Some("Headset Earphone (A50 Voice)".into()));
        m.assign_app("discord.exe", Assignment::Channel("media".into()));
        assert_eq!(m.backend_mut().routes.last(), Some(&(42, None)));
    }

    #[test]
    fn routing_left_by_a_previous_run_is_undone() {
        let config = MixerConfig {
            routed_processes: vec!["discord.exe".into()],
            ..Default::default()
        };
        let mut discord = session("d", "discord.exe");
        discord.pid = 42;
        let backend = FakeBackend {
            sessions: vec![discord],
            ..Default::default()
        };
        let mut m = Mixer::new(backend, config);
        m.refresh();
        assert_eq!(m.backend_mut().routes, vec![(42, None)]);
        assert!(m.config().routed_processes.is_empty());
    }

    #[test]
    fn device_with_new_id_after_driver_update_is_found_by_name() {
        let mut m = routed_mixer();
        m.set_channel_output("chat", Some("Headset Earphone (A50 Voice)".into()));
        // Driver update: same device, new id and a Windows instance number.
        m.backend_mut().outputs = vec![output("voice-new", "Headset Earphone (2- A50 Voice)")];
        m.backend_mut().sessions[0].id = "d2".into(); // app restarted
        m.refresh();
        assert_eq!(
            m.backend_mut().routes.last(),
            Some(&(42, Some("voice-new".to_string())))
        );
        let stored = m.config().channel("chat").unwrap().output.clone().unwrap();
        assert_eq!(stored.id.as_deref(), Some("voice-new"));
        assert_eq!(stored.name, "Headset Earphone (A50 Voice)");
    }

    #[test]
    fn unplugged_output_leaves_routing_alone_and_shows_unavailable() {
        let mut m = routed_mixer();
        m.set_channel_output("chat", Some("Headset Earphone (A50 Voice)".into()));
        let calls = m.backend_mut().routes.len();
        m.backend_mut().outputs.clear();
        m.refresh();
        assert_eq!(m.backend_mut().routes.len(), calls);
        let chat = m
            .snapshot()
            .channels
            .into_iter()
            .find(|c| c.id == "chat")
            .unwrap();
        assert!(!chat.output_available);
    }

    #[test]
    fn mic_channel_ignores_output() {
        let mut m = routed_mixer();
        m.set_channel_output("mic", Some("Headphones (A50 Game)".into()));
        assert!(m.config().channel("mic").unwrap().output.is_none());
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

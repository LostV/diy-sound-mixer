use std::collections::{HashMap, HashSet};

use base64::Engine;
use mixer_core::{
    Assignment, ChannelKind, ChannelSnapshot, Command, Language, MixerSnapshot, SessionSnapshot,
};

use crate::protocol::{AppItem, AssignTarget, DeviceKind, DeviceMessage, HostMessage};

/// Identifies one connection, e.g. `sim` or a COM port name.
pub type LinkId = String;

/// Channel slots on the hardware (four knobs, four screen columns).
pub const SLOTS: usize = 4;

/// Edge length of the app icons sent to screens, in pixels.
pub const ICON_SIZE: u16 = 24;

/// An app icon as straight RGBA, from whatever can read icons on this OS.
pub struct IconRgba {
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
}

/// Looks up the icon for an executable path. Called once per app; the hub
/// caches the result.
pub type IconSource = Box<dyn FnMut(&str) -> Option<IconRgba> + Send>;

#[derive(Default)]
struct Link {
    /// `None` until the device said hello; nothing is sent before that.
    kind: Option<DeviceKind>,
    /// Last `ch` sent per slot, to only send changes.
    sent_ch: Vec<Option<HostMessage>>,
    sent_led: Vec<Option<bool>>,
    sent_lang: Option<Language>,
    sent_apps: Option<Vec<AppItem>>,
    sent_icons: HashSet<String>,
    sent_devices: Option<HostMessage>,
}

/// Translates between devices and the mixer. Knob `i` and screen column `i`
/// are the `i`-th channel in the config.
///
/// Pure state machine: transports feed it lines and send what it returns.
#[derive(Default)]
pub struct DeviceHub {
    links: HashMap<LinkId, Link>,
    /// Channel ids and kinds by slot, from the latest snapshot.
    slots: Vec<String>,
    slot_kinds: Vec<ChannelKind>,
    icon_source: Option<IconSource>,
    /// Encoded icon messages by executable path; `None` when it has no icon.
    icons: HashMap<String, Option<HostMessage>>,
}

impl DeviceHub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a connection. Returns what to send first (a hello query).
    pub fn connect(&mut self, link: &str) -> Vec<HostMessage> {
        self.links.insert(link.to_string(), Link::default());
        vec![HostMessage::HelloQuery]
    }

    pub fn disconnect(&mut self, link: &str) {
        self.links.remove(link);
    }

    pub fn device_kind(&self, link: &str) -> Option<DeviceKind> {
        self.links.get(link).and_then(|l| l.kind)
    }

    /// Enables app icons on screens.
    pub fn set_icon_source(&mut self, source: IconSource) {
        self.icon_source = Some(source);
        self.icons.clear();
    }

    /// Handles one line from a device and returns the mixer commands it maps to.
    /// Bad lines are logged and ignored; a flaky cable must not crash anything.
    pub fn handle_line(&mut self, link: &str, line: &str) -> Vec<Command> {
        if line.trim().is_empty() {
            return Vec::new();
        }
        match DeviceMessage::parse(line) {
            Ok(message) => self.handle(link, message),
            Err(e) => {
                log::warn!("{link}: bad line {line:?}: {e}");
                Vec::new()
            }
        }
    }

    pub fn handle(&mut self, link: &str, message: DeviceMessage) -> Vec<Command> {
        match message {
            DeviceMessage::Hello { dev, fw, proto } => {
                log::info!("{link}: {dev:?} firmware {fw}, protocol {proto}");
                // Fresh state: the next update resends everything.
                let entry = self.links.entry(link.to_string()).or_default();
                *entry = Link {
                    kind: Some(dev),
                    ..Default::default()
                };
                Vec::new()
            }
            DeviceMessage::Enc { ch, d } => self
                .channel_at(ch)
                .map(|channel| vec![Command::AdjustVolume { channel, steps: d }])
                .unwrap_or_default(),
            DeviceMessage::Btn { id, s } => {
                // Act on press only; release is reported for future long-press use.
                if s != 1 {
                    return Vec::new();
                }
                let slot = id
                    .strip_prefix("enc")
                    .or_else(|| id.strip_prefix("mute"))
                    .and_then(|n| n.parse().ok());
                slot.and_then(|ch| self.channel_at(ch))
                    .map(|channel| vec![Command::ToggleMute { channel }])
                    .unwrap_or_default()
            }
            DeviceMessage::Touch { action, ch } => match action.as_str() {
                "mute" => self
                    .channel_at(ch)
                    .map(|channel| vec![Command::ToggleMute { channel }])
                    .unwrap_or_default(),
                other => {
                    log::debug!("{link}: unknown touch action {other}");
                    Vec::new()
                }
            },
            DeviceMessage::Assign { app, to } => {
                let assignment = match to {
                    AssignTarget::Slot(slot) => match self.playback_at(slot) {
                        Some(channel) => Assignment::Channel(channel),
                        None => return Vec::new(),
                    },
                    AssignTarget::Keyword(word) => match word.as_str() {
                        "none" => Assignment::Uncontrolled,
                        "auto" => Assignment::Default,
                        other => {
                            log::debug!("{link}: unknown assign target {other}");
                            return Vec::new();
                        }
                    },
                };
                vec![Command::AssignApp {
                    process_name: app,
                    assignment,
                }]
            }
            DeviceMessage::Device { ch, name } => {
                let Some(channel) = self.channel_at(ch) else {
                    return Vec::new();
                };
                let name = Some(name).filter(|n| !n.is_empty());
                match self.slot_kinds.get(usize::from(ch)) {
                    Some(ChannelKind::Capture) => vec![Command::SetCaptureDevice { name }],
                    _ => vec![Command::SetChannelOutput {
                        channel,
                        device: name,
                    }],
                }
            }
        }
    }

    /// Messages for every greeted link, given a new mixer snapshot: changed
    /// channels and LEDs, plus meters for screens.
    pub fn update(&mut self, snapshot: &MixerSnapshot) -> Vec<(LinkId, Vec<HostMessage>)> {
        let channels: Vec<&ChannelSnapshot> = snapshot.channels.iter().take(SLOTS).collect();
        self.slots = channels.iter().map(|c| c.id.clone()).collect();
        self.slot_kinds = channels.iter().map(|c| c.kind).collect();

        // The app page data is only built when a screen that shows it is connected.
        let wants_apps = self
            .links
            .values()
            .any(|l| l.kind.is_some_and(DeviceKind::has_app_page));
        let (apps, icons, devices) = if wants_apps {
            let groups = app_groups(&snapshot.sessions);
            let icons: Vec<(String, Option<HostMessage>)> = groups
                .iter()
                .map(|g| (g.id.clone(), self.icon_for(g)))
                .collect();
            let apps: Vec<AppItem> = groups
                .iter()
                .zip(&icons)
                .map(|(g, (_, icon))| AppItem {
                    id: g.id.clone(),
                    name: g.name.clone(),
                    ch: g
                        .channel
                        .as_deref()
                        .and_then(|id| self.slots.iter().position(|s| s == id))
                        .map_or(-1, |slot| slot as i8),
                    auto: g.auto,
                    icon: icon.is_some(),
                })
                .collect();
            (apps, icons, Some(devices_message(snapshot, &channels)))
        } else {
            (Vec::new(), Vec::new(), None)
        };

        let mut out = Vec::new();
        for (id, link) in &mut self.links {
            let Some(kind) = link.kind else { continue };
            link.sent_ch.resize(channels.len(), None);
            link.sent_led.resize(channels.len(), None);
            let mut messages = Vec::new();

            if kind.has_screen() && link.sent_lang != Some(snapshot.language) {
                link.sent_lang = Some(snapshot.language);
                messages.push(HostMessage::Lang {
                    v: snapshot.language,
                });
            }
            for (i, channel) in channels.iter().enumerate() {
                if kind.has_screen() {
                    let ch = channel_message(i, channel);
                    if link.sent_ch[i].as_ref() != Some(&ch) {
                        link.sent_ch[i] = Some(ch.clone());
                        messages.push(ch);
                    }
                }
                if kind.has_leds() && link.sent_led[i] != Some(channel.muted) {
                    link.sent_led[i] = Some(channel.muted);
                    messages.push(HostMessage::Led {
                        id: i as u8,
                        on: channel.muted,
                    });
                }
            }
            if kind.has_app_page() {
                // Icons first, so the list that refers to them finds them.
                for (id, icon) in &icons {
                    if let Some(icon) = icon
                        && link.sent_icons.insert(id.clone())
                    {
                        messages.push(icon.clone());
                    }
                }
                if link.sent_apps.as_ref() != Some(&apps) {
                    link.sent_apps = Some(apps.clone());
                    messages.push(HostMessage::Apps { list: apps.clone() });
                }
                if link.sent_devices != devices {
                    link.sent_devices = devices.clone();
                    messages.extend(devices.clone());
                }
            }
            if kind.has_screen() {
                let v = channels
                    .iter()
                    .map(|c| if c.muted { 0 } else { meter_level(c.peak) })
                    .collect();
                messages.push(HostMessage::Meter { v });
            }
            out.push((id.clone(), messages));
        }
        out
    }

    fn channel_at(&self, slot: u8) -> Option<String> {
        self.slots.get(usize::from(slot)).cloned()
    }

    /// Apps can only go to playback channels, never the mic.
    fn playback_at(&self, slot: u8) -> Option<String> {
        match self.slot_kinds.get(usize::from(slot)) {
            Some(ChannelKind::Playback) => self.channel_at(slot),
            _ => None,
        }
    }

    fn icon_for(&mut self, app: &AppGroup) -> Option<HostMessage> {
        let path = app.exe_path.as_deref()?;
        if let Some(cached) = self.icons.get(path) {
            return cached.clone();
        }
        let source = self.icon_source.as_mut()?;
        let message = source(path).map(|icon| icon_message(&app.id, &icon));
        self.icons.insert(path.to_string(), message.clone());
        message
    }
}

/// One program on the app page; sessions on several devices count once.
struct AppGroup {
    id: String,
    name: String,
    exe_path: Option<String>,
    channel: Option<String>,
    auto: bool,
}

fn app_groups(sessions: &[SessionSnapshot]) -> Vec<AppGroup> {
    let mut groups: Vec<AppGroup> = Vec::new();
    for s in sessions {
        if s.process_name.is_empty() {
            continue; // system sounds can't be moved
        }
        if let Some(g) = groups
            .iter_mut()
            .find(|g| g.id.eq_ignore_ascii_case(&s.process_name))
        {
            g.channel = g.channel.take().or_else(|| s.channel.clone());
            continue;
        }
        groups.push(AppGroup {
            id: s.process_name.clone(),
            name: s.display_name.clone(),
            exe_path: s.exe_path.clone(),
            channel: s.channel.clone(),
            auto: s.assignment == "default",
        });
    }
    groups
}

fn devices_message(snapshot: &MixerSnapshot, channels: &[&ChannelSnapshot]) -> HostMessage {
    let sel = channels
        .iter()
        .map(|c| match c.kind {
            ChannelKind::Playback => c.output.clone().unwrap_or_default(),
            ChannelKind::Capture => snapshot.capture_device.clone().unwrap_or_default(),
        })
        .collect();
    HostMessage::Devices {
        out: snapshot.output_devices.clone(),
        inputs: snapshot.capture_devices.clone(),
        sel,
        routing: snapshot.routing_supported,
    }
}

/// RGBA → LVGL RGB565A8 (colour plane, then alpha plane), base64.
fn icon_message(id: &str, icon: &IconRgba) -> HostMessage {
    let pixels = usize::from(icon.width) * usize::from(icon.height);
    let mut data = Vec::with_capacity(pixels * 3);
    for px in icon.rgba.chunks_exact(4).take(pixels) {
        let rgb565 =
            (u16::from(px[0] >> 3) << 11) | (u16::from(px[1] >> 2) << 5) | u16::from(px[2] >> 3);
        data.extend_from_slice(&rgb565.to_le_bytes());
    }
    data.extend(icon.rgba.chunks_exact(4).take(pixels).map(|px| px[3]));
    HostMessage::Icon {
        id: id.to_string(),
        w: icon.width,
        h: icon.height,
        px: base64::engine::general_purpose::STANDARD.encode(data),
    }
}

fn channel_message(i: usize, c: &ChannelSnapshot) -> HostMessage {
    HostMessage::Ch {
        i: i as u8,
        name: c.name.clone(),
        vol: c.volume,
        mute: c.muted,
        color: c.color.clone(),
        sub: subtitle(&c.apps),
        avail: c.available,
        cap: c.kind == ChannelKind::Capture,
    }
}

/// "Discord", "Spotify, Chrome" or "Spotify, Chrome +2".
fn subtitle(apps: &[String]) -> String {
    match apps.len() {
        0 => String::new(),
        1 | 2 => apps.join(", "),
        n => format!("{}, {} +{}", apps[0], apps[1], n - 2),
    }
}

/// Peak 0..1 → 0..100 on a -60..0 dB scale.
pub fn meter_level(peak: f32) -> u8 {
    if peak <= 0.001 {
        return 0;
    }
    (((20.0 * peak.log10() + 60.0) / 60.0).clamp(0.0, 1.0) * 100.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use mixer_core::ChannelKind;

    fn channel(id: &str, volume: u8, muted: bool) -> ChannelSnapshot {
        ChannelSnapshot {
            id: id.into(),
            name: id.to_uppercase(),
            color: "#123456".into(),
            kind: ChannelKind::Playback,
            volume,
            muted,
            peak: 1.0,
            available: true,
            apps: vec!["Discord".into()],
            output: None,
            output_available: true,
        }
    }

    fn snapshot(channels: Vec<ChannelSnapshot>) -> MixerSnapshot {
        MixerSnapshot {
            channels,
            sessions: Vec::new(),
            sessions_version: 0,
            capture_devices: Vec::new(),
            capture_device: None,
            output_devices: Vec::new(),
            routing_supported: false,
            language: Language::En,
        }
    }

    fn greeted(kind: &str) -> DeviceHub {
        let mut hub = DeviceHub::new();
        hub.connect("x");
        hub.handle_line(
            "x",
            &format!(r#"{{"t":"hello","dev":"{kind}","fw":"test"}}"#),
        );
        hub.update(&snapshot(vec![
            channel("game", 50, false),
            channel("chat", 80, false),
        ]));
        hub
    }

    #[test]
    fn connect_asks_for_hello_and_sends_nothing_before_it() {
        let mut hub = DeviceHub::new();
        assert_eq!(hub.connect("x"), vec![HostMessage::HelloQuery]);
        assert!(
            hub.update(&snapshot(vec![channel("game", 50, false)]))
                .is_empty()
        );
    }

    #[test]
    fn encoder_adjusts_channel_in_that_slot() {
        let mut hub = greeted("knobs");
        assert!(matches!(
            hub.handle_line("x", r#"{"t":"enc","ch":1,"d":-2}"#).as_slice(),
            [Command::AdjustVolume { channel, steps: -2 }] if channel == "chat"
        ));
        assert!(
            hub.handle_line("x", r#"{"t":"enc","ch":7,"d":1}"#)
                .is_empty()
        );
    }

    #[test]
    fn button_press_toggles_mute_release_does_nothing() {
        let mut hub = greeted("knobs");
        assert!(matches!(
            hub.handle_line("x", r#"{"t":"btn","id":"enc0","s":1}"#).as_slice(),
            [Command::ToggleMute { channel }] if channel == "game"
        ));
        assert!(matches!(
            hub.handle_line("x", r#"{"t":"btn","id":"mute1","s":1}"#).as_slice(),
            [Command::ToggleMute { channel }] if channel == "chat"
        ));
        assert!(
            hub.handle_line("x", r#"{"t":"btn","id":"enc0","s":0}"#)
                .is_empty()
        );
        assert!(
            hub.handle_line("x", r#"{"t":"btn","id":"weird","s":1}"#)
                .is_empty()
        );
    }

    #[test]
    fn touch_mute_toggles_mute() {
        let mut hub = greeted("panel");
        assert!(matches!(
            hub.handle_line("x", r#"{"t":"touch","action":"mute","ch":0}"#).as_slice(),
            [Command::ToggleMute { channel }] if channel == "game"
        ));
    }

    #[test]
    fn bad_lines_are_ignored() {
        let mut hub = greeted("knobs");
        assert!(hub.handle_line("x", "garbage").is_empty());
        assert!(hub.handle_line("x", "   ").is_empty());
    }

    #[test]
    fn panel_gets_channels_once_then_only_changes() {
        let mut hub = DeviceHub::new();
        hub.connect("x");
        hub.handle_line("x", r#"{"t":"hello","dev":"panel","fw":"test"}"#);

        let first = hub.update(&snapshot(vec![
            channel("game", 50, false),
            channel("chat", 80, false),
        ]));
        let chs = |msgs: &[HostMessage]| {
            msgs.iter()
                .filter(|m| matches!(m, HostMessage::Ch { .. }))
                .count()
        };
        assert_eq!(chs(&first[0].1), 2);

        let same = hub.update(&snapshot(vec![
            channel("game", 50, false),
            channel("chat", 80, false),
        ]));
        assert_eq!(chs(&same[0].1), 0);
        assert!(matches!(same[0].1.last(), Some(HostMessage::Meter { .. })));

        let changed = hub.update(&snapshot(vec![
            channel("game", 52, false),
            channel("chat", 80, false),
        ]));
        assert!(matches!(
            changed[0].1.as_slice(),
            [
                HostMessage::Ch { i: 0, vol: 52, .. },
                HostMessage::Meter { .. }
            ]
        ));
    }

    #[test]
    fn hello_again_resends_everything() {
        let mut hub = greeted("panel");
        hub.handle_line("x", r#"{"t":"hello","dev":"panel","fw":"test"}"#);
        let out = hub.update(&snapshot(vec![
            channel("game", 50, false),
            channel("chat", 80, false),
        ]));
        // lang, two channels, apps, devices, meter
        assert_eq!(out[0].1.len(), 6);
    }

    #[test]
    fn screens_get_language_first_and_on_change() {
        let mut hub = DeviceHub::new();
        hub.connect("x");
        hub.handle_line("x", r#"{"t":"hello","dev":"panel","fw":"test"}"#);
        let mut snap = snapshot(vec![channel("game", 50, false)]);
        let first = hub.update(&snap);
        assert_eq!(first[0].1[0], HostMessage::Lang { v: Language::En });

        let again = hub.update(&snap);
        assert!(
            !again[0]
                .1
                .iter()
                .any(|m| matches!(m, HostMessage::Lang { .. }))
        );

        snap.language = Language::No;
        let changed = hub.update(&snap);
        assert_eq!(changed[0].1[0], HostMessage::Lang { v: Language::No });
    }

    #[test]
    fn knobs_get_leds_but_no_screen_data() {
        let mut hub = DeviceHub::new();
        hub.connect("x");
        hub.handle_line("x", r#"{"t":"hello","dev":"knobs","fw":"test"}"#);
        let out = hub.update(&snapshot(vec![channel("game", 50, true)]));
        assert_eq!(out[0].1, vec![HostMessage::Led { id: 0, on: true }]);
    }

    #[test]
    fn muted_channel_meter_is_zero() {
        let mut hub = DeviceHub::new();
        hub.connect("x");
        hub.handle_line("x", r#"{"t":"hello","dev":"sim","fw":"test"}"#);
        let out = hub.update(&snapshot(vec![
            channel("game", 50, true),
            channel("chat", 50, false),
        ]));
        assert!(out[0].1.contains(&HostMessage::Meter { v: vec![0, 100] }));
    }

    fn mic(id: &str) -> ChannelSnapshot {
        ChannelSnapshot {
            kind: ChannelKind::Capture,
            apps: vec!["A50 Mic".into()],
            ..channel(id, 100, false)
        }
    }

    fn session(
        process: &str,
        name: &str,
        channel: Option<&str>,
        assignment: &str,
    ) -> SessionSnapshot {
        SessionSnapshot {
            id: format!("{process}-{}", channel.unwrap_or("none")),
            process_name: process.into(),
            display_name: name.into(),
            exe_path: Some(format!(r"C:\{process}")),
            device_name: "Headset".into(),
            channel: channel.map(str::to_string),
            assignment: assignment.into(),
        }
    }

    fn app_snapshot() -> MixerSnapshot {
        let mut snap = snapshot(vec![
            channel("game", 50, false),
            channel("media", 50, false),
            channel("chat", 50, false),
            mic("mic"),
        ]);
        snap.sessions = vec![
            session("Discord.exe", "Discord", Some("chat"), "default"),
            session("Discord.exe", "Discord", Some("chat"), "default"), // second device
            session("steam.exe", "Steam", None, "none"),
            session("", "System sounds", None, "default"),
        ];
        snap.output_devices = vec!["Headphones (A50 Game)".into(), "Soundbar".into()];
        snap.capture_devices = vec!["A50 Mic".into()];
        snap.capture_device = Some("A50 Mic".into());
        snap.channels[1].output = Some("Soundbar".into());
        snap.routing_supported = true;
        snap
    }

    fn panel_hub() -> DeviceHub {
        let mut hub = DeviceHub::new();
        hub.set_icon_source(Box::new(|path| {
            path.ends_with("Discord.exe").then(|| IconRgba {
                width: 2,
                height: 1,
                rgba: vec![255, 0, 0, 255, 0, 0, 255, 128],
            })
        }));
        hub.connect("x");
        hub.handle_line("x", r#"{"t":"hello","dev":"panel","fw":"test"}"#);
        hub
    }

    #[test]
    fn panel_gets_icons_then_apps_then_devices() {
        let mut hub = panel_hub();
        let out = hub.update(&app_snapshot());
        let msgs = &out[0].1;
        let pos = |f: fn(&HostMessage) -> bool| msgs.iter().position(f).unwrap();
        let icon = pos(|m| matches!(m, HostMessage::Icon { .. }));
        let apps = pos(|m| matches!(m, HostMessage::Apps { .. }));
        assert!(icon < apps);

        let Some(HostMessage::Apps { list }) =
            msgs.iter().find(|m| matches!(m, HostMessage::Apps { .. }))
        else {
            unreachable!()
        };
        assert_eq!(
            list,
            &vec![
                AppItem {
                    id: "Discord.exe".into(),
                    name: "Discord".into(),
                    ch: 2,
                    auto: true,
                    icon: true
                },
                AppItem {
                    id: "steam.exe".into(),
                    name: "Steam".into(),
                    ch: -1,
                    auto: false,
                    icon: false
                },
            ]
        );
        assert!(msgs.contains(&HostMessage::Devices {
            out: vec!["Headphones (A50 Game)".into(), "Soundbar".into()],
            inputs: vec!["A50 Mic".into()],
            sel: vec!["".into(), "Soundbar".into(), "".into(), "A50 Mic".into()],
            routing: true,
        }));
    }

    #[test]
    fn icon_is_rgb565_then_alpha() {
        let mut hub = panel_hub();
        let out = hub.update(&app_snapshot());
        let Some(HostMessage::Icon { w, h, px, .. }) = out[0]
            .1
            .iter()
            .find(|m| matches!(m, HostMessage::Icon { .. }))
        else {
            unreachable!()
        };
        assert_eq!((*w, *h), (2, 1));
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(px)
            .unwrap();
        // red = 0xF800, blue = 0x001F (little endian), then alphas 255 and 128
        assert_eq!(bytes, vec![0x00, 0xF8, 0x1F, 0x00, 255, 128]);
    }

    #[test]
    fn app_page_data_is_sent_once_until_it_changes() {
        let mut hub = panel_hub();
        let mut snap = app_snapshot();
        hub.update(&snap);
        let again = hub.update(&snap);
        assert!(!again[0].1.iter().any(|m| matches!(
            m,
            HostMessage::Icon { .. } | HostMessage::Apps { .. } | HostMessage::Devices { .. }
        )));

        snap.sessions[2].channel = Some("media".into());
        let moved = hub.update(&snap);
        assert!(
            moved[0]
                .1
                .iter()
                .any(|m| matches!(m, HostMessage::Apps { .. }))
        );
        assert!(
            !moved[0]
                .1
                .iter()
                .any(|m| matches!(m, HostMessage::Icon { .. }))
        );
    }

    #[test]
    fn simulator_and_knobs_get_no_app_page_data() {
        for kind in ["sim", "knobs"] {
            let mut hub = DeviceHub::new();
            hub.connect("x");
            hub.handle_line("x", &format!(r#"{{"t":"hello","dev":"{kind}","fw":"t"}}"#));
            let out = hub.update(&app_snapshot());
            assert!(!out[0].1.iter().any(|m| matches!(
                m,
                HostMessage::Icon { .. } | HostMessage::Apps { .. } | HostMessage::Devices { .. }
            )));
        }
    }

    #[test]
    fn assign_maps_to_mixer_assignments() {
        let mut hub = panel_hub();
        hub.update(&app_snapshot());
        assert!(matches!(
            hub.handle_line("x", r#"{"t":"assign","app":"steam.exe","to":0}"#).as_slice(),
            [Command::AssignApp { process_name, assignment: Assignment::Channel(c) }]
                if process_name == "steam.exe" && c == "game"
        ));
        assert!(matches!(
            hub.handle_line("x", r#"{"t":"assign","app":"Discord.exe","to":"none"}"#)
                .as_slice(),
            [Command::AssignApp {
                assignment: Assignment::Uncontrolled,
                ..
            }]
        ));
        assert!(matches!(
            hub.handle_line("x", r#"{"t":"assign","app":"Discord.exe","to":"auto"}"#)
                .as_slice(),
            [Command::AssignApp {
                assignment: Assignment::Default,
                ..
            }]
        ));
        // The mic slot and unknown slots take no apps.
        assert!(
            hub.handle_line("x", r#"{"t":"assign","app":"x.exe","to":3}"#)
                .is_empty()
        );
        assert!(
            hub.handle_line("x", r#"{"t":"assign","app":"x.exe","to":9}"#)
                .is_empty()
        );
    }

    #[test]
    fn device_picks_output_or_microphone() {
        let mut hub = panel_hub();
        hub.update(&app_snapshot());
        assert!(matches!(
            hub.handle_line("x", r#"{"t":"device","ch":1,"name":"Soundbar"}"#).as_slice(),
            [Command::SetChannelOutput { channel, device: Some(d) }] if channel == "media" && d == "Soundbar"
        ));
        assert!(matches!(
            hub.handle_line("x", r#"{"t":"device","ch":1,"name":""}"#)
                .as_slice(),
            [Command::SetChannelOutput { device: None, .. }]
        ));
        assert!(matches!(
            hub.handle_line("x", r#"{"t":"device","ch":3,"name":"A50 Mic"}"#).as_slice(),
            [Command::SetCaptureDevice { name: Some(n) }] if n == "A50 Mic"
        ));
    }

    #[test]
    fn meter_level_uses_db_scale() {
        assert_eq!(meter_level(0.0), 0);
        assert_eq!(meter_level(1.0), 100);
        assert_eq!(meter_level(0.1), 67);
    }

    #[test]
    fn subtitle_shortens_long_lists() {
        assert_eq!(subtitle(&[]), "");
        assert_eq!(subtitle(&["A".into(), "B".into()]), "A, B");
        assert_eq!(
            subtitle(&["A".into(), "B".into(), "C".into(), "D".into()]),
            "A, B +2"
        );
    }
}

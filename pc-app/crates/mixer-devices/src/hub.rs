use std::collections::HashMap;

use mixer_core::{ChannelSnapshot, Command, MixerSnapshot};

use crate::protocol::{DeviceKind, DeviceMessage, HostMessage};

/// Identifies one connection, e.g. `sim` or a COM port name.
pub type LinkId = String;

/// Channel slots on the hardware (four knobs, four screen columns).
pub const SLOTS: usize = 4;

#[derive(Default)]
struct Link {
    /// `None` until the device said hello; nothing is sent before that.
    kind: Option<DeviceKind>,
    /// Last `ch` sent per slot, to only send changes.
    sent_ch: Vec<Option<HostMessage>>,
    sent_led: Vec<Option<bool>>,
}

/// Translates between devices and the mixer. Knob `i` and screen column `i`
/// are the `i`-th channel in the config.
///
/// Pure state machine: transports feed it lines and send what it returns.
#[derive(Default)]
pub struct DeviceHub {
    links: HashMap<LinkId, Link>,
    /// Channel ids by slot, from the latest snapshot.
    slots: Vec<String>,
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
        }
    }

    /// Messages for every greeted link, given a new mixer snapshot: changed
    /// channels and LEDs, plus meters for screens.
    pub fn update(&mut self, snapshot: &MixerSnapshot) -> Vec<(LinkId, Vec<HostMessage>)> {
        let channels: Vec<&ChannelSnapshot> = snapshot.channels.iter().take(SLOTS).collect();
        self.slots = channels.iter().map(|c| c.id.clone()).collect();

        let mut out = Vec::new();
        for (id, link) in &mut self.links {
            let Some(kind) = link.kind else { continue };
            link.sent_ch.resize(channels.len(), None);
            link.sent_led.resize(channels.len(), None);
            let mut messages = Vec::new();

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
        }
    }

    fn snapshot(channels: Vec<ChannelSnapshot>) -> MixerSnapshot {
        MixerSnapshot {
            channels,
            sessions: Vec::new(),
            sessions_version: 0,
            capture_devices: Vec::new(),
            capture_device: None,
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
        assert_eq!(out[0].1.len(), 3);
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

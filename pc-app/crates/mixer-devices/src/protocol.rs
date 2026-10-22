//! One JSON object per line. See `protocol/README.md` for the full spec.

use mixer_core::Language;
use serde::{Deserialize, Serialize};

/// Bumped on breaking changes; devices report theirs in `hello`.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    /// Pico 2 with encoders, buttons and mute LEDs.
    Knobs,
    /// CrowPanel screen.
    Panel,
    /// The in-app simulator: knobs and screen in one.
    Sim,
}

impl DeviceKind {
    pub fn has_screen(self) -> bool {
        matches!(self, Self::Panel | Self::Sim)
    }

    pub fn has_leds(self) -> bool {
        matches!(self, Self::Knobs | Self::Sim)
    }

    /// Gets the app list, icons and devices for routing from the screen.
    pub fn has_app_page(self) -> bool {
        matches!(self, Self::Panel)
    }
}

/// Where `assign` moves an app: a channel slot, `"none"` (leave it alone)
/// or `"auto"` (let the rules decide again).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AssignTarget {
    Slot(u8),
    Keyword(String),
}

/// One app on the screen's app page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppItem {
    /// The process name; what `assign` sends back.
    pub id: String,
    pub name: String,
    /// Channel slot the app is in, or -1 when it is not controlled.
    pub ch: i8,
    /// Placed by the rules rather than by the user.
    pub auto: bool,
    /// An `icon` message with this id has been sent.
    pub icon: bool,
}

/// Device → PC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum DeviceMessage {
    /// Sent on boot and as the answer to `hello?`.
    Hello {
        dev: DeviceKind,
        fw: String,
        #[serde(default = "default_proto")]
        proto: u32,
    },
    /// Encoder `ch` turned `d` detents; positive is clockwise.
    Enc { ch: u8, d: i32 },
    /// Button pressed (`s: 1`) or released (`s: 0`). Ids: `enc0`..`enc3` for
    /// the encoder push, `mute0`..`mute3` for the separate buttons.
    Btn { id: String, s: u8 },
    /// Touch on the screen. `action` is `mute` for now.
    Touch { action: String, ch: u8 },
    /// The user moved an app on the screen's app page.
    Assign { app: String, to: AssignTarget },
    /// The user picked a device for slot `ch`: the output of a playback
    /// channel or the microphone of the mic channel. `""` = Windows default.
    Device { ch: u8, name: String },
}

fn default_proto() -> u32 {
    PROTOCOL_VERSION
}

/// PC → device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum HostMessage {
    /// Asks the device to send `hello`.
    #[serde(rename = "hello?")]
    HelloQuery,
    /// Mute LED for channel `id`.
    Led { id: u8, on: bool },
    /// Full state of channel slot `i`; sent when anything in it changes.
    Ch {
        i: u8,
        name: String,
        /// 0..=100
        vol: u8,
        mute: bool,
        /// `#RRGGBB`
        color: String,
        /// Short subtitle: the apps in the channel, or the mic name.
        sub: String,
        /// False when the channel has nothing to control (no mic).
        avail: bool,
        /// The microphone channel: it takes no apps.
        #[serde(default)]
        cap: bool,
    },
    /// Meter levels per channel slot, 0..=100, already on a dB scale so the
    /// device can draw them directly.
    Meter { v: Vec<u8> },
    /// UI language for the device's own texts (`en` or `no`). Sent before
    /// the channels and again when it changes. Devices default to English.
    Lang { v: Language },
    /// The apps that play sound, for the screen's app page. Sent when it changes.
    Apps { list: Vec<AppItem> },
    /// An app icon, sent once per app before the `apps` that uses it.
    /// `px` is base64 of LVGL's RGB565A8: `w*h` little-endian RGB565 pixels,
    /// then `w*h` alpha bytes.
    Icon {
        id: String,
        w: u16,
        h: u16,
        px: String,
    },
    /// Device choices for the pickers on the app page. `sel[i]` is slot i's
    /// choice (`""` = Windows default): an output for playback channels, a
    /// microphone for the mic channel. `routing` is false when Windows refuses
    /// per-app outputs; the output pickers are then hidden.
    Devices {
        out: Vec<String>,
        #[serde(rename = "in")]
        inputs: Vec<String>,
        sel: Vec<String>,
        routing: bool,
    },
}

impl DeviceMessage {
    pub fn parse(line: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(line.trim())
    }
}

impl HostMessage {
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("host messages always serialize")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_device_messages() {
        assert_eq!(
            DeviceMessage::parse(r#"{"t":"hello","dev":"knobs","fw":"0.1.0"}"#).unwrap(),
            DeviceMessage::Hello {
                dev: DeviceKind::Knobs,
                fw: "0.1.0".into(),
                proto: PROTOCOL_VERSION
            }
        );
        assert_eq!(
            DeviceMessage::parse("{\"t\":\"enc\",\"ch\":2,\"d\":-1}\r\n").unwrap(),
            DeviceMessage::Enc { ch: 2, d: -1 }
        );
        assert_eq!(
            DeviceMessage::parse(r#"{"t":"btn","id":"enc0","s":1}"#).unwrap(),
            DeviceMessage::Btn {
                id: "enc0".into(),
                s: 1
            }
        );
    }

    #[test]
    fn parses_assign_targets() {
        assert_eq!(
            DeviceMessage::parse(r#"{"t":"assign","app":"Discord.exe","to":2}"#).unwrap(),
            DeviceMessage::Assign {
                app: "Discord.exe".into(),
                to: AssignTarget::Slot(2)
            }
        );
        assert_eq!(
            DeviceMessage::parse(r#"{"t":"assign","app":"x.exe","to":"none"}"#).unwrap(),
            DeviceMessage::Assign {
                app: "x.exe".into(),
                to: AssignTarget::Keyword("none".into())
            }
        );
        assert_eq!(
            DeviceMessage::parse(r#"{"t":"device","ch":1,"name":""}"#).unwrap(),
            DeviceMessage::Device {
                ch: 1,
                name: String::new()
            }
        );
    }

    #[test]
    fn devices_message_uses_in_as_key() {
        let line = HostMessage::Devices {
            out: vec!["A".into()],
            inputs: vec!["M".into()],
            sel: vec![String::new()],
            routing: true,
        }
        .to_line();
        assert_eq!(
            line,
            r#"{"t":"devices","out":["A"],"in":["M"],"sel":[""],"routing":true}"#
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(DeviceMessage::parse("hello").is_err());
        assert!(DeviceMessage::parse(r#"{"t":"nope"}"#).is_err());
    }

    #[test]
    fn serializes_host_messages() {
        assert_eq!(HostMessage::HelloQuery.to_line(), r#"{"t":"hello?"}"#);
        assert_eq!(
            HostMessage::Led { id: 1, on: true }.to_line(),
            r#"{"t":"led","id":1,"on":true}"#
        );
        assert_eq!(
            HostMessage::Meter { v: vec![0, 50] }.to_line(),
            r#"{"t":"meter","v":[0,50]}"#
        );
        assert_eq!(
            HostMessage::Lang { v: Language::No }.to_line(),
            r#"{"t":"lang","v":"no"}"#
        );
    }
}

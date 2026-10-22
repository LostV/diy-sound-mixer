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
    },
    /// Meter levels per channel slot, 0..=100, already on a dB scale so the
    /// device can draw them directly.
    Meter { v: Vec<u8> },
    /// UI language for the device's own texts (`en` or `no`). Sent before
    /// the channels and again when it changes. Devices default to English.
    Lang { v: Language },
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

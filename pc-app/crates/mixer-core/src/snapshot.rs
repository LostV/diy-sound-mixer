use serde::Serialize;

use crate::config::ChannelKind;

/// Everything a UI (settings window, screen, simulator) needs to draw the mixer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixerSnapshot {
    pub channels: Vec<ChannelSnapshot>,
    pub sessions: Vec<SessionSnapshot>,
    /// Bumped whenever the set of sessions or their channels change, so UIs
    /// can skip rebuilding lists on every meter update.
    pub sessions_version: u64,
    /// All microphones, and the configured preference (`None` = Windows default).
    pub capture_devices: Vec<String>,
    pub capture_device: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSnapshot {
    pub id: String,
    pub name: String,
    pub color: String,
    pub kind: ChannelKind,
    pub volume: u8,
    pub muted: bool,
    /// 0.0..=1.0, loudest app in the channel.
    pub peak: f32,
    /// False when a capture channel has no microphone.
    pub available: bool,
    /// Display names of the apps currently in the channel (or the mic name).
    pub apps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub id: String,
    pub process_name: String,
    pub display_name: String,
    pub exe_path: Option<String>,
    pub device_name: String,
    /// Channel the session is in right now; `None` when uncontrolled.
    pub channel: Option<String>,
    /// How the user assigned the app: `"default"`, `"none"` or a channel id.
    pub assignment: String,
}

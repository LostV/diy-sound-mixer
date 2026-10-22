use std::{fs, io, path::Path};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelKind {
    /// Controls the volume of the apps assigned to it.
    Playback,
    /// Controls the microphone.
    Capture,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConfig {
    pub id: String,
    pub name: String,
    pub color: String,
    pub kind: ChannelKind,
    /// 0..=100
    pub volume: u8,
    pub muted: bool,
    /// Where the channel's apps play. `None` leaves them on the Windows default.
    /// Only used by playback channels.
    #[serde(default)]
    pub output: Option<OutputDevice>,
}

/// A chosen output device. Found again by name if the id changes, see
/// [`crate::devices::find_device`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputDevice {
    pub name: String,
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleMatch {
    /// Exact executable file name, case insensitive (`discord.exe`).
    ProcessName,
    /// Substring of the full executable path, case insensitive.
    PathContains,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRule {
    #[serde(rename = "match")]
    pub match_kind: RuleMatch,
    pub pattern: String,
    /// Target channel id. `None` means the app is left alone.
    pub channel: Option<String>,
}

impl AppRule {
    pub fn process(pattern: &str, channel: &str) -> Self {
        Self {
            match_kind: RuleMatch::ProcessName,
            pattern: pattern.into(),
            channel: Some(channel.into()),
        }
    }

    pub fn path(pattern: &str, channel: &str) -> Self {
        Self {
            match_kind: RuleMatch::PathContains,
            pattern: pattern.into(),
            channel: Some(channel.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MixerConfig {
    pub channels: Vec<ChannelConfig>,
    pub rules: Vec<AppRule>,
    /// Channel for apps no rule matches. `None` leaves them alone.
    pub default_channel: Option<String>,
    pub control_system_sounds: bool,
    /// Percent per encoder detent.
    pub volume_step: u8,
    /// Part of the microphone's name, e.g. `A50`. `None` follows the Windows default.
    pub capture_device: Option<String>,
    /// Processes we have sent to a specific output. Windows remembers that
    /// across restarts, so we must know to undo it when the user changes
    /// their mind, even after this app restarted.
    pub routed_processes: Vec<String>,
}

impl Default for MixerConfig {
    fn default() -> Self {
        let channel = |id: &str, name: &str, color: &str, kind| ChannelConfig {
            id: id.into(),
            name: name.into(),
            color: color.into(),
            kind,
            volume: 100,
            muted: false,
            output: None,
        };
        Self {
            channels: vec![
                channel("game", "Spill", "#30A46C", ChannelKind::Playback),
                channel("media", "Media", "#F76B15", ChannelKind::Playback),
                channel("chat", "Chat", "#5865F2", ChannelKind::Playback),
                channel("mic", "Mic", "#E5484D", ChannelKind::Capture),
            ],
            rules: vec![
                AppRule::process("discord.exe", "chat"),
                AppRule::process("teamspeak.exe", "chat"),
                AppRule::process("ts3client_win64.exe", "chat"),
                AppRule::process("ms-teams.exe", "chat"),
                AppRule::process("zoom.exe", "chat"),
                AppRule::process("spotify.exe", "media"),
                AppRule::process("vlc.exe", "media"),
                AppRule::path(r"\steamapps\common\", "game"),
                AppRule::path(r"\Epic Games\", "game"),
                AppRule::path(r"\Riot Games\", "game"),
                AppRule::path(r"\Ubisoft Game Launcher\games\", "game"),
                AppRule::path(r"\EA Games\", "game"),
                AppRule::path(r"\XboxGames\", "game"),
            ],
            default_channel: Some("media".into()),
            control_system_sounds: false,
            volume_step: 2,
            capture_device: None,
            routed_processes: Vec::new(),
        }
    }
}

impl MixerConfig {
    /// Loads the config, falling back to defaults if the file is missing.
    pub fn load(path: &Path) -> io::Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(io::Error::other),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Writes atomically (temp file + rename) so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(
            &tmp,
            serde_json::to_string_pretty(self).map_err(io::Error::other)?,
        )?;
        fs::rename(tmp, path)
    }

    pub fn channel(&self, id: &str) -> Option<&ChannelConfig> {
        self.channels.iter().find(|c| c.id == id)
    }

    pub fn channel_mut(&mut self, id: &str) -> Option<&mut ChannelConfig> {
        self.channels.iter_mut().find(|c| c.id == id)
    }
}

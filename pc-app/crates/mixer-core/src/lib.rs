//! Platform independent core of the mixer: channels, app rules, mixer state
//! and the background host thread. Audio I/O goes through [`AudioBackend`].

pub mod backend;
pub mod config;
pub mod devices;
pub mod host;
pub mod mixer;
pub mod rules;
pub mod snapshot;

pub use backend::{AudioBackend, CaptureInfo, DeviceInfo, NullBackend, SessionInfo};
pub use config::{AppRule, ChannelConfig, ChannelKind, MixerConfig, OutputDevice, RuleMatch};
pub use host::{Command, MixerHost};
pub use mixer::{Assignment, Mixer};
pub use snapshot::{ChannelSnapshot, MixerSnapshot, SessionSnapshot};

//! The protocol spoken with the mixer hardware, and the hub that turns device
//! input into mixer commands and mixer state into device output.
//!
//! Transports (USB serial, the in-app simulator) only move lines of text; all
//! meaning lives here so every transport behaves the same.

pub mod hub;
pub mod protocol;

pub use hub::{DeviceHub, LinkId};
pub use protocol::{DeviceKind, DeviceMessage, HostMessage, PROTOCOL_VERSION};

//! Prints live peak levels per app for a few seconds, as the mixer sees them
//! (loopback meters, falling back to Windows' session meters).
//! Read only: never changes any volume.
//!
//!     cargo run -p mixer-wasapi --example meters

use std::{collections::BTreeMap, thread::sleep, time::Duration};

use mixer_core::AudioBackend;
use mixer_wasapi::WasapiBackend;

fn main() -> windows::core::Result<()> {
    let mut backend = WasapiBackend::new()?;
    let sessions = backend.scan_sessions();
    for _ in 0..12 {
        sleep(Duration::from_millis(300));
        // One value per app: the loudest of its sessions.
        let mut apps: BTreeMap<&str, f32> = BTreeMap::new();
        for s in &sessions {
            let peak = backend.session_peak(&s.id);
            let entry = apps.entry(&s.display_name).or_default();
            *entry = entry.max(peak);
        }
        let line: Vec<String> = apps
            .iter()
            .map(|(name, peak)| format!("{name}={peak:.2}"))
            .collect();
        println!("{}", line.join("  "));
    }
    Ok(())
}

//! Lists audio sessions, which channel the default rules put them in, and the mic.
//! Read only: never changes any volume.
//!
//!     cargo run -p mixer-wasapi --example scan [mic-name-part]

use mixer_core::{AudioBackend, MixerConfig, rules};
use mixer_wasapi::WasapiBackend;

fn main() -> windows::core::Result<()> {
    let mut backend = WasapiBackend::new()?;
    let config = MixerConfig::default();

    let sessions = backend.scan_sessions();
    std::thread::sleep(std::time::Duration::from_millis(200));
    println!("{} session(s):", sessions.len());
    for s in &sessions {
        let channel = rules::resolve(
            &config,
            &s.process_name,
            s.exe_path.as_deref(),
            s.is_system_sounds,
        );
        println!(
            "  {:<24} {:<22} peak {:.2}  -> {:<6}  [{}]  {}",
            s.display_name,
            s.process_name,
            backend.session_peak(&s.id),
            channel.unwrap_or("-"),
            s.device_name,
            s.exe_path.as_deref().unwrap_or(""),
        );
    }

    println!("microphones: {:?}", backend.capture_devices());
    let preferred = std::env::args().nth(1);
    match backend.scan_capture(preferred.as_deref()) {
        Some(mic) => println!(
            "mic: {} (volume {:.0}%, muted {})",
            mic.device_name,
            mic.volume * 100.0,
            mic.muted
        ),
        None => println!("mic: none"),
    }
    Ok(())
}

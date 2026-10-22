//! Checks that per-app output routing works on this Windows, without touching
//! any real app: it routes *this example's own process* (which plays no
//! audio) to the first output device, reads it back, and resets it.
//!
//!     cargo run -p mixer-wasapi --example routing

use mixer_core::AudioBackend;
use mixer_wasapi::WasapiBackend;

fn main() -> windows::core::Result<()> {
    let mut backend = WasapiBackend::new()?;
    backend.scan_sessions();
    let devices = backend.output_devices();
    println!(
        "outputs: {:?}",
        devices.iter().map(|d| &d.name).collect::<Vec<_>>()
    );
    println!("routing supported: {}", backend.routing_supported());

    let pid = std::process::id();
    let Some(device) = devices.first() else {
        println!("no output devices");
        return Ok(());
    };
    println!("before: {:?}", backend.app_output(pid));
    println!(
        "route own pid {pid} to {}: {}",
        device.name,
        backend.set_app_output(pid, Some(&device.id))
    );
    println!("after:  {:?}", backend.app_output(pid));
    println!("reset: {}", backend.set_app_output(pid, None));
    println!("final:  {:?}", backend.app_output(pid));
    Ok(())
}

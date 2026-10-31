//! Device connections over USB serial: the knob board (Pico) today, the
//! screen later. Ports come and go as cables are plugged in, so a background
//! thread looks for them every second and opens each new one as a link.
//!
//! Only ports with a known USB vendor id are opened. Opening a serial port
//! can disturb other hardware (it resets many Arduino boards, for example).

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use serialport::{SerialPortType, UsbPortInfo};
use tauri::AppHandle;

use crate::links::{self, Links};

/// USB vendor ids of the mixer hardware. 0x2E8A is Raspberry Pi (the Pico).
const DEVICE_VIDS: &[u16] = &[0x2E8A];

const SCAN_INTERVAL: Duration = Duration::from_secs(1);
/// Short, so the reader notices an unplugged cable quickly.
const READ_TIMEOUT: Duration = Duration::from_millis(200);

/// Watches for mixer hardware on a background thread for the app's lifetime.
pub fn watch(app: AppHandle, links: Links) {
    let spawned = thread::Builder::new()
        .name("serial-watcher".into())
        .spawn(move || {
            // Ports with a link running, and ports that refused to open (so a
            // port held by another program is logged once, not every second).
            let open = Arc::new(Mutex::new(HashSet::<String>::new()));
            let mut refused = HashSet::<String>::new();
            loop {
                let present = device_ports();
                refused.retain(|port| present.contains(port));
                for port in present {
                    if open.lock().expect("serial lock").contains(&port) {
                        continue;
                    }
                    match connect(&app, &links, &port, &open) {
                        Ok(()) => {
                            refused.remove(&port);
                        }
                        Err(e) => {
                            if refused.insert(port.clone()) {
                                log::warn!("serial:{port}: can't open: {e}");
                            }
                        }
                    }
                }
                thread::sleep(SCAN_INTERVAL);
            }
        });
    if let Err(e) = spawned {
        log::error!("starting serial watcher: {e}");
    }
}

/// Names of present serial ports that belong to mixer hardware.
fn device_ports() -> Vec<String> {
    serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .filter(|p| {
            matches!(&p.port_type, SerialPortType::UsbPort(UsbPortInfo { vid, .. })
                if DEVICE_VIDS.contains(vid))
        })
        .map(|p| p.port_name)
        .collect()
}

/// Opens `port` and runs it as a device link on its own thread.
fn connect(
    app: &AppHandle,
    links: &Links,
    port: &str,
    open: &Arc<Mutex<HashSet<String>>>,
) -> serialport::Result<()> {
    let mut reader = serialport::new(port, 115_200)
        .timeout(READ_TIMEOUT)
        .open()?;
    // USB CDC devices only send once the host raises DTR.
    reader.write_data_terminal_ready(true)?;
    let writer = reader.try_clone()?;

    open.lock().expect("serial lock").insert(port.to_string());
    let (app, links, open_ports, name) =
        (app.clone(), links.clone(), open.clone(), port.to_string());
    let spawned = thread::Builder::new()
        .name("device-link".into())
        .spawn(move || {
            links::run_link(&app, &links, &format!("serial:{name}"), reader, writer);
            open_ports.lock().expect("serial lock").remove(&name);
        });
    if let Err(e) = spawned {
        open.lock().expect("serial lock").remove(port);
        log::error!("serial:{port}: starting link: {e}");
    }
    Ok(())
}

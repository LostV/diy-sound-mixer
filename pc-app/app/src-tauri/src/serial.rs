//! Device connections over USB serial: the knob board (Pico) and the
//! screen (CrowPanel). Ports come and go as cables are plugged in, so a background
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

/// USB vendor ids of the mixer hardware. 0x2E8A is Raspberry Pi (the Pico),
/// 0x1A86 is WCH, maker of the CH340C USB-serial chip on the CrowPanel.
const DEVICE_VIDS: &[u16] = &[PICO_VID, CH340_VID];
const PICO_VID: u16 = 0x2E8A;
const CH340_VID: u16 = 0x1A86;

/// The Pico's USB CDC ignores the rate; the CrowPanel firmware uses this one.
const BAUD: u32 = 921_600;

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
                refused.retain(|port| present.iter().any(|(p, _)| p == port));
                for (port, vid) in present {
                    if open.lock().expect("serial lock").contains(&port) {
                        continue;
                    }
                    match connect(&app, &links, &port, vid, &open) {
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

/// Present serial ports that belong to mixer hardware, with their vendor id.
fn device_ports() -> Vec<(String, u16)> {
    serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| match p.port_type {
            SerialPortType::UsbPort(UsbPortInfo { vid, .. }) if DEVICE_VIDS.contains(&vid) => {
                Some((p.port_name, vid))
            }
            _ => None,
        })
        .collect()
}

/// Opens `port` and runs it as a device link on its own thread.
fn connect(
    app: &AppHandle,
    links: &Links,
    port: &str,
    vid: u16,
    open: &Arc<Mutex<HashSet<String>>>,
) -> serialport::Result<()> {
    let mut reader = serialport::new(port, BAUD).timeout(READ_TIMEOUT).open()?;
    if vid == CH340_VID {
        // DTR and RTS drive the ESP32's reset and boot pins through the
        // auto-download circuit. With both released it just keeps running.
        reader.write_data_terminal_ready(false)?;
        reader.write_request_to_send(false)?;
    } else {
        // USB CDC devices only send once the host raises DTR.
        reader.write_data_terminal_ready(true)?;
    }
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

//! Device connections over local TCP, for the desktop builds of the
//! firmware (the LVGL screen simulator) during development. Each connection
//! is one device link speaking the same line protocol as USB serial will.
//!
//! Only listens on 127.0.0.1, so nothing outside this PC can connect.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    },
    thread,
};

use mixer_devices::HostMessage;
use tauri::{AppHandle, Manager};

use crate::{Devices, Host};

/// Port the desktop firmware builds connect to.
pub const DEV_PORT: u16 = 47800;

/// Outgoing line queues per connected link.
#[derive(Clone, Default)]
pub struct Links(Arc<Mutex<HashMap<String, Sender<String>>>>);

impl Links {
    /// Queues messages for `link`; false if it is not one of ours.
    pub fn send(&self, link: &str, messages: &[HostMessage]) -> bool {
        let writers = self.0.lock().expect("links lock");
        let Some(tx) = writers.get(link) else {
            return false;
        };
        for message in messages {
            let _ = tx.send(message.to_line());
        }
        true
    }
}

/// Accepts connections on a background thread for the app's lifetime.
pub fn listen(app: AppHandle, links: Links) {
    let listener = match TcpListener::bind(("127.0.0.1", DEV_PORT)) {
        Ok(listener) => listener,
        Err(e) => {
            log::warn!("device port {DEV_PORT} unavailable, desktop devices can't connect: {e}");
            return;
        }
    };
    let spawned = thread::Builder::new()
        .name("device-listener".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let (app, links) = (app.clone(), links.clone());
                let _ = thread::Builder::new()
                    .name("device-link".into())
                    .spawn(move || serve(app, links, stream));
            }
        });
    if let Err(e) = spawned {
        log::error!("starting device listener: {e}");
    }
}

fn serve(app: AppHandle, links: Links, stream: TcpStream) {
    let link = match stream.peer_addr() {
        Ok(addr) => format!("tcp:{addr}"),
        Err(_) => return,
    };
    let _ = stream.set_nodelay(true);
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    log::info!("{link}: connected");

    // Writer thread: drains the queue so a slow device never blocks the mixer.
    let (tx, rx) = mpsc::channel::<String>();
    links
        .0
        .lock()
        .expect("links lock")
        .insert(link.clone(), tx);
    thread::spawn(move || {
        for line in rx {
            if writer.write_all(line.as_bytes()).is_err() || writer.write_all(b"\n").is_err() {
                break;
            }
        }
    });

    let devices = app.state::<Devices>();
    let hello = devices.0.lock().expect("devices lock").connect(&link);
    links.send(&link, &hello);

    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        let commands = devices
            .0
            .lock()
            .expect("devices lock")
            .handle_line(&link, &line);
        let host = app.state::<Host>();
        for command in commands {
            host.send(command);
        }
    }

    devices.0.lock().expect("devices lock").disconnect(&link);
    links.0.lock().expect("links lock").remove(&link);
    log::info!("{link}: disconnected");
}

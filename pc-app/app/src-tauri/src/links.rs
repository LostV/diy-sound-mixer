//! Device connections over local TCP, for the desktop builds of the
//! firmware (the LVGL screen simulator) during development. Each connection
//! is one device link speaking the same line protocol as USB serial.
//!
//! Only listens on 127.0.0.1, so nothing outside this PC can connect.

use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
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

/// Port the desktop firmware builds connect to. Dev builds of the app use
/// the next one, so they can run next to the installed app.
pub const DEV_PORT: u16 = if cfg!(debug_assertions) { 47801 } else { 47800 };

/// Outgoing line queues per connected link.
#[derive(Clone, Default)]
pub struct Links(Arc<Mutex<HashMap<String, Sender<Outgoing>>>>);

/// One queued line. Meters are replaced by newer ones, so they may be dropped.
struct Outgoing {
    line: String,
    meter: bool,
}

impl Links {
    /// Queues messages for `link`; false if it is not one of ours.
    pub fn send(&self, link: &str, messages: &[HostMessage]) -> bool {
        let writers = self.0.lock().expect("links lock");
        let Some(tx) = writers.get(link) else {
            return false;
        };
        for message in messages {
            let _ = tx.send(Outgoing {
                line: message.to_line(),
                meter: matches!(message, HostMessage::Meter { .. }),
            });
        }
        true
    }
}

/// The bytes for a batch of queued lines, keeping only the newest meter.
fn batch_bytes(batch: Vec<Outgoing>) -> Vec<u8> {
    let newest_meter = batch.iter().rposition(|out| out.meter);
    let mut bytes = Vec::new();
    for (i, out) in batch.into_iter().enumerate() {
        if out.meter && Some(i) != newest_meter {
            continue;
        }
        bytes.extend_from_slice(out.line.as_bytes());
        bytes.push(b'\n');
    }
    bytes
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
    let Ok(writer) = stream.try_clone() else {
        return;
    };
    run_link(&app, &links, &link, stream, writer);
}

/// Runs one device link until it closes: registers it with the hub, queues
/// outgoing lines to a writer thread and feeds incoming lines to the mixer.
/// Shared by every transport, so they all behave the same.
pub fn run_link(
    app: &AppHandle,
    links: &Links,
    link: &str,
    reader: impl Read,
    mut writer: impl Write + Send + 'static,
) {
    log::info!("{link}: connected");

    // Writer thread: drains the queue so a slow device never blocks the mixer.
    // It writes everything queued in one go: a flush takes tens of
    // milliseconds on the CrowPanel's CH340, so flushing every line let the
    // queue (30 meters a second) grow without end.
    let (tx, rx) = mpsc::channel::<Outgoing>();
    links
        .0
        .lock()
        .expect("links lock")
        .insert(link.to_string(), tx);
    thread::spawn(move || {
        while let Ok(first) = rx.recv() {
            let mut batch = vec![first];
            batch.extend(rx.try_iter());
            let bytes = batch_bytes(batch);
            if writer.write_all(&bytes).and_then(|()| writer.flush()).is_err() {
                break;
            }
        }
    });

    let devices = app.state::<Devices>();
    let hello = devices.0.lock().expect("devices lock").connect(link);
    links.send(link, &hello);

    read_lines(reader, |line| {
        let commands = devices
            .0
            .lock()
            .expect("devices lock")
            .handle_line(link, line);
        let host = app.state::<Host>();
        for command in commands {
            host.send(command);
        }
    });

    devices.0.lock().expect("devices lock").disconnect(link);
    links.0.lock().expect("links lock").remove(link);
    log::info!("{link}: disconnected");
}

/// Calls `on_line` for each line until the reader ends or fails. Read
/// timeouts are not failures: serial ports time out whenever the device is
/// quiet.
fn read_lines(mut reader: impl Read, mut on_line: impl FnMut(&str)) {
    let mut pending = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => n,
            Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::Interrupted) => continue,
            Err(_) => return,
        };
        pending.extend_from_slice(&buf[..n]);
        while let Some(end) = pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = pending.drain(..=end).collect();
            on_line(String::from_utf8_lossy(&line).trim_end());
        }
        // A device that never ends a line is broken; don't grow forever.
        if pending.len() > 64 * 1024 {
            pending.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    /// Hands out chunks one read at a time, like a serial port does.
    struct Chunks(Vec<io::Result<Vec<u8>>>);

    impl Read for Chunks {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() {
                return Ok(0);
            }
            let chunk = self.0.remove(0)?;
            buf[..chunk.len()].copy_from_slice(&chunk);
            Ok(chunk.len())
        }
    }

    fn lines(chunks: Vec<io::Result<Vec<u8>>>) -> Vec<String> {
        let mut out = Vec::new();
        read_lines(Chunks(chunks), |line| out.push(line.to_string()));
        out
    }

    #[test]
    fn joins_split_lines_and_strips_crlf() {
        assert_eq!(
            lines(vec![Ok(b"{\"t\":".to_vec()), Ok(b"1}\r\n{\"t\":2}\n".to_vec())]),
            vec![r#"{"t":1}"#, r#"{"t":2}"#]
        );
    }

    #[test]
    fn batch_keeps_every_line_but_only_the_newest_meter() {
        let out = |line: &str, meter| Outgoing { line: line.into(), meter };
        let bytes = batch_bytes(vec![
            out("m1", true),
            out("ch", false),
            out("m2", true),
            out("led", false),
        ]);
        assert_eq!(bytes, b"ch\nm2\nled\n");
    }

    #[test]
    fn timeouts_keep_reading_other_errors_stop() {
        let timeout = || Err(io::Error::from(ErrorKind::TimedOut));
        assert_eq!(
            lines(vec![
                Ok(b"a\n".to_vec()),
                timeout(),
                Ok(b"b\n".to_vec()),
                Err(io::Error::from(ErrorKind::BrokenPipe)),
                Ok(b"c\n".to_vec()),
            ]),
            vec!["a", "b"]
        );
    }
}

use std::{
    path::PathBuf,
    sync::mpsc::{self, RecvTimeoutError, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    backend::AudioBackend,
    config::MixerConfig,
    mixer::{Assignment, Mixer},
    snapshot::MixerSnapshot,
};

const METER_INTERVAL: Duration = Duration::from_millis(33);
const REFRESH_INTERVAL: Duration = Duration::from_millis(500);
const SAVE_DELAY: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub enum Command {
    SetVolume {
        channel: String,
        volume: u8,
    },
    /// Encoder detents; positive is clockwise.
    AdjustVolume {
        channel: String,
        steps: i32,
    },
    SetMute {
        channel: String,
        muted: bool,
    },
    ToggleMute {
        channel: String,
    },
    AssignApp {
        process_name: String,
        assignment: Assignment,
    },
    /// Part of the mic's name; `None` follows the Windows default.
    SetCaptureDevice {
        name: Option<String>,
    },
    Shutdown,
}

/// Runs a [`Mixer`] on its own thread.
///
/// The backend is created on that thread, so COM objects never cross threads.
/// Snapshots are delivered ~30 times a second; config changes are saved to
/// `config_path` a second after the last change and on shutdown.
pub struct MixerHost {
    tx: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl MixerHost {
    pub fn spawn<B, F, S>(
        make_backend: F,
        config: MixerConfig,
        config_path: Option<PathBuf>,
        on_snapshot: S,
    ) -> Self
    where
        B: AudioBackend + 'static,
        F: FnOnce() -> B + Send + 'static,
        S: Fn(MixerSnapshot) + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("mixer".into())
            .spawn(move || {
                let mut mixer = Mixer::new(make_backend(), config);
                mixer.refresh();
                let mut next_refresh = Instant::now() + REFRESH_INTERVAL;
                let mut next_meter = Instant::now();
                let mut save_at: Option<Instant> = None;

                loop {
                    let timeout = next_meter.saturating_duration_since(Instant::now());
                    match rx.recv_timeout(timeout) {
                        Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                        Ok(cmd) => handle(&mut mixer, cmd),
                        Err(RecvTimeoutError::Timeout) => {}
                    }

                    let now = Instant::now();
                    if now >= next_refresh {
                        mixer.refresh();
                        next_refresh = now + REFRESH_INTERVAL;
                    }
                    if now >= next_meter {
                        mixer.update_meters();
                        on_snapshot(mixer.snapshot());
                        next_meter = now + METER_INTERVAL;
                    }
                    if mixer.take_dirty() {
                        save_at = Some(now + SAVE_DELAY);
                    }
                    if save_at.is_some_and(|t| now >= t) {
                        save(&mixer, config_path.as_ref());
                        save_at = None;
                    }
                }
                if save_at.is_some() || mixer.take_dirty() {
                    save(&mixer, config_path.as_ref());
                }
            })
            .expect("failed to spawn mixer thread");

        Self {
            tx,
            thread: Some(thread),
        }
    }

    pub fn send(&self, command: Command) {
        // Only fails after shutdown, when there is nobody left to care.
        let _ = self.tx.send(command);
    }
}

impl Drop for MixerHost {
    fn drop(&mut self) {
        self.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn handle<B: AudioBackend>(mixer: &mut Mixer<B>, cmd: Command) {
    match cmd {
        Command::SetVolume { channel, volume } => mixer.set_volume(&channel, volume),
        Command::AdjustVolume { channel, steps } => mixer.adjust_volume(&channel, steps),
        Command::SetMute { channel, muted } => mixer.set_mute(&channel, muted),
        Command::ToggleMute { channel } => mixer.toggle_mute(&channel),
        Command::AssignApp {
            process_name,
            assignment,
        } => mixer.assign_app(&process_name, assignment),
        Command::SetCaptureDevice { name } => mixer.set_capture_device(name),
        Command::Shutdown => {}
    }
}

fn save<B: AudioBackend>(mixer: &Mixer<B>, path: Option<&PathBuf>) {
    let Some(path) = path else { return };
    if let Err(e) = mixer.config().save(path) {
        log::error!("saving config to {}: {e}", path.display());
    }
}

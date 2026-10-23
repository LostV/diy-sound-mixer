use std::{
    path::PathBuf,
    sync::mpsc::{self, RecvTimeoutError, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    backend::AudioBackend,
    config::{Language, MixerConfig},
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
    /// Output device name for a playback channel; `None` = Windows default.
    SetChannelOutput {
        channel: String,
        device: Option<String>,
    },
    SetLanguage {
        language: Language,
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

                'run: loop {
                    let timeout = next_meter.saturating_duration_since(Instant::now());
                    let first = match rx.recv_timeout(timeout) {
                        Ok(cmd) => Some(cmd),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => break,
                    };
                    // Take every queued command before the slow refresh below,
                    // or a fader drag (dozens of commands) would play back one
                    // per refresh, long after the user let go.
                    for cmd in first
                        .into_iter()
                        .chain(std::iter::from_fn(|| rx.try_recv().ok()))
                    {
                        if matches!(cmd, Command::Shutdown) {
                            break 'run;
                        }
                        handle(&mut mixer, cmd);
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
        Command::SetChannelOutput { channel, device } => mixer.set_channel_output(&channel, device),
        Command::SetLanguage { language } => mixer.set_language(language),
        Command::Shutdown => {}
    }
}

fn save<B: AudioBackend>(mixer: &Mixer<B>, path: Option<&PathBuf>) {
    let Some(path) = path else { return };
    if let Err(e) = mixer.config().save(path) {
        log::error!("saving config to {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{CaptureInfo, DeviceInfo, NullBackend, SessionInfo};

    /// Like the real backend in a debug build: a rescan takes longer than the
    /// refresh interval, so every loop round rescans.
    struct SlowScan(NullBackend);

    impl AudioBackend for SlowScan {
        fn scan_sessions(&mut self) -> Vec<SessionInfo> {
            thread::sleep(REFRESH_INTERVAL + Duration::from_millis(100));
            self.0.scan_sessions()
        }
        fn session_peak(&mut self, id: &str) -> f32 {
            self.0.session_peak(id)
        }
        fn set_session_volume(&mut self, id: &str, volume: f32) {
            self.0.set_session_volume(id, volume)
        }
        fn set_session_mute(&mut self, id: &str, muted: bool) {
            self.0.set_session_mute(id, muted)
        }
        fn output_devices(&mut self) -> Vec<DeviceInfo> {
            self.0.output_devices()
        }
        fn routing_supported(&mut self) -> bool {
            self.0.routing_supported()
        }
        fn set_app_output(&mut self, pid: u32, device_id: Option<&str>) -> bool {
            self.0.set_app_output(pid, device_id)
        }
        fn scan_capture(&mut self, preferred: Option<&str>) -> Option<CaptureInfo> {
            self.0.scan_capture(preferred)
        }
        fn capture_devices(&mut self) -> Vec<String> {
            self.0.capture_devices()
        }
        fn capture_peak(&mut self) -> f32 {
            self.0.capture_peak()
        }
        fn set_capture_volume(&mut self, volume: f32) {
            self.0.set_capture_volume(volume)
        }
        fn set_capture_mute(&mut self, muted: bool) {
            self.0.set_capture_mute(muted)
        }
    }

    #[test]
    fn fader_drag_lands_at_once_despite_slow_rescans() {
        let (tx, rx) = mpsc::channel();
        let host = MixerHost::spawn(
            || SlowScan(NullBackend),
            MixerConfig::default(),
            None,
            move |s| {
                let _ = tx.send(s);
            },
        );
        // Let the start-up scan finish so the loop is in its steady rhythm.
        thread::sleep(REFRESH_INTERVAL * 2);
        // A drag: a pointer move every 30 ms, much faster than a rescan.
        for volume in 1..=20 {
            host.send(Command::SetVolume {
                channel: "media".into(),
                volume,
            });
            thread::sleep(Duration::from_millis(30));
        }
        let start = Instant::now();
        let media = |s: &MixerSnapshot| {
            s.channels
                .iter()
                .find(|c| c.id == "media")
                .map(|c| c.volume)
        };
        while media(&rx.recv_timeout(Duration::from_secs(20)).expect("snapshot")) != Some(20) {}
        // Played back one per rescan this would take about 20 × 600 ms.
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "took {:?}",
            start.elapsed()
        );
    }
}

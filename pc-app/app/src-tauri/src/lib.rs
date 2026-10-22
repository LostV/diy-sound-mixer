use std::sync::Mutex;

use mixer_core::{Assignment, AudioBackend, Command, MixerConfig, MixerHost, NullBackend};
use tauri::{Emitter, Manager, RunEvent, State};

/// Taken out and dropped on exit so the host saves the config and stops cleanly.
struct Host(Mutex<Option<MixerHost>>);

impl Host {
    fn send(&self, command: Command) {
        if let Some(host) = self.0.lock().expect("host lock").as_ref() {
            host.send(command);
        }
    }
}

fn make_backend() -> Box<dyn AudioBackend> {
    #[cfg(windows)]
    match mixer_wasapi::WasapiBackend::new() {
        Ok(backend) => return Box::new(backend),
        Err(e) => log::error!("starting Windows audio backend: {e}"),
    }
    Box::new(NullBackend)
}

#[tauri::command]
fn set_volume(host: State<Host>, channel: String, volume: u8) {
    host.send(Command::SetVolume { channel, volume });
}

#[tauri::command]
fn adjust_volume(host: State<Host>, channel: String, steps: i32) {
    host.send(Command::AdjustVolume { channel, steps });
}

#[tauri::command]
fn toggle_mute(host: State<Host>, channel: String) {
    host.send(Command::ToggleMute { channel });
}

/// `assignment` is `"default"`, `"none"` or a channel id.
#[tauri::command]
fn assign_app(host: State<Host>, process_name: String, assignment: String) {
    host.send(Command::AssignApp {
        process_name,
        assignment: Assignment::parse(&assignment),
    });
}

#[tauri::command]
fn set_capture_device(host: State<Host>, name: Option<String>) {
    host.send(Command::SetCaptureDevice { name });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let config_path = app.path().app_config_dir()?.join("config.json");
            let config = MixerConfig::load(&config_path).unwrap_or_else(|e| {
                log::error!("loading {}: {e}; using defaults", config_path.display());
                MixerConfig::default()
            });
            let handle = app.handle().clone();
            let host = MixerHost::spawn(make_backend, config, Some(config_path), move |snapshot| {
                let _ = handle.emit("mixer-snapshot", snapshot);
            });
            app.manage(Host(Mutex::new(Some(host))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_volume,
            adjust_volume,
            toggle_mute,
            assign_app,
            set_capture_device
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            let host = app.state::<Host>().0.lock().expect("host lock").take();
            drop(host);
        }
    });
}

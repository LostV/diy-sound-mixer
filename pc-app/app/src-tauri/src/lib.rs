mod links;
mod tray;

use std::sync::{Arc, Mutex};

use mixer_core::{
    Assignment, AudioBackend, Command, Language, MixerConfig, MixerHost, MixerSnapshot,
    NullBackend,
};
use mixer_devices::{DeviceHub, HostMessage};
use tauri::{
    AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;

/// Passed by the autostart entry so the app starts in the tray.
const MINIMIZED_ARG: &str = "--minimized";
/// Link id of the in-app simulator in the device hub.
const SIM_LINK: &str = "sim";

/// Taken out and dropped on exit so the host saves the config and stops cleanly.
struct Host(Mutex<Option<MixerHost>>);

impl Host {
    fn send(&self, command: Command) {
        if let Some(host) = self.0.lock().expect("host lock").as_ref() {
            host.send(command);
        }
    }
}

/// Shared between the mixer thread (snapshots out) and commands (device input in).
#[derive(Clone, Default)]
struct Devices(Arc<Mutex<DeviceHub>>);

fn make_backend() -> Box<dyn AudioBackend> {
    #[cfg(windows)]
    match mixer_wasapi::WasapiBackend::new() {
        Ok(backend) => return Box::new(backend),
        Err(e) => log::error!("starting Windows audio backend: {e}"),
    }
    Box::new(NullBackend)
}

/// Runs on the mixer thread ~30 times a second.
fn publish(app: &AppHandle, devices: &Devices, links: &links::Links, snapshot: MixerSnapshot) {
    let outgoing = devices.0.lock().expect("devices lock").update(&snapshot);
    for (link, messages) in outgoing {
        if messages.is_empty() {
            continue;
        }
        if link == SIM_LINK {
            send_to_sim(app, &messages);
        } else {
            links.send(&link, &messages);
        }
    }
    tray::sync_language(app, snapshot.language);
    let _ = app.emit_to("main", "mixer-snapshot", snapshot);
}

fn send_to_sim(app: &AppHandle, messages: &[HostMessage]) {
    let lines: Vec<String> = messages.iter().map(HostMessage::to_line).collect();
    let _ = app.emit_to("simulator", "sim-host", lines);
}

pub(crate) fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub(crate) fn open_simulator(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("simulator") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built =
        WebviewWindowBuilder::new(app, "simulator", WebviewUrl::App("simulator.html".into()))
            .title("Simulator – DIY Sound Mixer")
            .inner_size(960.0, 860.0)
            .min_inner_size(720.0, 640.0)
            .build();
    match built {
        Ok(_) => {
            let hello = app
                .state::<Devices>()
                .0
                .lock()
                .expect("devices lock")
                .connect(SIM_LINK);
            send_to_sim(app, &hello);
        }
        Err(e) => log::error!("opening simulator: {e}"),
    }
}

pub(crate) fn set_autostart_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let launcher = app.autolaunch();
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    result.map_err(|e| e.to_string())?;
    tray::sync_autostart(app, enabled);
    Ok(())
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

/// `language` is `"en"` or `"no"`.
#[tauri::command]
fn set_language(host: State<Host>, language: Language) {
    host.send(Command::SetLanguage { language });
}

#[tauri::command]
fn set_capture_device(host: State<Host>, name: Option<String>) {
    host.send(Command::SetCaptureDevice { name });
}

/// `device` is an output device name; `None` = Windows default.
#[tauri::command]
fn set_channel_output(host: State<Host>, channel: String, device: Option<String>) {
    host.send(Command::SetChannelOutput { channel, device });
}

/// The app's icon as a `data:` URL, or `None` if it has none. Async so the
/// icon extraction runs off the main thread.
#[tauri::command]
async fn app_icon(path: String) -> Option<String> {
    #[cfg(windows)]
    {
        use base64::Engine;
        let png = mixer_wasapi::icon_png(&path, 48)?;
        Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    set_autostart_enabled(&app, enabled)
}

/// Window background: `acrylic` (blurred desktop), `mica` (Windows 11 app
/// material) or `none` (the app paints its own background).
#[tauri::command]
fn set_backdrop(window: tauri::WebviewWindow, kind: String) -> Result<(), String> {
    use tauri::window::{Effect, EffectsBuilder};
    let effect = match kind.as_str() {
        "acrylic" => Some(Effect::Acrylic),
        "mica" => Some(Effect::Mica),
        "none" => None,
        other => return Err(format!("unknown backdrop {other}")),
    };
    let result = match effect {
        Some(effect) => window.set_effects(EffectsBuilder::new().effect(effect).build()),
        None => window.set_effects(None),
    };
    result.map_err(|e| e.to_string())
}

/// The title bar's close button: keep running in the tray, like closing does.
#[tauri::command]
fn hide_main(window: tauri::WebviewWindow) {
    let _ = window.hide();
}

/// Async on purpose: creating a window from a synchronous command deadlocks
/// on Windows (the command runs on the thread that must build the window),
/// leaving a blank window that can't be closed.
#[tauri::command]
async fn show_simulator(app: AppHandle) {
    open_simulator(&app);
}

/// One protocol line from the simulator, exactly as real hardware would send it.
#[tauri::command]
fn sim_send(host: State<Host>, devices: State<Devices>, line: String) {
    let commands = devices
        .0
        .lock()
        .expect("devices lock")
        .handle_line(SIM_LINK, &line);
    for command in commands {
        host.send(command);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        // Must be first: a second launch just brings up the running one.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app)
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_ARG]),
        ))
        .setup(|app| {
            let config_path = app.path().app_config_dir()?.join("config.json");
            let mut config = MixerConfig::load(&config_path).unwrap_or_else(|e| {
                log::error!("loading {}: {e}; using defaults", config_path.display());
                MixerConfig::default()
            });
            // First start: follow the Windows language until the user picks one.
            let language = *config.language.get_or_insert_with(|| {
                Language::from_locale(&sys_locale::get_locale().unwrap_or_default())
            });

            let devices = Devices::default();
            let links = links::Links::default();
            app.manage(devices.clone());
            let handle = app.handle().clone();
            let publish_links = links.clone();
            let host = MixerHost::spawn(make_backend, config, Some(config_path), move |snapshot| {
                publish(&handle, &devices, &publish_links, snapshot);
            });
            app.manage(Host(Mutex::new(Some(host))));
            links::listen(app.handle().clone(), links);

            tray::build(app.handle(), language)?;
            // The autostart entry stores the exe path; rewrite it so it
            // follows the installed app rather than an old dev build.
            #[cfg(not(debug_assertions))]
            if app.autolaunch().is_enabled().unwrap_or(false) {
                if let Err(e) = app.autolaunch().enable() {
                    log::error!("refreshing autostart: {e}");
                }
            }
            if !std::env::args().any(|a| a == MINIMIZED_ARG) {
                show_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| match (window.label(), event) {
            // Closing the mixer window keeps the app running in the tray.
            ("main", WindowEvent::CloseRequested { api, .. }) => {
                api.prevent_close();
                let _ = window.hide();
            }
            ("simulator", WindowEvent::Destroyed) => {
                window
                    .state::<Devices>()
                    .0
                    .lock()
                    .expect("devices lock")
                    .disconnect(SIM_LINK);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            set_volume,
            adjust_volume,
            toggle_mute,
            assign_app,
            set_capture_device,
            set_channel_output,
            set_language,
            app_icon,
            get_autostart,
            set_autostart,
            show_simulator,
            set_backdrop,
            hide_main,
            sim_send
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

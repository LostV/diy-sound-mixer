use std::sync::Mutex;

use mixer_core::Language;
use tauri::{
    AppHandle, Manager, Wry,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_autostart::ManagerExt;

/// Kept so the check mark follows changes made from the settings window,
/// and the texts follow the language.
struct TrayItems {
    open: MenuItem<Wry>,
    autostart: CheckMenuItem<Wry>,
    quit: MenuItem<Wry>,
    language: Mutex<Language>,
}

/// Open, Start with Windows, Quit.
fn texts(language: Language) -> [&'static str; 3] {
    match language {
        Language::En => ["Open mixer", "Start with Windows", "Quit"],
        Language::No => ["Åpne mikser", "Start med Windows", "Avslutt"],
    }
}

pub fn build(app: &AppHandle, language: Language) -> tauri::Result<()> {
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let [open_text, autostart_text, quit_text] = texts(language);

    let open = MenuItem::with_id(app, "open", open_text, true, None::<&str>)?;
    let simulator = MenuItem::with_id(app, "simulator", "Simulator", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        autostart_text,
        true,
        autostart_on,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", quit_text, true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &simulator,
            &PredefinedMenuItem::separator(app)?,
            &autostart,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    app.manage(TrayItems {
        open,
        autostart,
        quit,
        language: Mutex::new(language),
    });

    TrayIconBuilder::with_id("main")
        // The flat 32 px icon: Windows scales it to 16-24 px for the tray,
        // where the glass window icon turns to mush.
        .icon(tauri::include_image!("icons/32x32.png"))
        .tooltip("DIY Sound Mixer")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => crate::show_main(app),
            "simulator" => crate::open_simulator(app),
            "autostart" => {
                let enable = !app.autolaunch().is_enabled().unwrap_or(false);
                if let Err(e) = crate::set_autostart_enabled(app, enable) {
                    log::error!("changing autostart: {e}");
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Updates the tray check mark to match the real autostart state.
pub fn sync_autostart(app: &AppHandle, enabled: bool) {
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.autostart.set_checked(enabled);
    }
}

/// Retranslates the menu; cheap to call with every snapshot.
pub fn sync_language(app: &AppHandle, language: Language) {
    let Some(items) = app.try_state::<TrayItems>() else {
        return;
    };
    let mut current = items.language.lock().expect("tray language lock");
    if *current == language {
        return;
    }
    *current = language;
    let [open, autostart, quit] = texts(language);
    let _ = items.open.set_text(open);
    let _ = items.autostart.set_text(autostart);
    let _ = items.quit.set_text(quit);
}

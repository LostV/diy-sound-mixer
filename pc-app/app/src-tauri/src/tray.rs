use tauri::{
    AppHandle, Manager, Wry,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_autostart::ManagerExt;

/// Kept so the check mark follows changes made from the settings window.
struct AutostartItem(CheckMenuItem<Wry>);

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);

    let open = MenuItem::with_id(app, "open", "Åpne mikser", true, None::<&str>)?;
    let simulator = MenuItem::with_id(app, "simulator", "Simulator", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start med Windows",
        true,
        autostart_on,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Avslutt", true, None::<&str>)?;
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
    app.manage(AutostartItem(autostart));

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
    if let Some(item) = app.try_state::<AutostartItem>() {
        let _ = item.0.set_checked(enabled);
    }
}

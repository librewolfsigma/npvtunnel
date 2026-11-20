//! System tray icon (Windows): closing the window keeps the app running in
//! the tray with connect-toggle and quit entries; non-Windows builds exit.
//! Clicks arrive on the process-global MenuEvent / TrayIconEvent channels,
//! polled from the eframe update loop (see `App::poll_tray`).

use tray_icon::TrayIcon;

/// Tray menu id: show/focus the main window.
pub const MENU_SHOW: &str = "npvt-show";
/// Tray menu id: connect or disconnect the active profile.
pub const MENU_TOGGLE: &str = "npvt-toggle";
/// Tray menu id: disconnect (if running) and exit the app.
pub const MENU_QUIT: &str = "npvt-quit";

/// Build the tray icon and menu. Returns `None` when the icon asset cannot
/// be decoded or the platform refuses to create the tray; the app then
/// behaves as if the feature did not exist (window close = exit).
#[cfg(windows)]
pub fn create() -> Option<TrayIcon> {
    use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tray_icon::TrayIconBuilder;

    let icon = match tray_image() {
        Some(icon) => icon,
        None => return None,
    };

    let menu = Menu::new();
    if menu
        .append(&MenuItem::with_id(
            MENU_SHOW,
            "Show NPVTunnel",
            true,
            None::<tray_icon::menu::Accelerator>,
        ))
        .is_err()
    {
        return None;
    }
    if menu
        .append(&MenuItem::with_id(
            MENU_TOGGLE,
            "Connect / Disconnect",
            true,
            None::<tray_icon::menu::Accelerator>,
        ))
        .is_err()
    {
        return None;
    }
    if menu
        .append(&PredefinedMenuItem::separator())
        .is_err()
    {
        return None;
    }
    if menu
        .append(&MenuItem::with_id(
            MENU_QUIT,
            "Quit NPVTunnel",
            true,
            None::<tray_icon::menu::Accelerator>,
        ))
        .is_err()
    {
        return None;
    }

    match TrayIconBuilder::new()
        .with_tooltip("NPVTunnel")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
    {
        Ok(tray) => Some(tray),
        Err(e) => {
            eprintln!("tray icon unavailable: {e}");
            None
        }
    }
}

/// Decode the bundled icon asset into a tray icon.
#[cfg(windows)]
fn tray_image() -> Option<tray_icon::Icon> {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png")).ok()?;
    let rgba = img.to_rgba8();
    tray_icon::Icon::from_rgba(rgba.into_raw(), rgba.width(), rgba.height()).ok()
}

//! All OS-specific glue lives here. `main.rs` / `lib.rs` stay platform-agnostic
//! so the Windows and Linux ports only ever touch this file.

use tauri::{tray::TrayIcon, App, Runtime};

/// Hide the Dock icon on macOS so the app lives only in the menu bar.
/// No-op on other platforms.
pub fn hide_from_dock(app: &mut App) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
    }
}

/// Render the tray icon as a monochrome template on macOS so it adapts to the
/// light/dark menu bar. No-op elsewhere.
pub fn make_tray_template<R: Runtime>(tray: &TrayIcon<R>) {
    #[cfg(target_os = "macos")]
    {
        let _ = tray.set_icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = tray;
    }
}

/// Default global shortcut to toggle the popover, per platform.
pub fn default_toggle_shortcut() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Alt+Shift+G"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Ctrl+Shift+G"
    }
}

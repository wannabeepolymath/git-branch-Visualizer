//! All OS-specific glue lives here. `main.rs` / `lib.rs` stay platform-agnostic
//! so the Windows and Linux ports only ever touch this file.

use tauri::{
    tray::{TrayIcon, TrayIconBuilder},
    App, Runtime, WebviewWindow,
};

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

/// Put the window where it belongs before showing it. Both callers of this
/// (`toggle_popover` and `recenter_window`) route through here so neither has to
/// know that `Position::TrayCenter` only works where the tray reports a position.
/// Best-effort: a window in the wrong place still beats no window.
pub fn anchor_window<R: Runtime>(window: &WebviewWindow<R>) {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_positioner::{Position, WindowExt};
        let _ = window.move_window(Position::TrayCenter);
    }
    // Elsewhere the tray never reports a position (see `build_tray`), so
    // TrayCenter would just fail — center on screen instead.
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window.center();
    }
}

/// Build the tray icon. The two platforms don't share an interaction model:
/// macOS toggles the popover on left-click, Linux can only offer a menu.
///
/// A tray host that doesn't exist (stock GNOME ships without one) is NOT an
/// error — the icon silently never appears and the app stays reachable through
/// its launcher, which is exactly why `window` is the default mode there.
pub fn build_tray<R: Runtime>(app: &mut App<R>) -> tauri::Result<TrayIcon<R>> {
    let builder = TrayIconBuilder::with_id(crate::MAIN_WINDOW)
        .icon(app.default_window_icon().unwrap().clone());

    #[cfg(target_os = "macos")]
    let builder = builder
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            use tauri::{
                tray::{MouseButton, MouseButtonState, TrayIconEvent},
                Manager,
            };
            // Cache the tray position so Position::TrayCenter works.
            tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = tray.app_handle().get_webview_window(crate::MAIN_WINDOW) {
                    crate::toggle_popover(&window);
                }
            }
        });

    // `tray-icon` 0.24: "Linux: Unsupported. The event is not emitted even though
    // the icon is shown". So there is no click to hook — a menu is the only way
    // in, and it's built once because on Linux "once a menu is set, it cannot be
    // removed". (It also makes the icon reliably visible.)
    #[cfg(target_os = "linux")]
    let builder = {
        use tauri::{
            menu::{Menu, MenuItem},
            Emitter, Manager,
        };
        let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
        let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
        let updates = MenuItem::with_id(app, "updates", "Check for updates", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&show, &settings, &updates, &quit])?;
        builder.menu(&menu).on_menu_event(|app, event| {
            if event.id() == "quit" {
                app.exit(0);
                return;
            }
            if let Some(window) = app.get_webview_window(crate::MAIN_WINDOW) {
                let _ = window.show();
                let _ = window.set_focus();
            }
            // Settings and the update check are UI, not backend: showing the
            // window is all the tray can do, the frontend does the rest.
            let _ = app.emit("tray-menu", event.id().as_ref());
        })
    };

    builder.build(app)
}

/// Whether a global shortcut can actually fire on this session.
pub fn shortcut_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        // `global-hotkey` 0.8 is X11-only. Under Wayland registration appears to
        // succeed and the hotkey never fires; an honest "unsupported" beats that.
        !is_wayland()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// Whether hiding the popover on blur can be undone. macOS always has the tray
/// icon to bring it back. On Linux it may be a one-way door: a Wayland session has
/// no global shortcut (X11-only) and may have no tray at all (GNOME dropped it in
/// 3.26), and the app has no single-instance handling, so relaunching from the
/// desktop entry spawns a second process instead of re-showing the first. Hiding
/// there would strand the window with no way back — including no way back to the
/// setting that turned popover mode on.
pub fn blur_dismiss_is_recoverable() -> bool {
    #[cfg(target_os = "macos")]
    {
        true
    }
    #[cfg(not(target_os = "macos"))]
    {
        shortcut_supported()
    }
}

/// Whether this build can install its own updates. The bundler only emits updater
/// artifacts for updater-enabled targets — on Linux that is AppImage only — so a
/// deb/rpm install would be served the AppImage entry from `latest.json` and fail
/// in `install_deb`. Offering a button that cannot work is worse than saying so.
pub fn can_self_update() -> bool {
    use tauri::utils::{config::BundleType, platform::bundle_type};
    !matches!(bundle_type(), Some(BundleType::Deb) | Some(BundleType::Rpm))
}

#[cfg(target_os = "linux")]
fn is_wayland() -> bool {
    is_wayland_from(std::env::var("XDG_SESSION_TYPE").ok().as_deref())
}

/// Split from the env read above so it's testable without a real session.
#[cfg(any(target_os = "linux", test))]
fn is_wayland_from(session_type: Option<&str>) -> bool {
    session_type.is_some_and(|s| s.eq_ignore_ascii_case("wayland"))
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

#[cfg(test)]
mod tests {
    use super::is_wayland_from;

    #[test]
    fn wayland_session_is_detected_case_insensitively() {
        assert!(is_wayland_from(Some("wayland")));
        assert!(is_wayland_from(Some("Wayland")));
        assert!(!is_wayland_from(Some("x11")));
        // Unset means we can't tell it's Wayland — assume the hotkey works.
        assert!(!is_wayland_from(None));
    }
}

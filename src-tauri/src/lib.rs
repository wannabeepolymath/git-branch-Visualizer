//! Platform-agnostic app wiring: tray, popover toggle, global shortcut.
//! Anything OS-specific is delegated to `platform`.

mod platform;

use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WebviewWindow, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_positioner::{Position, WindowExt};

const MAIN_WINDOW: &str = "main";

/// Show the popover anchored under the tray icon, or hide it if already shown.
fn toggle_popover(window: &WebviewWindow) {
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        let _ = window.move_window(Position::TrayCenter);
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_positioner::init())
        // Autostart is registered but NOT enabled by default.
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .on_window_event(|window, event| {
            // Dismiss the popover when it loses focus (blur).
            if let WindowEvent::Focused(false) = event {
                if window.label() == MAIN_WINDOW {
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            platform::hide_from_dock(app);

            // Tray icon: left-click toggles the popover under the icon.
            let tray = TrayIconBuilder::with_id(MAIN_WINDOW)
                .icon(app.default_window_icon().unwrap().clone())
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    // Cache the tray position so Position::TrayCenter works.
                    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(window) = tray.app_handle().get_webview_window(MAIN_WINDOW) {
                            toggle_popover(&window);
                        }
                    }
                })
                .build(app)?;
            platform::make_tray_template(&tray);

            // Global shortcut to toggle the popover from anywhere.
            {
                use tauri_plugin_global_shortcut::{Shortcut, ShortcutState};

                let shortcut: Shortcut = platform::default_toggle_shortcut().parse()?;
                let toggle = shortcut;
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_shortcut(shortcut)?
                        .with_handler(move |app, triggered, event| {
                            if triggered == &toggle && event.state() == ShortcutState::Pressed {
                                if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                                    toggle_popover(&window);
                                }
                            }
                        })
                        .build(),
                )?;
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

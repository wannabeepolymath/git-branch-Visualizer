//! Platform-agnostic app wiring: tray, popover toggle, global shortcut.
//! Anything OS-specific is delegated to `platform`.

mod commands;
mod git;
mod platform;
mod state;
mod watcher;

use state::AppState;
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
        // Anchor under the tray icon only the first time. Afterwards the window
        // reopens wherever the user last left it (its move/resize is preserved,
        // since the window is only hidden, never destroyed). The header's
        // "Recenter" button re-anchors on demand.
        let first_show = window
            .app_handle()
            .try_state::<AppState>()
            .is_none_or(|s| !s.positioned.swap(true, std::sync::atomic::Ordering::SeqCst));
        if first_show {
            let _ = window.move_window(Position::TrayCenter);
        }
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_dialog::init())
        // Autostart is registered but NOT enabled by default.
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::update_settings,
            commands::recenter_window,
            commands::pick_repo_folder,
            commands::add_repo,
            commands::remove_repo,
            commands::set_active_repo,
            commands::get_branches,
            commands::get_worktrees,
            commands::get_log,
            commands::get_commit,
            commands::get_status,
            commands::diff_file,
            commands::stage_files,
            commands::unstage_files,
            commands::discard_files,
            commands::checkout,
            commands::create_branch,
            commands::rename_branch,
            commands::delete_branch,
            commands::fetch_repo,
            commands::pull_repo,
        ])
        .on_window_event(|window, event| {
            // Dismiss the popover when it loses focus (blur) — unless the blur
            // was caused by a native dialog we opened (e.g. the folder picker).
            if let WindowEvent::Focused(false) = event {
                if window.label() == MAIN_WINDOW {
                    let dialog_open = window
                        .app_handle()
                        .try_state::<AppState>()
                        .map(|s| s.dialog_open.load(std::sync::atomic::Ordering::SeqCst))
                        .unwrap_or(false);
                    if !dialog_open {
                        let _ = window.hide();
                    }
                }
            }
        })
        .setup(|app| {
            platform::hide_from_dock(app);

            // Load persisted settings + repos into managed state.
            let config_path = app
                .path()
                .app_config_dir()
                .map_err(|e| e.to_string())?
                .join("settings.json");
            let app_state = AppState::load(config_path, platform::default_toggle_shortcut());
            let saved_shortcut = {
                let s = app_state.settings.lock().unwrap();
                s.shortcut.clone()
            };
            // Start watchers for every already-registered repo.
            {
                let s = app_state.settings.lock().unwrap();
                for repo in &s.repos {
                    let _ = watcher::start(app.handle(), &app_state, &repo.id, &repo.path);
                }
            }
            app.manage(app_state);

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

            // Global shortcut to toggle the popover from anywhere. We only ever
            // register the toggle shortcut, so the handler fires on any press —
            // which lets update_settings re-register a new shortcut at runtime.
            {
                use tauri_plugin_global_shortcut::{Shortcut, ShortcutState};

                let shortcut: Shortcut = saved_shortcut
                    .parse()
                    .or_else(|_| platform::default_toggle_shortcut().parse())?;
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_shortcut(shortcut)?
                        .with_handler(move |app, _triggered, event| {
                            if event.state() == ShortcutState::Pressed {
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

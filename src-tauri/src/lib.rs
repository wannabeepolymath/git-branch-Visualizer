//! Platform-agnostic app wiring: tray, popover toggle, global shortcut.
//! Anything OS-specific is delegated to `platform`.

mod commands;
mod git;
mod open;
mod platform;
mod state;
mod watcher;

use state::AppState;
use tauri::{Manager, Runtime, WebviewWindow, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

const MAIN_WINDOW: &str = "main";

/// The window mode is one window with four flags, not two windows. The fourth,
/// hide-on-blur, isn't a window API — the blur handler reads the setting itself.
fn apply_window_mode<R: Runtime>(window: &WebviewWindow<R>, mode: &str) {
    let popover = mode == "popover";
    let _ = window.set_decorations(!popover);
    let _ = window.set_always_on_top(popover);
    let _ = window.set_skip_taskbar(popover);
}

/// Show the popover anchored under the tray icon, or hide it if already shown.
fn toggle_popover<R: Runtime>(window: &WebviewWindow<R>) {
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
            platform::anchor_window(window);
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // Autostart is registered but NOT enabled by default.
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::quit,
            commands::update_settings,
            commands::recenter_window,
            commands::pick_repo_folder,
            commands::add_repo,
            commands::remove_repo,
            commands::set_active_repo,
            commands::get_branches,
            commands::get_worktrees,
            commands::open_worktree,
            commands::get_log,
            commands::get_commit,
            commands::get_status,
            commands::diff_file,
            commands::diff_commit_file,
            commands::stage_files,
            commands::unstage_files,
            commands::discard_files,
            commands::checkout,
            commands::create_branch,
            commands::rename_branch,
            commands::delete_branch,
            commands::fetch_repo,
            commands::pull_repo,
            commands::push_branch,
            commands::get_platform_info,
        ])
        .on_window_event(|window, event| {
            if window.label() != MAIN_WINDOW {
                return;
            }
            match event {
                WindowEvent::Focused(false) => {
                    // Only the popover dismisses itself on blur — and not when the
                    // blur came from a native dialog we opened (the folder picker
                    // steals focus). In "window" mode clicking away leaves the
                    // window alone, like any ordinary desktop app.
                    let hide = platform::blur_dismiss_is_recoverable()
                        && window.app_handle().try_state::<AppState>().is_some_and(|s| {
                            !s.dialog_open.load(std::sync::atomic::Ordering::SeqCst)
                                && s.settings.lock().is_ok_and(|g| g.window_mode == "popover")
                        });
                    if hide {
                        let _ = window.hide();
                    }
                }
                // "window" mode has a titlebar close button, which quits. Hiding
                // instead is the trap: on a desktop with no tray and no hotkey
                // that strands the app running, invisible and unquittable.
                // The popover has no decorations, so it never gets here.
                WindowEvent::CloseRequested { .. } => window.app_handle().exit(0),
                _ => {}
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
            let (saved_shortcut, window_mode) = {
                let s = app_state.settings.lock().unwrap();
                (s.shortcut.clone(), s.window_mode.clone())
            };
            // The OS is the source of truth for autostart: update_settings only
            // writes it when the checkbox changes, so a LaunchAgent removed outside
            // the app would leave the checkbox lying forever. Best-effort.
            {
                use tauri_plugin_autostart::ManagerExt;
                if let Ok(enabled) = app.autolaunch().is_enabled() {
                    let mut s = app_state.settings.lock().unwrap();
                    if s.launch_at_login != enabled {
                        s.launch_at_login = enabled;
                        let _ = state::persist(&app_state.config_path, &s);
                    }
                }
            }
            // Start watchers for every already-registered repo.
            {
                let s = app_state.settings.lock().unwrap();
                for repo in &s.repos {
                    let _ = watcher::start(app.handle(), &app_state, &repo.id, &repo.path);
                }
            }
            app.manage(app_state);

            // The window is created hidden and popover-shaped (tauri.conf.json).
            // In "window" mode it's an ordinary app instead, so launching it has
            // to actually put a window on screen — that launcher entry is the
            // primary way in wherever there's no tray.
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                apply_window_mode(&window, &window_mode);
                if window_mode != "popover" {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }

            let tray = platform::build_tray(app)?;
            platform::make_tray_template(&tray);

            // Global shortcut to toggle the popover from anywhere. We only ever
            // register the toggle shortcut, so the handler fires on any press —
            // which lets update_settings re-register a new shortcut at runtime.
            {
                use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(move |app, _triggered, event| {
                            if event.state() == ShortcutState::Pressed {
                                if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                                    toggle_popover(&window);
                                }
                            }
                        })
                        .build(),
                )?;
                // Register after plugin init: a saved combo that another app now
                // owns falls back to the default shortcut instead of failing the
                // whole app launch (registration best-effort either way — the
                // tray icon still works without a hotkey).
                let gs = app.handle().global_shortcut();
                let shortcut: Shortcut = saved_shortcut
                    .parse()
                    .or_else(|_| platform::default_toggle_shortcut().parse())?;
                if gs.register(shortcut).is_err() {
                    let _ = gs.register(platform::default_toggle_shortcut().parse::<Shortcut>()?);
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

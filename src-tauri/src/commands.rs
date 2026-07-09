//! Tauri commands — the IPC surface for the frontend.
//! Git-invoking commands are `async` (they call blocking git directly, which is
//! fine for this app's scale). Errors are human-readable strings.
// ponytail: async fns run blocking git on a runtime worker; swap to spawn_blocking
// only if a git call is ever measured to starve the executor.

use crate::git;
use crate::state::{self, AppState, RepoInfo, Settings};
use crate::watcher;
use std::path::Path;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

/// Persist the current settings snapshot to disk.
fn save(state: &AppState) -> Result<(), String> {
    let snap = state.snapshot()?;
    state::persist(&state.config_path, &snap)
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<Settings, String> {
    state.snapshot()
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    let old = state.snapshot()?;

    // Re-register the global shortcut if it changed.
    if settings.shortcut != old.shortcut {
        let gs = app.global_shortcut();
        let _ = gs.unregister(old.shortcut.as_str());
        gs.register(settings.shortcut.as_str())
            .map_err(|e| format!("invalid shortcut '{}': {e}", settings.shortcut))?;
    }

    // Enable/disable autostart if it changed.
    if settings.launch_at_login != old.launch_at_login {
        let al = app.autolaunch();
        if settings.launch_at_login {
            al.enable().map_err(|e| e.to_string())?;
        } else {
            al.disable().map_err(|e| e.to_string())?;
        }
    }

    *state.settings.lock().map_err(|e| e.to_string())? = settings;
    save(&state)?;
    state.snapshot()
}

#[tauri::command]
pub async fn pick_repo_folder(app: AppHandle) -> Result<Option<String>, String> {
    use std::sync::atomic::Ordering;
    use tauri::Manager;

    // Flag the dialog so the popover's hide-on-blur doesn't fire while the
    // native picker holds focus (the picker steals focus from the popover).
    let state: State<AppState> = app.state();
    state.dialog_open.store(true, Ordering::SeqCst);

    // The blocking picker must not run on the main thread; use the async callback
    // and wait on a channel (the dialog runs on the event loop meanwhile).
    let (tx, rx) = std::sync::mpsc::channel();
    let mut dialog = app.dialog().file();
    let window = app.get_webview_window(crate::MAIN_WINDOW);
    if let Some(w) = &window {
        dialog = dialog.set_parent(w);
    }
    dialog.pick_folder(move |f| {
        let _ = tx.send(f);
    });
    let picked = rx.recv().map_err(|e| e.to_string());

    state.dialog_open.store(false, Ordering::SeqCst);
    // Bring the popover back to front so the user lands where they left off.
    if let Some(w) = &window {
        let _ = w.show();
        let _ = w.set_focus();
    }
    Ok(picked?.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn add_repo(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<RepoInfo, String> {
    let toplevel = git::resolve_toplevel(&path)?;
    let id = state::repo_id_for(&toplevel);

    {
        let mut s = state.settings.lock().map_err(|e| e.to_string())?;
        if let Some(existing) = s.repos.iter().find(|r| r.id == id) {
            return Ok(existing.clone());
        }
        let name = Path::new(&toplevel)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&toplevel)
            .to_string();
        let info = RepoInfo {
            id: id.clone(),
            name,
            path: toplevel.clone(),
        };
        s.repos.push(info);
        if s.active_repo_id.is_none() {
            s.active_repo_id = Some(id.clone());
        }
    }
    save(&state)?;
    let _ = watcher::start(&app, &state, &id, &toplevel); // best-effort

    let info = state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .repos
        .iter()
        .find(|r| r.id == id)
        .cloned()
        .ok_or_else(|| "repo vanished".to_string())?;
    Ok(info)
}

#[tauri::command]
pub fn remove_repo(state: State<AppState>, repo_id: String) -> Result<(), String> {
    {
        let mut s = state.settings.lock().map_err(|e| e.to_string())?;
        s.repos.retain(|r| r.id != repo_id);
        if s.active_repo_id.as_deref() == Some(repo_id.as_str()) {
            s.active_repo_id = s.repos.first().map(|r| r.id.clone());
        }
    }
    watcher::stop(&state, &repo_id);
    save(&state)
}

#[tauri::command]
pub fn set_active_repo(state: State<AppState>, repo_id: String) -> Result<(), String> {
    {
        let mut s = state.settings.lock().map_err(|e| e.to_string())?;
        if !s.repos.iter().any(|r| r.id == repo_id) {
            return Err(format!("repo not found: {repo_id}"));
        }
        s.active_repo_id = Some(repo_id);
    }
    save(&state)
}

#[tauri::command]
pub async fn get_branches(
    state: State<'_, AppState>,
    repo_id: String,
) -> Result<Vec<git::BranchInfo>, String> {
    let path = state.repo_path(&repo_id)?;
    let show_remotes = state.snapshot()?.show_remote_branches;
    git::get_branches(&path, show_remotes)
}

#[tauri::command]
pub async fn get_log(
    state: State<'_, AppState>,
    repo_id: String,
    refs: Vec<String>,
    skip: u32,
    limit: u32,
) -> Result<Vec<git::CommitInfo>, String> {
    let path = state.repo_path(&repo_id)?;
    git::get_log(&path, &refs, skip, limit)
}

#[tauri::command]
pub async fn get_commit(
    state: State<'_, AppState>,
    repo_id: String,
    hash: String,
) -> Result<git::CommitDetail, String> {
    let path = state.repo_path(&repo_id)?;
    git::get_commit(&path, &hash)
}

#[tauri::command]
pub async fn get_status(
    state: State<'_, AppState>,
    repo_id: String,
) -> Result<git::WorkingStatus, String> {
    let path = state.repo_path(&repo_id)?;
    git::get_status(&path)
}

#[tauri::command]
pub async fn checkout(
    state: State<'_, AppState>,
    repo_id: String,
    ref_name: String,
) -> Result<(), String> {
    let path = state.repo_path(&repo_id)?;
    git::checkout(&path, &ref_name)
}

#[tauri::command]
pub async fn create_branch(
    state: State<'_, AppState>,
    repo_id: String,
    name: String,
    from_ref: String,
) -> Result<(), String> {
    let path = state.repo_path(&repo_id)?;
    git::create_branch(&path, &name, &from_ref)
}

#[tauri::command]
pub async fn rename_branch(
    state: State<'_, AppState>,
    repo_id: String,
    old_name: String,
    new_name: String,
) -> Result<(), String> {
    let path = state.repo_path(&repo_id)?;
    git::rename_branch(&path, &old_name, &new_name)
}

#[tauri::command]
pub async fn delete_branch(
    state: State<'_, AppState>,
    repo_id: String,
    name: String,
) -> Result<(), String> {
    let path = state.repo_path(&repo_id)?;
    git::delete_branch(&path, &name)
}

#[tauri::command]
pub async fn fetch_repo(state: State<'_, AppState>, repo_id: String) -> Result<(), String> {
    let path = state.repo_path(&repo_id)?;
    git::fetch(&path)
}

#[tauri::command]
pub async fn pull_repo(state: State<'_, AppState>, repo_id: String) -> Result<(), String> {
    let path = state.repo_path(&repo_id)?;
    git::pull(&path)
}

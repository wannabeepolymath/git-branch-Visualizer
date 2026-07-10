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

/// Working directory a worktree-sensitive command should run in. `None`/empty →
/// the repo's main path. Otherwise the path is validated to be a worktree of this
/// repo (shares its common git dir) before being trusted as a cwd.
// ponytail: two extra `rev-parse` calls per routed action for validation; cache the
// worktree set in AppState only if it ever profiles hot.
fn work_dir(
    state: &AppState,
    repo_id: &str,
    worktree_path: Option<String>,
) -> Result<String, String> {
    let repo = state.repo_path(repo_id)?;
    match worktree_path {
        Some(wt) if !wt.is_empty() && wt != repo => {
            if git::common_dir(&repo)? == git::common_dir(&wt)? {
                Ok(wt)
            } else {
                Err(format!("not a worktree of this repo: {wt}"))
            }
        }
        _ => Ok(repo),
    }
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<Settings, String> {
    state.snapshot()
}

/// Reset the popover to its default size and re-anchor it under the tray icon
/// (manual override for when the user has dragged/resized it away). See
/// `toggle_popover`. Resize happens first because TrayCenter's x depends on the
/// window width.
// ponytail: default size mirrors tauri.conf.json's window config — keep in sync.
#[tauri::command]
pub fn recenter_window(app: AppHandle) -> Result<(), String> {
    use tauri::{LogicalSize, Manager};
    use tauri_plugin_positioner::{Position, WindowExt};
    let window = app
        .get_webview_window(crate::MAIN_WINDOW)
        .ok_or("main window not found")?;
    window
        .set_size(LogicalSize::new(420.0, 560.0))
        .map_err(|e| e.to_string())?;
    window
        .move_window(Position::TrayCenter)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    let old = state.snapshot()?;

    // Re-register the global shortcut if it changed. Register the new combo
    // BEFORE dropping the old one — a failed registration (combo taken by
    // another app) must leave the old shortcut working, not strand the user
    // with no hotkey.
    if settings.shortcut != old.shortcut {
        let gs = app.global_shortcut();
        gs.register(settings.shortcut.as_str())
            .map_err(|e| format!("invalid shortcut '{}': {e}", settings.shortcut))?;
        let _ = gs.unregister(old.shortcut.as_str());
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
    // The canonical path IS the id: stable across runs and Rust versions (a
    // hash wasn't), and dedupe is a straight path comparison. Old hash-based
    // ids persist in existing configs and keep working — they're only ever
    // compared, never recomputed.
    let id = toplevel.clone();

    {
        let mut s = state.settings.lock().map_err(|e| e.to_string())?;
        if let Some(existing) = s.repos.iter().find(|r| r.path == toplevel) {
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
pub async fn get_worktrees(
    state: State<'_, AppState>,
    repo_id: String,
) -> Result<Vec<git::WorktreeInfo>, String> {
    let path = state.repo_path(&repo_id)?;
    git::get_worktrees(&path)
}

/// Open a worktree with a configured target (editor / terminal / file manager).
#[tauri::command]
pub async fn open_worktree(
    state: State<'_, AppState>,
    repo_id: String,
    worktree_path: String,
    target_id: String,
) -> Result<(), String> {
    let path = work_dir(&state, &repo_id, Some(worktree_path))?;
    let command = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.open_targets
            .iter()
            .find(|t| t.id == target_id)
            .map(|t| t.command.clone())
    }
    .ok_or_else(|| format!("open target not found: {target_id}"))?;
    crate::open::run(&command, &path)
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
    worktree_path: Option<String>,
) -> Result<git::WorkingStatus, String> {
    let path = work_dir(&state, &repo_id, worktree_path)?;
    git::get_status(&path)
}

#[tauri::command]
pub async fn diff_file(
    state: State<'_, AppState>,
    repo_id: String,
    path: String,
    staged: bool,
    untracked: bool,
    worktree_path: Option<String>,
) -> Result<String, String> {
    let repo = work_dir(&state, &repo_id, worktree_path)?;
    git::diff_file(&repo, &path, staged, untracked)
}

#[tauri::command]
pub async fn stage_files(
    state: State<'_, AppState>,
    repo_id: String,
    paths: Vec<String>,
    worktree_path: Option<String>,
) -> Result<(), String> {
    let repo = work_dir(&state, &repo_id, worktree_path)?;
    git::stage(&repo, &paths)
}

#[tauri::command]
pub async fn unstage_files(
    state: State<'_, AppState>,
    repo_id: String,
    paths: Vec<String>,
    worktree_path: Option<String>,
) -> Result<(), String> {
    let repo = work_dir(&state, &repo_id, worktree_path)?;
    git::unstage(&repo, &paths)
}

#[tauri::command]
pub async fn discard_files(
    state: State<'_, AppState>,
    repo_id: String,
    paths: Vec<String>,
    untracked: bool,
    worktree_path: Option<String>,
) -> Result<(), String> {
    let repo = work_dir(&state, &repo_id, worktree_path)?;
    git::discard(&repo, &paths, untracked)
}

#[tauri::command]
pub async fn checkout(
    state: State<'_, AppState>,
    repo_id: String,
    ref_name: String,
    worktree_path: Option<String>,
) -> Result<(), String> {
    let path = work_dir(&state, &repo_id, worktree_path)?;
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
    force: bool,
) -> Result<(), String> {
    let path = state.repo_path(&repo_id)?;
    git::delete_branch(&path, &name, force)
}

/// Ok(true) if any ref changed — the frontend's toast says so honestly.
#[tauri::command]
pub async fn fetch_repo(state: State<'_, AppState>, repo_id: String) -> Result<bool, String> {
    let path = state.repo_path(&repo_id)?;
    git::fetch(&path)
}

/// Pull runs in the focused worktree — it merges into a working tree, so it must
/// target the same one the rest of the UI is acting on. Ok(false) = already up
/// to date (nothing was pulled).
#[tauri::command]
pub async fn pull_repo(
    state: State<'_, AppState>,
    repo_id: String,
    worktree_path: Option<String>,
) -> Result<bool, String> {
    let path = work_dir(&state, &repo_id, worktree_path)?;
    git::pull(&path)
}

#[tauri::command]
pub async fn push_branch(
    state: State<'_, AppState>,
    repo_id: String,
    branch: String,
    upstream: Option<String>,
    set_upstream: bool,
    force: bool,
) -> Result<(), String> {
    let path = state.repo_path(&repo_id)?;
    git::push(&path, &branch, upstream.as_deref(), set_upstream, force)
}

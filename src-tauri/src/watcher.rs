//! notify watcher on <repo>/.git. Debounces ~300ms and drops *.lock churn,
//! then emits the Tauri event `repo-changed` with `{ "repoId": "<id>" }`.

use crate::state::AppState;
use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RepoChanged {
    repo_id: String,
}

/// True for noisy files we don't want to trigger a refresh (index.lock etc).
fn is_noise(path: &PathBuf) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".lock"))
        .unwrap_or(false)
}

/// Start (or restart) a watcher for `repo_id` on the repo's git dir.
/// In a linked worktree `.git` is a file pointing at the shared git dir — resolve
/// and watch that instead (it also holds the worktree's HEAD/index, under
/// `worktrees/<name>/`). Best-effort: returns Err if nothing watchable exists.
pub fn start(
    app: &AppHandle,
    state: &AppState,
    repo_id: &str,
    repo_path: &str,
) -> Result<(), String> {
    let mut git_dir = PathBuf::from(repo_path).join(".git");
    if !git_dir.is_dir() {
        git_dir = PathBuf::from(crate::git::common_dir(repo_path)?);
    }
    if !git_dir.is_dir() {
        return Err(format!("{} is not a .git directory", git_dir.display()));
    }

    let (tx, rx) = mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            // Skip events whose paths are all lock-file churn.
            if !event.paths.is_empty() && event.paths.iter().all(is_noise) {
                return;
            }
            let _ = tx.send(());
        }
    })
    .map_err(|e| e.to_string())?;

    watcher
        .watch(&git_dir, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    // Debounce thread: coalesce bursts, emit once things go quiet.
    let app = app.clone();
    let id = repo_id.to_string();
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            // Drain until 300ms of silence (or the sender is dropped).
            loop {
                match rx.recv_timeout(DEBOUNCE) {
                    Ok(()) => continue,
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
            let _ = app.emit("repo-changed", RepoChanged { repo_id: id.clone() });
        }
    });

    // Dropping the previous watcher (if any) disconnects its thread's channel.
    state
        .watchers
        .lock()
        .map_err(|e| e.to_string())?
        .insert(repo_id.to_string(), watcher);
    Ok(())
}

/// Stop the watcher for `repo_id` (dropping it ends the debounce thread).
pub fn stop(state: &AppState, repo_id: &str) {
    if let Ok(mut w) = state.watchers.lock() {
        w.remove(repo_id);
    }
}

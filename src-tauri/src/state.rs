//! Registered repos + settings, persisted as JSON in the app config dir.
//! Managed behind a Mutex; watchers keyed by repo id.

use notify::RecommendedWatcher;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub repos: Vec<RepoInfo>,
    pub active_repo_id: Option<String>,
    pub shortcut: String,
    pub launch_at_login: bool,
    pub theme: String,
    pub commits_per_page: u32,
    pub show_remote_branches: bool,
    /// Ask for tick/cross confirmation before codebase-affecting actions.
    /// `serde(default)` keeps pre-existing config files loadable (defaults to on).
    #[serde(default = "default_true")]
    pub confirm_actions: bool,
}

fn default_true() -> bool {
    true
}

impl Settings {
    pub fn defaults(shortcut: &str) -> Self {
        Settings {
            repos: Vec::new(),
            active_repo_id: None,
            shortcut: shortcut.to_string(),
            launch_at_login: false,
            theme: "graphite".to_string(),
            commits_per_page: 200,
            show_remote_branches: true,
            confirm_actions: true,
        }
    }
}

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub config_path: PathBuf,
    pub watchers: Mutex<HashMap<String, RecommendedWatcher>>,
    /// True while a native dialog is open — suppresses the popover's hide-on-blur.
    pub dialog_open: AtomicBool,
    /// Set once the popover has been anchored under the tray icon. After that we
    /// reopen wherever the user last left it instead of re-centering every time.
    pub positioned: AtomicBool,
}

impl AppState {
    /// Load settings from disk, or fall back to defaults with the given shortcut.
    pub fn load(config_path: PathBuf, default_shortcut: &str) -> Self {
        let settings = std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
            .unwrap_or_else(|| Settings::defaults(default_shortcut));
        AppState {
            settings: Mutex::new(settings),
            config_path,
            watchers: Mutex::new(HashMap::new()),
            dialog_open: AtomicBool::new(false),
            positioned: AtomicBool::new(false),
        }
    }

    pub fn snapshot(&self) -> Result<Settings, String> {
        Ok(self.settings.lock().map_err(|e| e.to_string())?.clone())
    }

    pub fn repo_path(&self, repo_id: &str) -> Result<String, String> {
        self.settings
            .lock()
            .map_err(|e| e.to_string())?
            .repos
            .iter()
            .find(|r| r.id == repo_id)
            .map(|r| r.path.clone())
            .ok_or_else(|| format!("repo not found: {repo_id}"))
    }
}

/// Deterministic id from the (canonical) repo path — makes dedupe trivial.
pub fn repo_id_for(path: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    format!("{:x}", h.finish())
}

pub fn persist(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_config_without_confirm_actions_loads_with_it_on() {
        // A settings blob written before confirm_actions existed must still parse
        // (not reset the user's settings) and default the new field to on.
        let json = r#"{
            "repos": [], "activeRepoId": null, "shortcut": "Alt+Shift+G",
            "launchAtLogin": false, "theme": "system",
            "commitsPerPage": 200, "showRemoteBranches": true
        }"#;
        let s: Settings = serde_json::from_str(json).expect("old config should still parse");
        assert!(s.confirm_actions);
    }
}

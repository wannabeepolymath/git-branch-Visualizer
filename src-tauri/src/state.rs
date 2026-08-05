//! Registered repos + settings, persisted as JSON in the app config dir.
//! Managed behind a Mutex; watchers keyed by repo id.

use notify::RecommendedWatcher;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
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

/// A user-configurable "open worktree with…" action. `command` is a shell command
/// template holding a `{path}` placeholder, filled with the worktree directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenTarget {
    pub id: String,
    pub name: String,
    pub command: String,
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
    /// "Open worktree with…" targets and which one the one-click ↗ uses.
    #[serde(default = "default_open_targets")]
    pub open_targets: Vec<OpenTarget>,
    #[serde(default = "default_open_target_id")]
    pub default_open_target: Option<String>,
}

fn default_true() -> bool {
    true
}

// ponytail: macOS defaults (v1 target). `open` is macOS-only; move these behind
// platform:: when Windows/Linux seeds are needed.
fn default_open_targets() -> Vec<OpenTarget> {
    let t = |id: &str, name: &str, command: &str| OpenTarget {
        id: id.to_string(),
        name: name.to_string(),
        command: command.to_string(),
    };
    vec![
        t("editor", "Editor", "code {path}"),
        t("terminal", "Terminal", "open -a Terminal {path}"),
        t("finder", "Finder", "open {path}"),
    ]
}

fn default_open_target_id() -> Option<String> {
    Some("terminal".to_string())
}

impl Settings {
    pub fn defaults(shortcut: &str) -> Self {
        Settings {
            repos: Vec::new(),
            active_repo_id: None,
            shortcut: shortcut.to_string(),
            launch_at_login: false,
            theme: "midnight".to_string(),
            commits_per_page: 200,
            show_remote_branches: true,
            confirm_actions: true,
            open_targets: default_open_targets(),
            default_open_target: default_open_target_id(),
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
    /// A file that EXISTS but won't parse is corrupt, not a fresh install: keep it
    /// (renamed `.corrupt`) so the user's repos/shortcut are recoverable instead of
    /// being silently overwritten by defaults on the next settings write. A file
    /// merely missing newer fields still parses (serde defaults) and is untouched.
    pub fn load(config_path: PathBuf, default_shortcut: &str) -> Self {
        let settings = match std::fs::read_to_string(&config_path) {
            Ok(raw) => serde_json::from_str::<Settings>(&raw).unwrap_or_else(|e| {
                let corrupt = config_path.with_extension("corrupt");
                let _ = std::fs::rename(&config_path, &corrupt);
                eprintln!(
                    "settings.json did not parse ({e}); kept a copy at {} and started from defaults",
                    corrupt.display()
                );
                Settings::defaults(default_shortcut)
            }),
            Err(_) => Settings::defaults(default_shortcut),
        };
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

/// Serializes the write-then-rename below. Commands run concurrently on the tokio
/// runtime, and they all share one temp path — without this, two overlapping saves
/// truncate each other's temp file and one renames a half-written config into place,
/// which is the very corruption the rename is here to prevent.
static PERSIST: Mutex<()> = Mutex::new(());

pub fn persist(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let _guard = PERSIST.lock().map_err(|e| e.to_string())?;
    // Write-then-rename: this runs on every checkbox toggle, and a plain write that
    // dies half-way leaves a truncated file that `load` can only read as corrupt.
    // The temp file is a sibling so the rename stays on one filesystem (atomic).
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    f.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
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

    #[test]
    fn old_config_gets_seeded_open_targets() {
        // A config written before open targets existed must load with them seeded,
        // defaulting the one-click target to the editor.
        let json = r#"{
            "repos": [], "activeRepoId": null, "shortcut": "Alt+Shift+G",
            "launchAtLogin": false, "theme": "graphite",
            "commitsPerPage": 200, "showRemoteBranches": true
        }"#;
        let s: Settings = serde_json::from_str(json).expect("old config should still parse");
        assert_eq!(s.open_targets.len(), 3);
        assert_eq!(s.default_open_target.as_deref(), Some("terminal"));
    }

    /// Fresh empty dir under the system temp dir, named after the test.
    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bv-state-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn truncated_config_is_kept_aside_not_silently_wiped() {
        let cfg = tmp_dir("truncated").join("settings.json");
        // Half-written file, exactly what a crash mid-`persist` used to leave.
        std::fs::write(&cfg, r#"{"repos":[{"id":"/a","name":"a","pa"#).unwrap();

        let st = AppState::load(cfg.clone(), "Alt+Shift+G");

        assert!(st.settings.lock().unwrap().repos.is_empty());
        assert!(!cfg.exists(), "corrupt file must be moved aside");
        assert!(
            cfg.with_extension("corrupt").exists(),
            "user's config must stay recoverable"
        );
    }

    #[test]
    fn config_missing_a_newer_field_is_left_alone() {
        // A missing optional field is a successful parse, NOT corruption — the file
        // must survive untouched with its repos intact.
        let cfg = tmp_dir("old-field").join("settings.json");
        std::fs::write(
            &cfg,
            r#"{
                "repos": [{"id": "/a", "name": "a", "path": "/a"}],
                "activeRepoId": "/a", "shortcut": "Alt+Shift+G",
                "launchAtLogin": false, "theme": "midnight",
                "commitsPerPage": 200, "showRemoteBranches": true
            }"#,
        )
        .unwrap();

        let st = AppState::load(cfg.clone(), "Alt+Shift+G");

        assert_eq!(st.settings.lock().unwrap().repos.len(), 1);
        assert!(cfg.exists());
        assert!(!cfg.with_extension("corrupt").exists());
    }

    #[test]
    fn persist_leaves_no_temp_file_and_round_trips() {
        let cfg = tmp_dir("persist").join("settings.json");
        let mut s = Settings::defaults("Alt+Shift+G");
        s.repos.push(RepoInfo {
            id: "/a".into(),
            name: "a".into(),
            path: "/a".into(),
        });

        persist(&cfg, &s).unwrap();

        assert!(!cfg.with_extension("tmp").exists(), "temp must be renamed");
        let loaded = AppState::load(cfg.clone(), "Alt+Shift+G");
        assert_eq!(loaded.settings.lock().unwrap().repos.len(), 1);
        assert!(!cfg.with_extension("corrupt").exists());
    }
}

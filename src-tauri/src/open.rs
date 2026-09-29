//! Spawn a user-configured "open worktree with…" command (editor / terminal /
//! file manager). Templates use a `{path}` placeholder, filled with the worktree
//! directory and launched detached.

/// Tokenize `template` on whitespace, substituting `{path}` with `path`. Because
/// `{path}` occupies its own token, a path containing spaces stays a single
/// argument (no shell, no quoting games).
pub fn substitute_path(template: &str, path: &str) -> Vec<String> {
    template
        .split_whitespace()
        .map(|tok| tok.replace("{path}", path))
        .collect()
}

/// Spawn the command detached — we don't wait for the launched app to exit.
pub fn run(template: &str, path: &str) -> Result<(), String> {
    let tokens = substitute_path(template, path);
    let (program, args) = tokens
        .split_first()
        .ok_or_else(|| "empty open command".to_string())?;
    let spawn = |path_env: Option<&str>| {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args);
        if let Some(p) = path_env {
            cmd.env("PATH", p); // std resolves `program` against the child's PATH
        }
        cmd.spawn()
    };
    let mut child = match spawn(None) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => match login_shell_path() {
            Some(p) => spawn(Some(&p)),
            None => Err(e),
        },
        r => r,
    }
    .map_err(|e| format!("failed to run '{program}': {e}"))?;
    // Reap off-thread: nothing waits on a dropped Child, so every launched editor
    // would otherwise sit in the process table as a zombie for the app's lifetime.
    // ponytail: one short-lived thread per click, which ends when the editor does.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// An app launched from Finder/Dock inherits launchd's bare PATH
/// (/usr/bin:/bin:/usr/sbin:/sbin), so editor launchers in /usr/local/bin or
/// /opt/homebrew/bin (cursor, code, zed) aren't found. Ask the user's login shell
/// for the PATH their terminal gets. `printenv` is external, so this works in
/// fish too. ponytail: no timeout — a login profile that blocks hangs the click.
fn login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").ok()?;
    let out = std::process::Command::new(shell)
        .args(["-l", "-c", "/usr/bin/printenv PATH"])
        .output() // stdin is null, so a profile that reads input can't wait on us
        .ok()?;
    let path = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !path.is_empty()).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_path_with_spaces_as_one_arg() {
        assert_eq!(
            substitute_path("code {path}", "/Users/me/my repo"),
            vec!["code", "/Users/me/my repo"],
        );
    }

    #[test]
    fn splits_multi_flag_command() {
        assert_eq!(
            substitute_path("open -a Terminal {path}", "/tmp/wt"),
            vec!["open", "-a", "Terminal", "/tmp/wt"],
        );
    }

    #[test]
    fn login_shell_reports_a_path() {
        if std::env::var("SHELL").is_err() {
            return; // nothing to ask; run() then reports the original NotFound
        }
        let p = login_shell_path().expect("login shell gave no PATH");
        assert!(p.split(':').any(|d| d == "/usr/bin" || d == "/bin"), "{p}");
    }

    #[test]
    fn empty_template_yields_no_tokens() {
        assert!(substitute_path("   ", "/x").is_empty());
    }
}

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
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("failed to run '{program}': {e}"))
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
    fn empty_template_yields_no_tokens() {
        assert!(substitute_path("   ", "/x").is_empty());
    }
}

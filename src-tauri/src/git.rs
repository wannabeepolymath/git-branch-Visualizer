//! Run the system `git` binary (never a shell) and parse machine-readable output.
//! All functions take the repo's working-tree path. Pure parsers are unit-tested.

use serde::Serialize;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub last_commit_time: i64,
    pub last_commit_subject: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub hash: String,
    pub parents: Vec<String>,
    pub subject: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub status: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    #[serde(flatten)]
    pub commit: CommitInfo,
    pub body: String,
    pub files: Vec<FileChange>,
}

/// Working-tree status split into index (staged) and worktree (unstaged) changes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingStatus {
    pub staged: Vec<FileChange>,
    pub unstaged: Vec<FileChange>,
}

/// One entry from `git worktree list`. `branch` is None for a detached HEAD (or a
/// bare main worktree). `ahead`/`behind` are intentionally absent — the frontend
/// joins them from the branch list by branch name (no extra git cost).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfo {
    pub path: String,
    pub branch: Option<String>,
    pub head: String, // full SHA; frontend shortens for detached display
    pub is_main: bool,
    pub dirty: bool,
    pub locked: bool,
    pub prunable: bool,
}

/// Run git in `repo` with locks disabled and terminal prompts suppressed.
/// Returns stdout on success, trimmed stderr as the error otherwise.
/// `allow_one` also accepts exit status 1 (used by `git diff --no-index`,
/// which signals "differences found" — not an error — with code 1).
fn git_ok(repo: &str, args: &[&str], allow_one: bool) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("--no-optional-locks")
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if out.status.success() || (allow_one && out.status.code() == Some(1)) {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() {
            format!("git exited with status {}", out.status)
        } else {
            err
        })
    }
}

fn git(repo: &str, args: &[&str]) -> Result<String, String> {
    git_ok(repo, args, false)
}

/// git version of a for-each-ref line: fields separated by NUL.
const REF_FORMAT: &str =
    "%(HEAD)%00%(refname:short)%00%(upstream:short)%00%(upstream:track)%00%(committerdate:unix)%00%(subject)";

const LOG_FORMAT: &str = "%H%x00%P%x00%s%x00%an%x00%ae%x00%ct%x00%D";
// Adds body as an 8th NUL field for commit detail.
const SHOW_FORMAT: &str = "%H%x00%P%x00%s%x00%an%x00%ae%x00%ct%x00%D%x00%b";

// ---- pure parsers ----

/// Parse "[ahead 2, behind 1]" / "[ahead 3]" / "[behind 4]" / "" / "[gone]".
pub fn parse_upstream_track(track: &str) -> (u32, u32) {
    let inner = track.trim().trim_start_matches('[').trim_end_matches(']');
    let mut ahead = 0;
    let mut behind = 0;
    for part in inner.split(',') {
        let part = part.trim();
        if let Some(n) = part.strip_prefix("ahead ") {
            ahead = n.trim().parse().unwrap_or(0);
        } else if let Some(n) = part.strip_prefix("behind ") {
            behind = n.trim().parse().unwrap_or(0);
        }
    }
    (ahead, behind)
}

/// Parse one NUL-separated for-each-ref line into a BranchInfo.
/// `is_remote` sets the flag and suppresses upstream/ahead/behind for remotes.
/// Returns None for malformed lines or remote HEAD pointers (e.g. origin/HEAD).
pub fn parse_for_each_ref_line(line: &str, is_remote: bool) -> Option<BranchInfo> {
    let f: Vec<&str> = line.split('\u{0}').collect();
    if f.len() < 6 {
        return None;
    }
    let name = f[1].to_string();
    if name.is_empty() || (is_remote && name.ends_with("/HEAD")) {
        return None;
    }
    let (upstream, ahead, behind) = if is_remote {
        (None, 0, 0)
    } else {
        let up = if f[2].is_empty() {
            None
        } else {
            Some(f[2].to_string())
        };
        let (a, b) = parse_upstream_track(f[3]);
        (up, a, b)
    };
    Some(BranchInfo {
        is_current: f[0] == "*",
        name,
        is_remote,
        upstream,
        ahead,
        behind,
        last_commit_time: f[4].parse().unwrap_or(0),
        last_commit_subject: f[5].to_string(),
    })
}

/// Clean %D ref decorations: drop "HEAD"/"HEAD ->", tags as "tag:v1.0".
fn clean_refs(d: &str) -> Vec<String> {
    d.split(", ")
        .filter_map(|r| {
            let r = r.trim();
            if r.is_empty() || r == "HEAD" {
                None
            } else if let Some(rest) = r.strip_prefix("HEAD -> ") {
                Some(rest.to_string())
            } else if let Some(tag) = r.strip_prefix("tag: ") {
                Some(format!("tag:{tag}"))
            } else {
                Some(r.to_string())
            }
        })
        .collect()
}

/// Parse one NUL-separated `git log` line (LOG_FORMAT) into a CommitInfo.
pub fn parse_log_line(line: &str) -> Option<CommitInfo> {
    let f: Vec<&str> = line.splitn(7, '\u{0}').collect();
    if f.len() < 7 {
        return None;
    }
    Some(CommitInfo {
        hash: f[0].to_string(),
        parents: f[1].split_whitespace().map(String::from).collect(),
        subject: f[2].to_string(),
        author_name: f[3].to_string(),
        author_email: f[4].to_string(),
        timestamp: f[5].parse().unwrap_or(0),
        refs: clean_refs(f[6]),
    })
}

/// Parse `git status --porcelain=v1 -z` output into staged/unstaged file lists.
/// Each entry is `XY<space>PATH`; X is the index status, Y the worktree status.
/// Rename/copy entries carry the original path in a following NUL field (consumed).
pub fn parse_status_porcelain(out: &str) -> WorkingStatus {
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut entries = out.split('\u{0}');
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue; // trailing empty field after final NUL, or malformed line
        }
        let b = entry.as_bytes();
        let (x, y) = (b[0] as char, b[1] as char);
        let path = entry[3..].to_string(); // skip "XY "
        if x == 'R' || x == 'C' || y == 'R' || y == 'C' {
            entries.next(); // original path of a rename/copy — not a separate entry
        }
        if x == '?' {
            unstaged.push(FileChange { status: "?".to_string(), path }); // untracked
            continue;
        }
        if x != ' ' {
            staged.push(FileChange { status: x.to_string(), path: path.clone() });
        }
        if y != ' ' {
            unstaged.push(FileChange { status: y.to_string(), path });
        }
    }
    WorkingStatus { staged, unstaged }
}

/// Parse `git worktree list --porcelain` into records. Records are separated by a
/// blank line; the first record is the main worktree. `dirty` is left false here —
/// it needs a per-worktree git call, filled in by `get_worktrees`.
pub fn parse_worktree_list(out: &str) -> Vec<WorktreeInfo> {
    let mut result = Vec::new();
    let mut cur: Option<WorktreeInfo> = None;
    let flush = |cur: &mut Option<WorktreeInfo>, result: &mut Vec<WorktreeInfo>| {
        if let Some(w) = cur.take() {
            result.push(w);
        }
    };
    for line in out.lines() {
        if line.is_empty() {
            flush(&mut cur, &mut result);
            continue;
        }
        let (key, val) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "worktree" => {
                flush(&mut cur, &mut result); // defensive: records are blank-separated
                cur = Some(WorktreeInfo {
                    path: val.to_string(),
                    branch: None,
                    head: String::new(),
                    is_main: result.is_empty(), // main worktree is listed first
                    dirty: false,
                    locked: false,
                    prunable: false,
                });
            }
            "HEAD" => {
                if let Some(w) = cur.as_mut() {
                    w.head = val.to_string();
                }
            }
            "branch" => {
                if let Some(w) = cur.as_mut() {
                    w.branch = Some(val.strip_prefix("refs/heads/").unwrap_or(val).to_string());
                }
            }
            "locked" => {
                if let Some(w) = cur.as_mut() {
                    w.locked = true;
                }
            }
            "prunable" => {
                if let Some(w) = cur.as_mut() {
                    w.prunable = true;
                }
            }
            _ => {} // "detached", "bare" — branch stays None
        }
    }
    flush(&mut cur, &mut result);
    result
}

// ---- git invocations ----

/// List the repository's worktrees, each tagged with a cheap dirty flag.
pub fn get_worktrees(repo: &str) -> Result<Vec<WorktreeInfo>, String> {
    let out = git(repo, &["worktree", "list", "--porcelain"])?;
    let mut worktrees = parse_worktree_list(&out);
    for w in worktrees.iter_mut() {
        if !w.head.is_empty() {
            // bare worktrees have no HEAD/working tree
            w.dirty = is_dirty(&w.path);
        }
    }
    Ok(worktrees)
}

/// True if the worktree at `path` has any staged or unstaged change. Best-effort:
/// a stale/unreadable path reports clean rather than erroring the whole listing.
fn is_dirty(path: &str) -> bool {
    git(path, &["status", "--porcelain", "--untracked-files=all"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

pub fn get_branches(repo: &str, include_remotes: bool) -> Result<Vec<BranchInfo>, String> {
    let mut out = Vec::new();
    let locals = git(repo, &["for-each-ref", &format!("--format={REF_FORMAT}"), "refs/heads"])?;
    for line in locals.lines() {
        if let Some(b) = parse_for_each_ref_line(line, false) {
            out.push(b);
        }
    }
    if include_remotes {
        let remotes = git(
            repo,
            &["for-each-ref", &format!("--format={REF_FORMAT}"), "refs/remotes"],
        )?;
        for line in remotes.lines() {
            if let Some(b) = parse_for_each_ref_line(line, true) {
                out.push(b);
            }
        }
    }
    Ok(out)
}

/// Log the union of the given refs. Empty `refs` means all branches/remotes/tags.
pub fn get_log(
    repo: &str,
    refs: &[String],
    skip: u32,
    limit: u32,
) -> Result<Vec<CommitInfo>, String> {
    let skip_arg = format!("--skip={skip}");
    let max_arg = format!("--max-count={limit}");
    let fmt_arg = format!("--format={LOG_FORMAT}");
    let mut args = vec!["log", &skip_arg, &max_arg, "--date-order", &fmt_arg];
    if refs.is_empty() {
        args.extend_from_slice(&["--branches", "--remotes", "--tags"]);
    } else {
        args.extend(refs.iter().map(String::as_str));
    }
    let out = git(repo, &args)?;
    Ok(out.lines().filter_map(parse_log_line).collect())
}

pub fn get_commit(repo: &str, hash: &str) -> Result<CommitDetail, String> {
    let meta = git(repo, &["show", "--no-patch", &format!("--format={SHOW_FORMAT}"), hash])?;
    let raw = meta.strip_suffix('\n').unwrap_or(&meta);
    let f: Vec<&str> = raw.splitn(8, '\u{0}').collect();
    if f.len() < 8 {
        return Err("unexpected git show output".to_string());
    }
    let commit = CommitInfo {
        hash: f[0].to_string(),
        parents: f[1].split_whitespace().map(String::from).collect(),
        subject: f[2].to_string(),
        author_name: f[3].to_string(),
        author_email: f[4].to_string(),
        timestamp: f[5].parse().unwrap_or(0),
        refs: clean_refs(f[6]),
    };
    let body = f[7].trim_end().to_string();

    // Separate call keeps file parsing simple (no interleaving with the format).
    let names = git(repo, &["show", "--name-status", "--format=", hash])?;
    let files = names
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let mut parts = l.split('\t');
            let status = parts.next()?.chars().next()?.to_string();
            let path = parts.last().unwrap_or("").to_string(); // new name for renames
            if path.is_empty() {
                None
            } else {
                Some(FileChange { status, path })
            }
        })
        .collect();

    Ok(CommitDetail { commit, body, files })
}

pub fn get_status(repo: &str) -> Result<WorkingStatus, String> {
    let out = git(repo, &["status", "--porcelain=v1", "-z", "--untracked-files=all"])?;
    Ok(parse_status_porcelain(&out))
}

/// Unified diff for one path. `staged` diffs the index against HEAD; otherwise
/// the worktree against the index. `untracked` files have no other side, so we
/// diff against /dev/null to render the whole file as additions.
// ponytail: /dev/null is the special token git understands on all platforms for
// --no-index; fine for this macOS app.
pub fn diff_file(repo: &str, path: &str, staged: bool, untracked: bool) -> Result<String, String> {
    if untracked {
        return git_ok(repo, &["diff", "--no-index", "--", "/dev/null", path], true);
    }
    let mut args = vec!["diff"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    args.push(path);
    git(repo, &args)
}

/// Build `[cmd..., "--", paths...]` — the `--` stops paths being read as flags.
fn with_paths<'a>(head: &[&'a str], paths: &'a [String]) -> Vec<&'a str> {
    let mut args: Vec<&str> = head.to_vec();
    args.push("--");
    args.extend(paths.iter().map(String::as_str));
    args
}

// ---- write / network ops ----

/// Stage paths into the index (`git add` — covers modified, deleted, untracked).
pub fn stage(repo: &str, paths: &[String]) -> Result<(), String> {
    git(repo, &with_paths(&["add"], paths)).map(|_| ())
}

/// Unstage paths, leaving worktree contents untouched.
pub fn unstage(repo: &str, paths: &[String]) -> Result<(), String> {
    git(repo, &with_paths(&["restore", "--staged"], paths)).map(|_| ())
}

/// Discard worktree changes. `untracked` files are removed from disk (`git clean`);
/// tracked files are reverted to their index contents (`git restore`). Destructive.
pub fn discard(repo: &str, paths: &[String], untracked: bool) -> Result<(), String> {
    let head: &[&str] = if untracked { &["clean", "-f"] } else { &["restore"] };
    git(repo, &with_paths(head, paths)).map(|_| ())
}

pub fn checkout(repo: &str, ref_name: &str) -> Result<(), String> {
    git(repo, &["checkout", ref_name]).map(|_| ())
}

pub fn create_branch(repo: &str, name: &str, from_ref: &str) -> Result<(), String> {
    let mut args = vec!["branch", name];
    if !from_ref.is_empty() {
        args.push(from_ref);
    }
    git(repo, &args).map(|_| ())
}

pub fn rename_branch(repo: &str, old_name: &str, new_name: &str) -> Result<(), String> {
    git(repo, &["branch", "-m", old_name, new_name]).map(|_| ())
}

/// Uses `-d` (safe). "not fully merged" surfaces as the error; frontend decides.
pub fn delete_branch(repo: &str, name: &str) -> Result<(), String> {
    git(repo, &["branch", "-d", name]).map(|_| ())
}

pub fn fetch(repo: &str) -> Result<(), String> {
    git(repo, &["fetch", "--all", "--prune"]).map(|_| ())
}

pub fn pull(repo: &str) -> Result<(), String> {
    git(repo, &["pull", "--ff-only"]).map(|_| ())
}

/// Absolute path to the repository's common git dir — the same for a repo and all
/// its linked worktrees. Used to check a candidate worktree belongs to a given repo.
pub fn common_dir(path: &str) -> Result<String, String> {
    let out = git(path, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    Ok(out.trim().to_string())
}

/// Validate `path` is inside a work tree and return the repo top-level path.
pub fn resolve_toplevel(path: &str) -> Result<String, String> {
    let inside = git(path, &["rev-parse", "--is-inside-work-tree"])?;
    if inside.trim() != "true" {
        return Err("not a git working tree".to_string());
    }
    let top = git(path, &["rev-parse", "--show-toplevel"])?;
    Ok(top.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_parses_all_shapes() {
        assert_eq!(parse_upstream_track("[ahead 2, behind 1]"), (2, 1));
        assert_eq!(parse_upstream_track("[ahead 3]"), (3, 0));
        assert_eq!(parse_upstream_track("[behind 4]"), (0, 4));
        assert_eq!(parse_upstream_track(""), (0, 0));
        assert_eq!(parse_upstream_track("[gone]"), (0, 0));
    }

    #[test]
    fn ref_line_local_current() {
        let line = "*\u{0}main\u{0}origin/main\u{0}[ahead 1, behind 2]\u{0}1700000000\u{0}Fix the thing";
        let b = parse_for_each_ref_line(line, false).unwrap();
        assert_eq!(b.name, "main");
        assert!(b.is_current);
        assert!(!b.is_remote);
        assert_eq!(b.upstream.as_deref(), Some("origin/main"));
        assert_eq!((b.ahead, b.behind), (1, 2));
        assert_eq!(b.last_commit_time, 1700000000);
        assert_eq!(b.last_commit_subject, "Fix the thing");
    }

    #[test]
    fn ref_line_local_no_upstream() {
        let line = " \u{0}feature/x\u{0}\u{0}\u{0}1699999999\u{0}wip";
        let b = parse_for_each_ref_line(line, false).unwrap();
        assert_eq!(b.name, "feature/x");
        assert!(!b.is_current);
        assert_eq!(b.upstream, None);
        assert_eq!((b.ahead, b.behind), (0, 0));
    }

    #[test]
    fn ref_line_remote_and_head_skip() {
        let line = " \u{0}origin/feature/y\u{0}\u{0}\u{0}1698000000\u{0}remote work";
        let b = parse_for_each_ref_line(line, true).unwrap();
        assert!(b.is_remote);
        assert_eq!(b.name, "origin/feature/y");
        assert_eq!(b.upstream, None);
        // remote HEAD pointer is dropped
        let head = " \u{0}origin/HEAD\u{0}\u{0}\u{0}0\u{0}";
        assert!(parse_for_each_ref_line(head, true).is_none());
    }

    #[test]
    fn log_line_parses() {
        let line = "abc123\u{0}p1 p2\u{0}Merge stuff\u{0}Ada\u{0}ada@x.io\u{0}1700000001\u{0}HEAD -> main, origin/main, tag: v1.0";
        let c = parse_log_line(line).unwrap();
        assert_eq!(c.hash, "abc123");
        assert_eq!(c.parents, vec!["p1", "p2"]);
        assert_eq!(c.subject, "Merge stuff");
        assert_eq!(c.author_name, "Ada");
        assert_eq!(c.author_email, "ada@x.io");
        assert_eq!(c.timestamp, 1700000001);
        assert_eq!(c.refs, vec!["main", "origin/main", "tag:v1.0"]);
    }

    #[test]
    fn status_porcelain_splits_staged_and_unstaged() {
        // staged+unstaged modify, staged add, untracked, staged rename, unstaged delete
        let out = "MM src/a.rs\u{0}A  src/new.rs\u{0}?? junk.txt\u{0}R  dst.rs\u{0}src.rs\u{0} D gone.rs\u{0}";
        let s = parse_status_porcelain(out);
        assert_eq!(s.staged.len(), 3);
        assert_eq!((s.staged[0].status.as_str(), s.staged[0].path.as_str()), ("M", "src/a.rs"));
        assert_eq!((s.staged[1].status.as_str(), s.staged[1].path.as_str()), ("A", "src/new.rs"));
        assert_eq!((s.staged[2].status.as_str(), s.staged[2].path.as_str()), ("R", "dst.rs"));
        assert_eq!(s.unstaged.len(), 3);
        assert_eq!((s.unstaged[0].status.as_str(), s.unstaged[0].path.as_str()), ("M", "src/a.rs"));
        assert_eq!((s.unstaged[1].status.as_str(), s.unstaged[1].path.as_str()), ("?", "junk.txt"));
        assert_eq!((s.unstaged[2].status.as_str(), s.unstaged[2].path.as_str()), ("D", "gone.rs"));
    }

    #[test]
    fn with_paths_inserts_separator() {
        let paths = vec!["a.rs".to_string(), "-weird".to_string()];
        assert_eq!(with_paths(&["add"], &paths), vec!["add", "--", "a.rs", "-weird"]);
        assert_eq!(with_paths(&["restore", "--staged"], &[]), vec!["restore", "--staged", "--"]);
    }

    #[test]
    fn status_porcelain_empty_is_clean() {
        let s = parse_status_porcelain("");
        assert!(s.staged.is_empty() && s.unstaged.is_empty());
    }

    #[test]
    fn log_line_no_refs() {
        let line = "def\u{0}\u{0}subject only\u{0}Bob\u{0}bob@x.io\u{0}1700000002\u{0}";
        let c = parse_log_line(line).unwrap();
        assert!(c.parents.is_empty());
        assert!(c.refs.is_empty());
    }

    #[test]
    fn worktree_list_parses_records() {
        // main (on branch) + linked branch + detached + locked, blank-separated,
        // with the trailing blank line git emits after the last record.
        let out = "worktree /repo/main\nHEAD aaaa1111\nbranch refs/heads/main\n\nworktree /repo/wt-feature\nHEAD bbbb2222\nbranch refs/heads/feature/x\n\nworktree /repo/wt-detached\nHEAD cccc3333\ndetached\n\nworktree /repo/wt-locked\nHEAD dddd4444\nbranch refs/heads/wip\nlocked being tested\n\n";
        let ws = parse_worktree_list(out);
        assert_eq!(ws.len(), 4);

        assert_eq!(ws[0].path, "/repo/main");
        assert_eq!(ws[0].branch.as_deref(), Some("main"));
        assert_eq!(ws[0].head, "aaaa1111");
        assert!(ws[0].is_main);
        assert!(!ws[0].locked);

        assert_eq!(ws[1].branch.as_deref(), Some("feature/x")); // refs/heads/ stripped
        assert!(!ws[1].is_main);

        assert_eq!(ws[2].path, "/repo/wt-detached");
        assert_eq!(ws[2].branch, None); // detached HEAD
        assert_eq!(ws[2].head, "cccc3333");

        assert_eq!(ws[3].branch.as_deref(), Some("wip"));
        assert!(ws[3].locked);
    }

    #[test]
    fn worktree_list_handles_no_trailing_blank() {
        let out = "worktree /repo/only\nHEAD abcd\nbranch refs/heads/main";
        let ws = parse_worktree_list(out);
        assert_eq!(ws.len(), 1);
        assert!(ws[0].is_main);
    }
}

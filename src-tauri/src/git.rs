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
/// Returns (stdout, stderr) on success, trimmed stderr as the error otherwise.
/// LC_ALL=C pins output to English — callers match message text ("Already up
/// to date", "not fully merged"), which must not vary with the system locale.
/// core.quotepath=false stops git C-quoting non-ASCII paths ("caf\303\251.txt"),
/// which would otherwise be fed back to git as a pathspec and match nothing.
/// `allow_one` also accepts exit status 1 (used by `git diff --no-index`,
/// which signals "differences found" — not an error — with code 1).
fn git_full(repo: &str, args: &[&str], allow_one: bool) -> Result<(String, String), String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("--no-optional-locks")
        .args(["-c", "core.quotepath=false"]) // must precede the subcommand
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if out.status.success() || (allow_one && out.status.code() == Some(1)) {
        Ok((
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ))
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() {
            format!("git exited with status {}", out.status)
        } else {
            err
        })
    }
}

fn git_ok(repo: &str, args: &[&str], allow_one: bool) -> Result<String, String> {
    git_full(repo, args, allow_one).map(|(stdout, _)| stdout)
}

fn git(repo: &str, args: &[&str]) -> Result<String, String> {
    git_ok(repo, args, false)
}

/// `git` for argv that has to be owned (pathspecs and refs built at runtime).
fn git_owned(repo: &str, args: &[String]) -> Result<String, String> {
    git(repo, &args.iter().map(String::as_str).collect::<Vec<_>>())
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
/// Returns None for malformed lines (incl. an unparseable date — dropping the
/// branch beats rendering a fabricated 1970 timestamp) or remote HEAD pointers.
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
        last_commit_time: f[4].parse().ok()?,
        last_commit_subject: f[5].to_string(),
    })
}

/// Clean %D ref decorations: strip the "HEAD -> " prefix, tags as "tag:v1.0".
/// A bare "HEAD" is kept — %D only emits it when HEAD is detached, and that pill
/// is the only place the user can see where a detached checkout landed.
fn clean_refs(d: &str) -> Vec<String> {
    d.split(", ")
        .filter_map(|r| {
            let r = r.trim();
            if r.is_empty() {
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
/// None for malformed lines, including an unparseable commit date.
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
        timestamp: f[5].parse().ok()?,
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
        // Merge conflicts (UU, AA, DD, U?): one "U" entry under unstaged — staging
        // it is `git add`, i.e. "mark resolved".
        if x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D') {
            unstaged.push(FileChange { status: "U".to_string(), path });
            continue;
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

/// Staged rename/copy pairs from `git status --porcelain=v1 -z` as (new, original).
/// The frontend only ever sees the new path, but `git restore --staged <new>` alone
/// leaves the original staged as a deletion — both sides have to be restored.
pub fn parse_staged_rename_pairs(out: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut entries = out.split('\u{0}');
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let (x, y) = (entry.as_bytes()[0] as char, entry.as_bytes()[1] as char);
        if x == 'R' || x == 'C' || y == 'R' || y == 'C' {
            let Some(orig) = entries.next() else { break };
            if x == 'R' || x == 'C' {
                pairs.push((entry[3..].to_string(), orig.to_string()));
            }
        }
    }
    pairs
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
    // -unormal (not -uall): we only need "is anything dirty", and normal mode
    // stops descending into untracked directories — much cheaper on big trees.
    git(path, &["status", "--porcelain", "--untracked-files=normal"])
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

/// argv for `git log`. The trailing `--` is load-bearing: without it a ref that
/// also names a path (branch "docs" + a docs/ directory) either errors as
/// ambiguous or, once the branch is gone, silently becomes a pathspec filter.
fn log_args(refs: &[String], skip: u32, limit: u32) -> Vec<String> {
    let mut args = vec![
        "log".to_string(),
        format!("--skip={skip}"),
        format!("--max-count={limit}"),
        "--date-order".to_string(),
        format!("--format={LOG_FORMAT}"),
    ];
    if refs.is_empty() {
        args.extend(["--branches", "--remotes", "--tags"].map(String::from));
    } else {
        args.extend(refs.iter().cloned());
    }
    args.push("--".to_string());
    args
}

/// Log the union of the given refs. Empty `refs` means all branches/remotes/tags.
pub fn get_log(
    repo: &str,
    refs: &[String],
    skip: u32,
    limit: u32,
) -> Result<Vec<CommitInfo>, String> {
    let out = git_owned(repo, &log_args(refs, skip, limit))?;
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
        timestamp: f[5].parse().map_err(|_| "unexpected git show output".to_string())?,
        refs: clean_refs(f[6]),
    };
    let body = f[7].trim_end().to_string();

    // Separate call keeps file parsing simple (no interleaving with the format).
    // --diff-merges=first-parent: plain `git show` uses the combined diff for
    // merges, which lists (almost) no files; the diff vs the first parent is
    // what the detail panel should show. No effect on non-merge commits.
    // ponytail: line-based parse. core.quotepath=false (see git_full) gives
    // verbatim non-ASCII paths, but git still escapes control characters, so a
    // filename containing a newline still breaks this — pass -z if that shows up.
    let names = git(
        repo,
        &["show", "--name-status", "--diff-merges=first-parent", "--format=", hash],
    )?;
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
    let spec = literal(path);
    args.push("--");
    args.push(&spec);
    git(repo, &args)
}

/// Unified diff for one path as changed by a commit (vs its first parent).
/// `git show` handles root commits (no parent) for free; first-parent keeps
/// merge diffs consistent with the file list from `get_commit`.
pub fn diff_commit_file(repo: &str, hash: &str, path: &str) -> Result<String, String> {
    let spec = literal(path);
    git(
        repo,
        &["show", "--format=", "--diff-merges=first-parent", hash, "--", &spec],
    )
}

/// Wrap a path as an exact pathspec. `--` only stops option parsing — git still
/// globs pathspecs, so a file literally named "report[1].txt" would otherwise
/// also match (and stage, revert or delete) its sibling "report1.txt".
fn literal(path: &str) -> String {
    format!(":(literal){path}")
}

/// Build `[cmd..., "--", literal pathspecs...]`.
fn with_paths(head: &[&str], paths: &[String]) -> Vec<String> {
    let mut args: Vec<String> = head.iter().map(|s| s.to_string()).collect();
    args.push("--".to_string());
    args.extend(paths.iter().map(|p| literal(p)));
    args
}

// ---- write / network ops ----

/// Stage paths into the index (`git add` — covers modified, deleted, untracked).
pub fn stage(repo: &str, paths: &[String]) -> Result<(), String> {
    git_owned(repo, &with_paths(&["add"], paths)).map(|_| ())
}

/// Unstage paths, leaving worktree contents untouched. A staged rename is one
/// index entry with two paths but the frontend only knows the new one, so we
/// re-read status here and restore the original alongside it — otherwise the
/// original is left staged as a deletion of a file the user never touched.
pub fn unstage(repo: &str, paths: &[String]) -> Result<(), String> {
    let status = git(repo, &["status", "--porcelain=v1", "-z", "--untracked-files=no"])?;
    let mut all = paths.to_vec();
    for (new, orig) in parse_staged_rename_pairs(&status) {
        if paths.contains(&new) && !all.contains(&orig) {
            all.push(orig);
        }
    }
    git_owned(repo, &with_paths(&["restore", "--staged"], &all)).map(|_| ())
}

/// Discard worktree changes. `untracked` files are removed from disk (`git clean`);
/// tracked files are reverted to their index contents (`git restore`). Destructive.
pub fn discard(repo: &str, paths: &[String], untracked: bool) -> Result<(), String> {
    let head: &[&str] = if untracked { &["clean", "-f"] } else { &["restore"] };
    git_owned(repo, &with_paths(head, paths)).map(|_| ())
}

/// Check out a ref. The trailing `--` forces `ref_name` to be read as a revision:
/// a branch that also names a directory ("docs") is otherwise rejected as
/// ambiguous. DWIM (creating a local branch tracking origin/<name>) still applies.
pub fn checkout(repo: &str, ref_name: &str) -> Result<(), String> {
    git(repo, &["checkout", ref_name, "--"]).map(|_| ())
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

/// `-d` (safe) by default; "not fully merged" surfaces as the error and the
/// frontend re-confirms with `force` (`-D`).
pub fn delete_branch(repo: &str, name: &str, force: bool) -> Result<(), String> {
    git(repo, &["branch", if force { "-D" } else { "-d" }, name]).map(|_| ())
}

/// Ref updates land on stderr as "<old>..<new>  branch -> origin/branch" (also
/// "* [new branch] … ->" / "- [deleted] … ->"); a quiet stderr means nothing new.
fn fetch_changed(stderr: &str) -> bool {
    stderr.lines().any(|l| l.contains("->"))
}

/// An up-to-date pull says "Already up to date." on stdout ("Already up-to-date."
/// pre-2.28); anything else means commits came in.
fn pull_updated(stdout: &str) -> bool {
    !stdout.trim_start().starts_with("Already up")
}

/// Fetch + prune all remotes. Ok(true) if any ref changed.
pub fn fetch(repo: &str) -> Result<bool, String> {
    git_full(repo, &["fetch", "--all", "--prune"], false).map(|(_, stderr)| fetch_changed(&stderr))
}

/// Fast-forward-only pull. Ok(false) when there was nothing to pull.
pub fn pull(repo: &str) -> Result<bool, String> {
    git(repo, &["pull", "--ff-only"]).map(|out| pull_updated(&out))
}

/// Derive the push remote + refspec from the branch's tracked upstream
/// ("upstream/feature/x" → remote "upstream", refspec "local:feature/x").
/// No upstream (publishing) → origin, same name. A slashless upstream is a
/// *local* branch (branch.<n>.remote = "."), which names no remote at all —
/// erroring beats quietly pushing to origin/<branch> instead.
fn push_target(branch: &str, upstream: Option<&str>) -> Result<(String, String), String> {
    match upstream {
        Some(u) => match u.split_once('/') {
            Some((remote, remote_branch)) => {
                Ok((remote.to_string(), format!("{branch}:{remote_branch}")))
            }
            None => Err(format!("'{branch}' tracks local branch '{u}', not a remote")),
        },
        None => Ok(("origin".to_string(), branch.to_string())),
    }
}

/// Push local `branch` to its upstream's remote (origin when none). `set_upstream`
/// publishes it with `-u` (first push, no tracking yet); `force` uses
/// `--force-with-lease` (safe force — refuses to clobber if the remote moved
/// unexpectedly). Always an explicit remote + refspec so the intended branch is
/// pushed even when it isn't the current checkout.
pub fn push(
    repo: &str,
    branch: &str,
    upstream: Option<&str>,
    set_upstream: bool,
    force: bool,
) -> Result<(), String> {
    let (remote, refspec) = push_target(branch, upstream)?;
    let mut args = vec!["push"];
    if set_upstream {
        args.push("--set-upstream");
    } else if force {
        args.push("--force-with-lease");
    }
    args.push(&remote);
    args.push(&refspec);
    git(repo, &args).map(|_| ())
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
    fn with_paths_inserts_separator_and_literal_pathspecs() {
        // glob metacharacters must not survive as a pattern — ":(literal)" pins them
        let paths = vec!["report[1].txt".to_string(), "-weird".to_string()];
        assert_eq!(
            with_paths(&["add"], &paths),
            vec!["add", "--", ":(literal)report[1].txt", ":(literal)-weird"]
        );
        assert_eq!(with_paths(&["restore", "--staged"], &[]), vec!["restore", "--staged", "--"]);
        assert_eq!(with_paths(&["clean", "-f"], &["a*.rs".to_string()]), vec![
            "clean",
            "-f",
            "--",
            ":(literal)a*.rs"
        ]);
    }

    #[test]
    fn staged_rename_pairs_extracted() {
        // staged rename + staged copy + a plain modify (no pair), NUL-separated
        let out = "R  dst.rs\u{0}src.rs\u{0}C  copy.rs\u{0}orig.rs\u{0}M  ok.rs\u{0}";
        assert_eq!(
            parse_staged_rename_pairs(out),
            vec![
                ("dst.rs".to_string(), "src.rs".to_string()),
                ("copy.rs".to_string(), "orig.rs".to_string()),
            ]
        );
        // a worktree-side rename is not a staged pair, but still consumes its
        // original path so the following entry is not misread
        let out = " R wt-dst.rs\u{0}wt-src.rs\u{0}M  after.rs\u{0}";
        assert!(parse_staged_rename_pairs(out).is_empty());
        assert!(parse_staged_rename_pairs("").is_empty());
    }

    #[test]
    fn log_args_terminate_refs() {
        // explicit refs: "--" keeps a path-shaped branch ("docs") from becoming a pathspec
        assert_eq!(
            log_args(&["docs".to_string()], 0, 200),
            vec![
                "log",
                "--skip=0",
                "--max-count=200",
                "--date-order",
                &format!("--format={LOG_FORMAT}"),
                "docs",
                "--"
            ]
        );
        let all = log_args(&[], 10, 50);
        assert_eq!(&all[1..=2], ["--skip=10", "--max-count=50"]);
        assert_eq!(&all[all.len() - 4..], ["--branches", "--remotes", "--tags", "--"]);
    }

    #[test]
    fn detached_head_is_kept_as_a_ref() {
        // %D emits a bare "HEAD" only when detached — the only marker the UI gets
        let line = "abc\u{0}p1\u{0}s\u{0}A\u{0}a@x.io\u{0}1700000003\u{0}HEAD, tag: v2";
        assert_eq!(parse_log_line(line).unwrap().refs, vec!["HEAD", "tag:v2"]);
        // attached HEAD is unchanged: "HEAD -> main" still collapses to "main"
        let line = "abc\u{0}p1\u{0}s\u{0}A\u{0}a@x.io\u{0}1700000003\u{0}HEAD -> main";
        assert_eq!(parse_log_line(line).unwrap().refs, vec!["main"]);
    }

    #[test]
    fn bad_timestamp_is_dropped_not_faked() {
        let line = "abc\u{0}p1\u{0}s\u{0}A\u{0}a@x.io\u{0}not-a-date\u{0}";
        assert!(parse_log_line(line).is_none());
        let line = " \u{0}main\u{0}\u{0}\u{0}\u{0}subject"; // empty committerdate
        assert!(parse_for_each_ref_line(line, false).is_none());
    }

    #[test]
    fn status_porcelain_unmerged_is_single_conflict_entry() {
        let out = "UU conflict.rs\u{0}AA both-added.rs\u{0}M  ok.rs\u{0}";
        let s = parse_status_porcelain(out);
        assert_eq!(s.staged.len(), 1); // only ok.rs — conflicts never count as staged
        assert_eq!(s.unstaged.len(), 2);
        assert_eq!((s.unstaged[0].status.as_str(), s.unstaged[0].path.as_str()), ("U", "conflict.rs"));
        assert_eq!((s.unstaged[1].status.as_str(), s.unstaged[1].path.as_str()), ("U", "both-added.rs"));
    }

    #[test]
    fn pull_and_fetch_detect_no_op() {
        assert!(!pull_updated("Already up to date.\n"));
        assert!(!pull_updated("Already up-to-date.\n")); // pre-2.28 spelling
        assert!(pull_updated("Updating 3238613..3c1620a\nFast-forward\n"));
        assert!(!fetch_changed(""));
        assert!(fetch_changed(
            "From /repo/remote\n   3238613..3c1620a  main       -> origin/main\n"
        ));
        assert!(fetch_changed(" - [deleted]         (none)     -> origin/old\n"));
    }

    #[test]
    fn push_target_follows_upstream() {
        assert_eq!(
            push_target("main", Some("origin/main")).unwrap(),
            ("origin".to_string(), "main:main".to_string())
        );
        // differently-named upstream on a non-origin remote, incl. slashes in the branch
        assert_eq!(
            push_target("feat", Some("upstream/feature/x")).unwrap(),
            ("upstream".to_string(), "feat:feature/x".to_string())
        );
        // no upstream (publish) → origin, same name
        assert_eq!(
            push_target("new-branch", None).unwrap(),
            ("origin".to_string(), "new-branch".to_string())
        );
        // upstream is a local branch (no remote in it) → error, never origin
        assert!(push_target("feat", Some("main")).is_err());
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

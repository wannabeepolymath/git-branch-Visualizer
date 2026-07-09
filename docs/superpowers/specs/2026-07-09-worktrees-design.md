# Worktrees — design

Show and navigate a repository's git worktrees from the Branch Visualizer popover.

## Problem

The user works across many worktrees of a repo at once and finds it hard to move
between them. Today a "repo" is a single working directory; worktrees are invisible,
and adding one as a separate repo is half-broken (the watcher can't watch a linked
worktree's `.git` file, and every worktree would show an identical, shared branch list).

## Model

Worktrees belong to the **active repo**, not the top-level repo switcher. Branches and
the commit graph are **shared** across all worktrees of a repository (same refs, same
object DB). "Focusing" a worktree therefore changes only three things:

1. the working-changes (status) panel,
2. where write actions run (checkout / stage / unstage / discard / diff),
3. which branch shows as `●` current.

Scope: **view + navigate only**. No create / remove / prune in v1.

## UI

- The left panel gains a `Branches | Worktrees` **segmented toggle**. The shared commit
  graph on the right does not move when the lens flips.
- **Worktrees tab** — one row per worktree: branch (or short SHA if detached),
  ahead/behind (joined from the branch list), a dirty dot, focused row highlighted.
  Row interaction is **split**:
  - click the **name** → *focus* the worktree in-app,
  - click **↗** → *open with the default open-target*,
  - **right-click** → "Open with…" (full target list) + Copy path.
- **Branches tab** — today's view, plus a small **badge** on any branch checked out in a
  worktree, and the "Current" group reflects the **focused** worktree's branch.
- **Header** — a compact pill shows the focused worktree when it isn't the main one;
  clicking it resets focus to main.

## Open targets

A configurable list, replacing hardcoded editor/terminal command settings:

```rust
struct OpenTarget { id: String, name: String, command: String } // command holds {path}
```

Seeded defaults: Editor `code {path}`, Terminal `open -a Terminal {path}`, Finder
`open {path}`. One target is marked **default** (used by the one-click ↗). Users add
their own, e.g. `open -a Ghostty {path}`. `{path}` is substituted as a single argument
(whitespace-tokenized command), so paths with spaces are safe.

## Backend (Rust)

Data model (`git.rs`):

```rust
struct WorktreeInfo {
    path: String,
    branch: Option<String>, // None = detached
    head: String,           // short SHA
    is_main: bool,
    dirty: bool,
    locked: bool,
    prunable: bool,
}
```

`ahead`/`behind` are **not** stored here — the frontend joins them from the existing
branch list by branch name (zero extra git cost).

Commands:

- **`get_worktrees(repo_id) -> Vec<WorktreeInfo>`** — `git worktree list --porcelain`,
  parse blocks, then `dirty` per worktree via `git -C <path> status --porcelain`
  non-empty. (ponytail: N status calls, one per worktree — fine for the handful in
  practice; cache/batch only if it profiles hot.)
- **Worktree-cwd routing** on `get_status`, `checkout`, `stage_files`, `unstage_files`,
  `discard_files`, `diff_file`: add an optional `worktreePath`. A helper returns the
  repo path when absent, else validates that `p` is a worktree of this repo
  (`git -C p rev-parse --git-common-dir` matches the repo's common dir) and uses it as
  the cwd. The `git.rs` functions are **unchanged** — they already take the cwd as their
  first arg (`git -C <dir>`).
- **`open_worktree(repo_id, worktree_path, target_id)`** — validate the worktree, then
  `std::process::Command` spawns the target's template with `{path}` filled. No new Tauri
  plugin required.

Settings (`state.rs`) gains, both `#[serde(default)]` for back-compat:

```rust
open_targets: Vec<OpenTarget>,        // seeded Editor / Terminal / Finder
default_open_target: Option<String>,  // OpenTarget id
```

Watcher: **unchanged**. It already recursively watches `.git`, which contains
`worktrees/<name>/HEAD`, so external checkouts in any worktree already fire
`repo-changed`. Working-dir file edits are not watched today either (main repo included),
so dirty flags refresh on git ops / manual refresh — consistent, not a regression.

## Frontend

- `lib/ipc.ts` — `WorktreeInfo` / `OpenTarget` types, `getWorktrees`, `openWorktree`,
  optional `worktreePath` on the six worktree-sensitive commands, Settings additions.
- `App.tsx` — owns `worktrees` and `focusedWorktree` state (defaults to the main
  worktree, resets on repo switch, self-heals to main if the focused path vanishes from
  `get_worktrees`); passes both down.
- `components/BranchPane.tsx` — hosts the toggle; renders the branch content or the new
  `WorktreePane.tsx`. Branch tab gains the worktree badge and focused-worktree-driven
  "Current".
- `components/WorktreePane.tsx` (new) — the Worktrees tab: rows, focus, ↗ / right-click.
- `components/CommitGraph.tsx` — the status panel and stage/unstage/discard/diff pass
  `focusedWorktree.path`.
- `components/Header.tsx` — the focus pill.
- `components/SettingsView.tsx` — open-targets editor (add/remove rows, edit name +
  command, mark default).

## Error handling / edge cases

- **Checkout collision** (branch already checked out in another worktree): git's own
  clear error surfaces via the existing toast; the badge is the pre-emptive visual cue.
  Checkout stays enabled — git is the source of truth.
- **Detached / bare / prunable** worktrees: shown, greyed where appropriate; actions that
  don't apply error gracefully.
- **Missing editor** (`code` not on PATH) or **stale path**: spawn/status fails → error
  toast.
- **Focused worktree pruned while focused**: next `get_worktrees` omits it → focus
  self-heals to main.

## Testing (matches existing `git.rs` test style)

- `parse_worktree_list` on a sample porcelain blob (main + linked + detached + locked).
- `{path}` template substitution, including a path with spaces.
- Settings back-compat: an old config (no open-target fields) loads with seeded targets.

## Build order

1. Backend read-only — `WorktreeInfo` + parser + `get_worktrees` (+ unit test).
2. Frontend read-only — toggle, Worktrees tab (focus highlight, dirty, ahead/behind
   join), Branches-tab badge. Focus drives only the `●` marker at this stage.
3. Focus routing — thread `worktreePath` through the six commands; header pill; self-heal.
4. Open targets — settings list editor + `open_worktree` + ↗ / right-click menu.
5. Tests + polish.

## Deliberately skipped (add when needed)

- Create / remove / prune worktrees.
- Cross-repo "all worktrees" global view (per-repo only for now).
- Live FS-watching of worktree working dirs for dirty status.
- Caching the worktree set.

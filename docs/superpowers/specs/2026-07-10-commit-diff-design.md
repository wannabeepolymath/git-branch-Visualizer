# Commit Diff Viewing — Design

Date: 2026-07-10
Branch: `feat/commit-diff` (from `main`)

## Goal

In the commit detail panel, clicking a changed file expands its diff inline —
the same interaction the working-tree changes panel already has.

## Backend (Rust)

One new function in `src-tauri/src/git.rs`:

```rust
pub fn diff_commit_file(repo: &str, hash: &str, path: &str) -> Result<String, String> {
    git(repo, &["show", "--format=", "--diff-merges=first-parent", hash, "--", path])
}
```

- `git show` rather than `git diff <hash>^ <hash>`: handles root commits (no
  parent) for free.
- `--diff-merges=first-parent` matches how the detail panel's file list is
  already produced (`get_commit`), so the diff shown always corresponds to a
  listed file.

One new Tauri command in `src-tauri/src/commands.rs`, shaped like `get_commit`
(repo path only — commits are worktree-independent, no `worktree_path`
routing):

```rust
#[tauri::command]
pub async fn diff_commit_file(state, repo_id: String, hash: String, path: String) -> Result<String, String>
```

Registered in the invoke handler in `lib.rs`.

## IPC (TypeScript)

One wrapper in `src/lib/ipc.ts`:

```ts
export const diffCommitFile = (repoId: string, hash: string, path: string): Promise<string>
```

## Frontend

In `DetailPanel` (`src/components/CommitGraph.tsx`):

- Make each `FileRow` clickable (button semantics, hover state, chevron/affordance
  consistent with the working-changes rows).
- State: `openDiff: string | null` (file path) and `diffText: string | null`.
- Click toggles: same path closes; new path sets `openDiff`, clears `diffText`,
  fetches via `diffCommitFile`, renders with the existing `DiffView` under the
  row (loading and empty states come free from `DiffView`).
- Reset both states when the displayed commit changes.
- `DetailPanel` needs `repoId` passed in (currently only gets `detail`).

## Edge cases

- Binary files: git prints "Binary files … differ"; `DiffView` already styles it.
- Renames (`R` status): `git show <hash> -- <newpath>` shows the rename diff.
- Large diffs: `DiffView` truncates at 600 lines; panel scrolls.
- Errors: surfaced via the existing toast, same as `getCommit`.

## Testing

- `bun run check` (typecheck + graph self-check).
- `cargo check` in `src-tauri`.
- Manual: expand a file diff on a normal commit, a merge commit, a root commit,
  and a binary change.

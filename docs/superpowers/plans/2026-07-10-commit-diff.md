# Commit Diff Viewing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clicking a changed file in the commit detail panel expands its diff inline, matching the interaction the working-tree changes panel already has.

**Architecture:** One new git invocation (`git show --format= --diff-merges=first-parent <hash> -- <path>`) exposed as a Tauri command, one typed IPC wrapper, and expand/collapse state in the existing `DetailPanel` component reusing the existing `FileRow` and `DiffView` components.

**Tech Stack:** Rust (Tauri 2 backend, shells out to git), TypeScript + React 19 frontend, Tailwind 4.

## Global Constraints

- Work happens on branch `feat/commit-diff` (already created from `main`; the spec is committed there).
- Spec: `docs/superpowers/specs/2026-07-10-commit-diff-design.md`.
- No new dependencies.
- No `any` in TypeScript.
- Errors are human-readable strings surfaced via the existing toast.
- This repo has no Rust or React test framework; do not add one. Each task verifies with the project's check commands (`cargo check`, `bun run check`) plus the exact manual verifications listed in its steps.
- Verification commands: `cargo check` runs in `src-tauri/`; `bun run check` runs at the repo root.
- Do NOT run `bun run dev`, `bun run build`, or `bun run tauri` — the dev server is assumed running.

**Spec correction:** The spec says the detail panel's file list is already produced with `--diff-merges=first-parent`. It is not — the current call (`src-tauri/src/git.rs:368`) has no diff-merges flag, so for merge commits `git show --name-status` uses the combined diff, which lists (almost) no files. Task 1 adds the flag to that existing call as well as to the new diff function, making the spec's claim true and merge commits useful.

---

### Task 1: Backend — commit file diff command

**Files:**
- Modify: `src-tauri/src/git.rs` (add function after `diff_file`, ~line 408; add flag at line 368)
- Modify: `src-tauri/src/commands.rs` (add command after `get_commit`, ~line 268)
- Modify: `src-tauri/src/lib.rs` (register in `generate_handler!`, line 52–77)

**Interfaces:**
- Consumes: existing `fn git(repo: &str, args: &[&str]) -> Result<String, String>` (git.rs:96), `AppState::repo_path` (as used by `get_commit`, commands.rs:260–268).
- Produces: Tauri command `diff_commit_file(repo_id: String, hash: String, path: String) -> Result<String, String>` returning a unified diff string. Task 2 invokes it as `invoke("diff_commit_file", { repoId, hash, path })`.

- [ ] **Step 1: Add `--diff-merges=first-parent` to the existing file-list call**

In `src-tauri/src/git.rs`, change line 368 from:

```rust
    let names = git(repo, &["show", "--name-status", "--format=", hash])?;
```

to:

```rust
    // --diff-merges=first-parent: plain `git show` uses the combined diff for
    // merges, which lists (almost) no files; the diff vs the first parent is
    // what the detail panel should show. No effect on non-merge commits.
    let names = git(
        repo,
        &["show", "--name-status", "--diff-merges=first-parent", "--format=", hash],
    )?;
```

- [ ] **Step 2: Add `diff_commit_file` to git.rs**

Insert after the `diff_file` function (after line 408 in `src-tauri/src/git.rs`):

```rust
/// Unified diff for one path as changed by a commit (vs its first parent).
/// `git show` handles root commits (no parent) for free; first-parent keeps
/// merge diffs consistent with the file list from `get_commit`.
pub fn diff_commit_file(repo: &str, hash: &str, path: &str) -> Result<String, String> {
    git(
        repo,
        &["show", "--format=", "--diff-merges=first-parent", hash, "--", path],
    )
}
```

- [ ] **Step 3: Add the Tauri command**

Insert after the `get_commit` command (after line 268 in `src-tauri/src/commands.rs`):

```rust
#[tauri::command]
pub async fn diff_commit_file(
    state: State<'_, AppState>,
    repo_id: String,
    hash: String,
    path: String,
) -> Result<String, String> {
    let repo = state.repo_path(&repo_id)?;
    git::diff_commit_file(&repo, &hash, &path)
}
```

(No `worktree_path` routing — commits are worktree-independent, same as `get_commit`.)

- [ ] **Step 4: Register the command**

In `src-tauri/src/lib.rs`, inside `tauri::generate_handler![...]`, add after `commands::diff_file,` (line 66):

```rust
            commands::diff_commit_file,
```

- [ ] **Step 5: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: finishes with no errors (warnings unrelated to the new code are acceptable only if they pre-exist).

- [ ] **Step 6: Verify the git invocation behaves**

Run these against this repo itself (any repo works). First find a merge commit and a normal commit:

```bash
cd /Users/daksh/mySpace/code/branch-Visualizer
git log --merges -1 --format=%H            # merge hash (may be empty if no merges — skip merge check then)
git log --no-merges -1 --format=%H         # normal hash
git rev-list --max-parents=0 HEAD          # root commit hash
```

Then for the normal commit (substitute HASH and a path listed by `git show --name-status --format= HASH`):

```bash
git show --format= --diff-merges=first-parent HASH -- PATH
```

Expected: a unified diff starting with `diff --git`, only for PATH.

For the root commit: same command; expected: whole-file addition diff (no error about missing parent).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/git.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "Add diff_commit_file command; first-parent diffs for merge file lists

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: Frontend — expandable per-file diffs in the commit detail panel

**Files:**
- Modify: `src/lib/ipc.ts` (add wrapper after `getCommit`, line 131–132)
- Modify: `src/components/CommitGraph.tsx` (`DetailPanel`, lines 233–288; call site line 762; import list lines 11–26)

**Interfaces:**
- Consumes: Tauri command `diff_commit_file` from Task 1 via `invoke("diff_commit_file", { repoId, hash, path })`; existing `FileRow` (`onToggleDiff` prop, CommitGraph.tsx:136) and `DiffView` (CommitGraph.tsx:212) components.
- Produces: user-facing behavior only; nothing downstream consumes this.

- [ ] **Step 1: Add the IPC wrapper**

In `src/lib/ipc.ts`, insert after the `getCommit` export (line 132):

```ts
/** Unified diff for one path as changed by a commit (vs its first parent). */
export const diffCommitFile = (repoId: string, hash: string, path: string): Promise<string> =>
  invoke("diff_commit_file", { repoId, hash, path });
```

- [ ] **Step 2: Import it in CommitGraph.tsx**

In the `../lib/ipc` import block (lines 12–26 of `src/components/CommitGraph.tsx`), the names are alphabetized; `diffCommitFile` sorts before `diffFile`:

```ts
  createBranch,
  diffCommitFile,
  diffFile,
  discardFiles,
```

- [ ] **Step 3: Rework `DetailPanel` to fetch and show per-file diffs**

Replace the whole `DetailPanel` function (`src/components/CommitGraph.tsx` lines 233–288) with:

```tsx
function DetailPanel({
  top,
  repoId,
  detail,
  onToast,
}: {
  top: number;
  repoId: string;
  detail: CommitDetail | null;
  onToast: (msg: string) => void;
}) {
  const [openDiff, setOpenDiff] = useState<string | null>(null);
  const [diffText, setDiffText] = useState<string | null>(null);

  // Collapse any open diff when the panel switches to another commit.
  const hash = detail?.hash;
  useEffect(() => {
    setOpenDiff(null);
    setDiffText(null);
  }, [hash]);

  const toggleDiff = (path: string) => {
    if (!hash) return;
    if (openDiff === path) {
      setOpenDiff(null);
      setDiffText(null);
      return;
    }
    setOpenDiff(path);
    setDiffText(null); // DiffView renders its loading state on null
    diffCommitFile(repoId, hash, path)
      .then(setDiffText)
      .catch((e: unknown) => {
        onToast(String(e));
        setOpenDiff(null);
      });
  };

  return (
    <div
      className="absolute inset-x-0 z-10 overflow-y-auto border-y border-edge bg-panel2 px-3 py-2"
      style={{ top, height: DETAIL_H }}
      onContextMenu={(e) => e.stopPropagation()}
    >
      {!detail ? (
        <div className="text-[11px] text-faint">Loading…</div>
      ) : (
        <>
          <div className="flex items-center gap-2">
            <span className="font-mono text-[11px] text-muted select-text">
              {detail.hash.slice(0, 12)}
            </span>
            <button
              className="rounded border border-edge px-1.5 text-[10px] leading-[16px] text-muted hover:bg-hover"
              onClick={() => {
                void navigator.clipboard.writeText(detail.hash);
                onToast("Hash copied");
              }}
            >
              Copy hash
            </button>
          </div>
          <div className="mt-1.5 text-[12px] font-medium select-text">{detail.subject}</div>
          {detail.body.trim() !== "" && (
            <pre className="mt-1 font-sans text-[11.5px] whitespace-pre-wrap text-muted select-text">
              {detail.body.trim()}
            </pre>
          )}
          <div className="mt-1.5 text-[11px] text-muted select-text">
            {`${detail.authorName} <${detail.authorEmail}>`}
            <span className="mx-1 text-faint">·</span>
            {new Date(detail.timestamp * 1000).toLocaleString()}
          </div>
          <div className="mt-2 border-t border-edge pt-1.5">
            {detail.files.length === 0 ? (
              <div className="text-[11px] text-faint">No files changed</div>
            ) : (
              detail.files.map((f) => (
                <div key={f.path}>
                  <FileRow f={f} onToggleDiff={() => toggleDiff(f.path)} />
                  {openDiff === f.path && <DiffView text={diffText} />}
                </div>
              ))
            )}
          </div>
        </>
      )}
    </div>
  );
}
```

Notes for the implementer:
- `FileRow` already takes an optional `onToggleDiff` and turns the row into a
  pointer-cursor click target (CommitGraph.tsx:136–172) — no changes to it.
- `DiffView` already handles `null` → "Loading diff…", empty → "No textual
  changes", and caps rendering at 600 lines (CommitGraph.tsx:212–231).
- The panel keeps its fixed height (`DETAIL_H`) and scrolls internally, so
  expanding a diff does not disturb the list virtualization math.

- [ ] **Step 4: Pass `repoId` at the call site**

At `src/components/CommitGraph.tsx` line 762, change:

```tsx
              <DetailPanel top={rowTop(expIdx) + ROW_H} detail={detail} onToast={onToast} />
```

to:

```tsx
              <DetailPanel
                top={rowTop(expIdx) + ROW_H}
                repoId={repoId}
                detail={detail}
                onToast={onToast}
              />
```

(The enclosing component already has `repoId` in scope — it uses it for `checkout` at line 679.)

- [ ] **Step 5: Verify types and checks pass**

Run at repo root: `bun run check`
Expected: tsc exits clean and `graph.check.ts` assertions pass, no output errors.

- [ ] **Step 6: Manual verification in the running app**

In the dev app (already running):
1. Expand a normal commit → click a file row → diff appears under the row; click again → collapses.
2. Click a different file while one is open → first closes, second opens with its own diff.
3. Expand a merge commit → files are listed (first-parent) and each shows a diff.
4. Expand the root commit → files diff as whole-file additions.
5. Switch to another commit while a diff is open → new panel opens with no diff expanded.

Expected: all five behave as described; errors (if any) appear as toasts.

- [ ] **Step 7: Commit**

```bash
git add src/lib/ipc.ts src/components/CommitGraph.tsx
git commit -m "Expand per-file diffs in the commit detail panel

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

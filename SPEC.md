# Branch Visualizer — Spec

A menu bar app that shows a git repository's branches and commit graph, IDE-style, one click (or hotkey) away.

## 1. Product overview

- Lives in the macOS menu bar (system tray). No Dock icon, no regular window by default.
- Click the tray icon → a popover/dropdown opens under it showing the branch visualizer.
- A user-configurable global shortcut (default: `⌥⇧G`) toggles the same popover from anywhere.
- Read-mostly tool: its job is *seeing* the repo clearly. Light write actions (checkout, create/delete branch, fetch/pull) included; heavy operations (rebase, merge conflict resolution) are out of scope for v1.

## 2. UI

### Tray icon
- Monochrome template icon (git-branch glyph) that adapts to light/dark menu bar.
- Optional badge/dot when the current branch is ahead/behind its remote.

### Popover (roughly 420 × 560 px, resizable)

```
┌────────────────────────────────────────┐
│ ▾ my-repo (~/code/my-repo)        ⚙ ⟳ │  ← repo switcher, settings, fetch
│ 🔍 filter branches…                    │
├──────────────┬─────────────────────────┤
│ BRANCHES     │ COMMIT GRAPH            │
│ ● main       │ ●─┐ feat: add login     │
│   develop    │ │ ● fix: typo           │
│ ▸ feature/…  │ ●─┘ merge develop       │
│ ▸ remotes/…  │ ●   chore: deps         │
└──────────────┴─────────────────────────┘
```

- **Left pane — branch list.** Grouped: current (highlighted), local, remotes (collapsible), tags. Each row shows name, ahead/behind counts (`↑2 ↓1`), last-commit relative time. Filter box on top.
- **Right pane — commit graph.** Vertical rail-style DAG (like VS Code's Git Graph / IntelliJ log): colored lanes, merge lines, commit message, author, short hash, branch/tag labels as pills. Infinite scroll, loads ~200 commits at a time. Selecting a branch on the left scrolls/filters the graph to it.
- **Commit detail.** Clicking a commit expands an inline panel: full message, author, date, changed files list. Copy-hash button.
- **Context menus.**
  - Branch: Checkout, New branch from here, Rename, Delete, Copy name.
  - Commit: Copy hash, Copy message, Create branch here, Checkout (detached).
- Destructive actions (delete branch) confirm first.

### Settings (⚙ → separate small window)
- Repositories: add/remove watched repos (folder picker), pick active one.
- Global shortcut recorder.
- Launch at login.
- Theme: system / light / dark.
- Graph options: date format, show remote branches by default, commits per page.

## 3. Features (v1)

| Feature | Notes |
|---|---|
| Multi-repo | User registers repos; quick switcher in header. Auto-detect repo of frontmost editor is a later nice-to-have. |
| Live refresh | FS watcher on `.git/` (HEAD, refs, packed-refs) → refresh branch list/graph automatically. |
| Fetch / pull | Buttons in header; uses system git so existing SSH keys/credential helpers just work. |
| Branch ops | checkout / create / rename / delete via context menu. |
| Search | Filter branches; later: search commits by message/author. |

Out of scope v1: staging/committing, diffs, rebase/merge UI, GitHub PR integration.

## 4. Implementation

### Stack: Tauri v2 + React + TypeScript + Tailwind

Chosen specifically for the cross-platform requirement:

- **Tauri v2 (Rust shell).** First-class tray icon, positioned tray popover, global shortcuts, autostart — all via official plugins, all working on **macOS, Windows, and Linux**. ~10 MB binary vs Electron's ~150 MB.
- **React + TS + Tailwind frontend** renders everything inside the popover — the entire UI is platform-independent by construction.
- **Git access: shell out to the system `git` binary** from the Rust side (`git log --format=… --parents`, `git for-each-ref`, `git branch`, etc.) and parse the stable machine-readable output. No libgit2 binding to maintain, credentials/SSH work for free, identical on all platforms. Swap in `git2` (libgit2) later only if process-spawn latency ever becomes a measured problem.

### Architecture

```
┌───────────────────────────────┐
│ Frontend (React/TS/Tailwind)  │  branch list, graph renderer (SVG lanes),
│  – pure UI, platform-agnostic │  settings screens
└──────────────┬────────────────┘
               │ Tauri IPC (typed commands + events)
┌──────────────┴────────────────┐
│ Rust core                     │
│  git.rs      – run git, parse into Branch/Commit structs
│  watcher.rs  – notify(.git) → emit "repo-changed" event
│  state.rs    – registered repos, settings (JSON in app-config dir)
│  platform glue: tray, popover positioning, global shortcut,
│                 autostart (Tauri plugins, feature-gated per OS)
└───────────────────────────────┘
```

IPC surface (small, typed): `list_repos`, `get_branches(repo)`, `get_log(repo, ref?, skip, limit)`, `get_commit(repo, hash)`, `checkout`, `create_branch`, `rename_branch`, `delete_branch`, `fetch`, `pull`, `get_settings`, `set_settings`. Event: `repo-changed`.

Graph layout (lane assignment from parent hashes) is a well-known small algorithm — implement in TS in the frontend (~150 lines), render as absolutely-positioned SVG rails + HTML rows for virtualized scrolling.

### Cross-platform plan (Windows / Linux)

Everything above is cross-platform already; the per-OS surface is deliberately tiny:

| Concern | macOS (v1) | Windows / Linux (later) |
|---|---|---|
| Tray + popover | NSStatusItem via Tauri tray, popover anchored to icon | Same Tauri API; Linux needs AppIndicator note in docs |
| Global shortcut | tauri-plugin-global-shortcut | same plugin, both OSes |
| Autostart | tauri-plugin-autostart (LaunchAgent) | same plugin (registry / .desktop) |
| Default hotkey | `⌥⇧G` | `Ctrl+Shift+G` (define per-platform constant) |
| git binary | assume present; onboarding check with install hint | same check; Windows hint → Git for Windows |

Rules to keep the port cheap:
1. **No platform logic in the frontend** — UI reads capabilities/settings via IPC only.
2. All OS-specific code isolated behind Tauri plugins or `#[cfg(target_os)]` in a single `platform` module.
3. Keyboard labels and default shortcuts come from one per-platform constants file.

### Milestones

1. **M1 – Skeleton:** Tauri tray app, popover opens on click + global shortcut, hardcoded repo, plain branch list.
2. **M2 – Graph:** commit log parsing, lane layout, virtualized graph, commit detail.
3. **M3 – Actions & live updates:** branch context-menu ops, fetch/pull, FS watcher refresh.
4. **M4 – Polish:** multi-repo, settings window, shortcut recorder, autostart, onboarding (git check), signing + notarization, auto-update (tauri-plugin-updater).

## 5. Risks

- **Tray popover focus quirks** differ per OS (dismiss-on-blur behavior) — keep popover logic in one Rust module.
- **Huge repos:** always paginate `git log`; never load full history.
- **macOS signing/notarization** required for distribution outside the App Store — budget it into M4.

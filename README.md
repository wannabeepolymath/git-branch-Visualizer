# Branch Visualizer

A macOS menu bar app that shows your git repositories' branches and commit graph, IDE-style — one click or hotkey away. Built with Tauri v2, React, TypeScript, and Tailwind. See [SPEC.md](SPEC.md) for the full product/architecture spec.

![platform](https://img.shields.io/badge/platform-macOS-blue) (Windows/Linux planned — the codebase is structured for it)

## Requirements

- macOS
- [git](https://git-scm.com) on your PATH (the app shells out to your system git, so your existing SSH keys and credential helpers just work)
- To build from source: [Rust](https://rustup.rs) (stable) and [Bun](https://bun.sh)

## Run from source

```sh
bun install        # once
bun run tauri dev  # compiles the Rust shell + starts Vite, launches the app
```

The first build compiles all Rust dependencies and takes a few minutes; subsequent runs are fast. Frontend changes hot-reload instantly; Rust changes trigger an automatic rebuild + relaunch.

## Build a release app

```sh
bun run tauri build
```

The `.app` bundle and `.dmg` land in `src-tauri/target/release/bundle/`. (Distribution outside your machine needs code signing/notarization — not set up yet.)

## Using the app

The app lives in the **menu bar only** — no Dock icon, no regular window.

- **Open/close the popover:** click the menu bar icon, or press **⌥⇧G** (configurable). It closes automatically when it loses focus.
- **Add a repository:** click the repo name (top-left) → *Add repository…*, or use the button on the empty state / in Settings. Pick any folder inside a git working tree.
- **Switch repositories:** repo dropdown, top-left.

### Branch panel (left)

- Branches grouped as **Current / Local / Remotes** (collapsible), each with ahead/behind counts (`↑2 ↓1`) and last-commit age. Filter box on top.
- Click a branch to filter the commit graph to it; *All branches* shows everything.
- **Right-click a branch** for: Checkout, New branch from here…, Rename… (local only), Delete… (local only, safe delete — unmerged branches are refused with git's error), Copy name.
- Toggle the panel with the panel icon in the header; drag the divider to resize it. Both are remembered.

### Commit graph (right)

- Colored-lane DAG of commits with merge/fork connectors, branch/tag pills, short hash, and age. Scroll to load more (paginated).
- **Click a commit** to expand its details: full message, author, date, changed files, copy-hash button.
- **Right-click a commit** for: Copy hash, Copy message, Create branch here…, Checkout (detached).

### Header actions

- **Fetch** (`git fetch --all --prune`) and **Pull** (`git pull --ff-only`) for the active repo.
- The view refreshes automatically whenever the repository changes on disk (commits, checkouts, fetches from anywhere — a file watcher on `.git` keeps it live).

### Settings (gear icon)

- Manage registered repositories.
- **Global shortcut:** click the field and press a new combo (e.g. ⌥⇧G → `Alt+Shift+G`).
- **Launch at login**, **theme** (system/light/dark), **commits per page** (50–1000), **show remote branches**.

Settings are stored in `~/Library/Application Support/com.branchvisualizer.app/settings.json`.

## Development

```sh
bunx tsc --noEmit             # typecheck frontend
bun src/lib/graph.check.ts    # graph lane-layout self-checks
cd src-tauri && cargo check   # compile-check Rust
cd src-tauri && cargo test    # git output parser tests
```

Layout: `src/` is the React UI (all platform-agnostic; talks to the backend only through the typed IPC wrappers in `src/lib/ipc.ts`), `src-tauri/src/` is the Rust shell — `git.rs` (run/parse git), `state.rs` (settings), `watcher.rs` (live refresh), `commands.rs` (IPC surface), and `platform.rs`, the **only** file with OS-specific code, which is what will make the Windows/Linux ports cheap.

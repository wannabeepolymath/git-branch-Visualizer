# Linux port — design

Run Branch Visualizer on Linux without pretending it is the same app it is on macOS.

## Problem

The README has claimed since v1.0.0 that the codebase is "structured for" Windows and
Linux, on the strength of `platform.rs` isolating the OS-specific code. That is half
true. `git.rs`, `state.rs`, `watcher.rs`, `open.rs` and the entire React UI are just git
and files; they port unchanged. What does not port is the way you *open* the app, and
that is not a translation job — three separate crates refuse, and they compound:

1. **Tray clicks do not exist.** `tray-icon` 0.24 (`src/lib.rs:550`): *"Linux:
   Unsupported. The event is not emitted even though the icon is shown and will still
   show a context menu on right click."* Our only entry point is
   `TrayIconEvent::Click` → `toggle_popover` (`lib.rs:141`). On Linux that closure never
   runs. Appindicators support menus, not click callbacks.

2. **The popover cannot be anchored.** `Position::TrayCenter` reads a tray position
   cached by `on_tray_event`, which never fires, so `tauri-plugin-positioner` returns
   `Err("Tray position not set")` (`ext.rs:229`). This breaks two callers, not one:
   `toggle_popover` (`lib.rs:35`) and `commands::recenter_window` (`commands.rs:73`).

3. **The hotkey is X11-only.** `global-hotkey` 0.8 (`src/lib.rs:13`) states "Linux (X11
   Only)". Wayland is the default session on Ubuntu, Fedora, GNOME and KDE Plasma 6.

Worse than any of them individually: **GNOME Shell has shipped without a system tray
since 3.26 (2017).** Tray icons need the AppIndicator extension. Ubuntu bundles it;
stock Fedora and Debian GNOME do not. So a stock Fedora GNOME Wayland user gets no tray
icon, no working hotkey, and therefore no way to open the app at all.

Any design that keeps the menu-bar identity on Linux ships an app a large fraction of
Linux users cannot launch.

## Model

**On Linux the app is a normal window by default, and a popover only if you ask.**

The two modes are not two interaction models. They are one window with five flags, all
of which Tauri can set at runtime (`set_decorations`, `set_always_on_top`,
`set_skip_taskbar`, `center`):

| | `popover` | `window` |
| --- | --- | --- |
| `decorations` | false | **true** |
| hide on blur | yes | **no** |
| `skipTaskbar` | true | **false** |
| `alwaysOnTop` | true | **false** |
| position on show | anchored, first show only | remembered |

New setting `windowMode: "popover" | "window"`, defaulting to `popover` on macOS and
`window` on Linux. Switching is a settings write plus four calls — no second window, no
restart. **The setting row renders only on Linux.** macOS gains no new surface; its
popover already works and a window mode there would be UI nobody asked for.

Default is `window` because it is the only mode that is always reachable: a `.desktop`
entry works on every desktop, tray or no tray, X11 or Wayland.

## Backend (Rust)

`platform.rs` stays a single file with `#[cfg]` blocks — the pattern already in the
codebase. It grows from 42 to roughly 150 lines. Split it into a module directory only
if it passes ~200; a directory for three functions is scaffolding.

Three new functions, and `lib.rs` stops touching tray events directly:

| Function | macOS | Linux |
| --- | --- | --- |
| `build_tray(app) -> Result<TrayIcon>` | click-to-toggle, no menu | menu only |
| `anchor_window(&Window)` | `Position::TrayCenter` | `window.center()` |
| `shortcut_supported() -> bool` | `true` | `false` under Wayland |

`anchor_window` is the root-cause fix for breakage 2: both `toggle_popover` and
`recenter_window` route through it, so neither calls `Position::TrayCenter` directly
any more. Fixing only the one the bug report names would leave `recenter_window`
broken.

**Linux tray menu:** Show / Settings / Check for updates / Quit. Built once —
`tray-icon` notes that on Linux "once a menu is set, it cannot be removed". If no tray
host exists (stock GNOME), the icon silently does not appear and the app stays
reachable through its launcher. That silence is exactly why `window` is the default.
A missing tray host is not an error: `build_tray` returns `Err` only on real failure,
and the app must start normally when the icon never shows.

**Closing the window.** `window` mode introduces a titlebar close button the app has
never had. It **quits the app**, the way any ordinary desktop app does. The tempting
alternative — hide on close, since we are a tray app — is a trap: on stock GNOME there
is no tray to restore from and no Wayland hotkey, so hiding would leave the app running,
invisible and unquittable. `popover` mode has no decorations, so the question does not
arise there.

**Shortcut:** registration stays best-effort as today. `shortcut_supported()` returns
false when `XDG_SESSION_TYPE == "wayland"`. X11 hotkeys do not reliably capture from
Wayland-native windows even under XWayland, so "sometimes works" is worse than an
honest no. The check is a pure function over an env var so it is unit-testable without
a session.

## Frontend

- `data-platform="linux"` on `<html>`, set from the existing settings payload.
- Settings gains the **Window mode** radio pair, rendered only when platform is Linux.
- The shortcut recorder disables itself when `shortcutSupported` is false, with
  *"Global shortcuts require an X11 session."*

## IPC (TypeScript)

Two additions to the settings payload, both read-only from the frontend's view:

```ts
platform: "macos" | "linux";
shortcutSupported: boolean;
```

and one persisted field:

```ts
windowMode: "popover" | "window";
```

## Transparency

`set_transparent` does not exist in Tauri v2 — transparency is create-time only, and
`tauri.conf.json` window config is not per-platform. So the window is created
transparent everywhere. On Linux without a compositor that paints black corners.

Fixed in CSS rather than Rust: under `[data-platform="linux"]`, `#root` drops to
`border-radius: 0`. The opaque root then fills the frame and the transparent edge is
never visible, compositor or not. Three lines, and no per-platform window construction
in `setup()`.

## Packaging and updates

Artifacts: **deb, rpm, AppImage**, x86_64 only for beta.

All three self-update. This was worth verifying because it is counterintuitive:
`tauri-plugin-updater` has real `install_deb` and `install_rpm` paths
(`updater.rs:1049-1065`), and the bundler stamps `__TAURI_BUNDLE_TYPE` into each
artifact at bundle time, so a single compiled binary knows which format it was shipped
as (`tauri-utils/src/platform.rs:353`).

| Format | Update mechanism | User sees |
| --- | --- | --- |
| AppImage | file swapped in place | nothing |
| deb | `pkexec dpkg -i` | polkit password prompt |
| rpm | `pkexec rpm -U` | polkit password prompt |

So Settings → Updates keeps one button on every platform. Copy gains one line about the
password prompt.

**Launcher entries differ, and the README must say so.** deb and rpm install a
`.desktop` file, so the app appears in the applications menu on install. AppImage does
not — it needs AppImageLauncher or a hand-written `.desktop` file. Since launching from
the applications menu is the primary entry point in `window` mode, deb/rpm are the
recommended install and AppImage is the distro-agnostic fallback.

**CI:**

- `release.yml` gains an `ubuntu-22.04` matrix leg. **22.04 deliberately, not
  `ubuntu-latest`** — building against 24.04's glibc produces a binary that will not
  start on older distros.
- Runner system deps: `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`,
  `librsvg2-dev`, `libgtk-3-dev`, `libssl-dev`, `build-essential`.
- `latest.json` gains a `linux-x86_64` entry.
- `ci.yml` gains a Linux `cargo check` + `cargo test` job so compile breaks surface on
  PRs rather than at tag time.

## Testing

The existing 26 tests cover `git.rs`, `state.rs` and `open.rs` — the platform-agnostic
90% — and keep passing unchanged. One test is added: the Wayland-detection helper,
written as a pure function over env vars.

There is no automated UX test and there will not be one. CI proves it compiles and
bundles; it cannot prove the tray menu appears or the window feels right.

## Shipping posture

Linux ships marked **beta**, built by CI and not validated on hardware — nobody on this
project owns a Linux machine. The README says so plainly, with the known limitations
listed: no Wayland hotkey, GNOME needs the AppIndicator extension for tray mode, tray
icon click does nothing by design. First real feedback comes from issue reports.

Claiming parity we have not tested would earn worse bug reports than admitting beta.

## Non-goals

- **Flatpak.** The sandbox fights shelling out to system git with the user's SSH keys
  and credential helpers, which is the app's whole premise. Not worth the fight.
- **XDG portal global shortcuts** (`org.freedesktop.portal.GlobalShortcuts`). The
  plugin does not speak it. Revisit if Wayland users ask.
- **aarch64 Linux.** Desktop Linux is overwhelmingly x86_64; add it when asked.
- **Windows.** Separate spec. `platform.rs` is shaped for it but nothing here assumes
  it.

## Alternatives considered

**Keep the tray popover, same as macOS.** Rejected: unopenable on stock Fedora/Debian
GNOME under Wayland, and the tray icon could never toggle directly regardless of
desktop. Preserving the macOS feel is not worth an app a user cannot start.

**Regular window only, no popover mode.** Simpler, and honestly close to right. Rejected
because the popover costs five runtime flags once the window already exists — the
saving was not real.

**A `PlatformShell` trait with per-OS impls.** Rejected as scaffolding: one
implementation per platform, selected at compile time, is what `#[cfg]` already does.

## Build order

1. `platform::anchor_window` + reroute both `TrayCenter` callers. Fixes breakage 2 on
   its own and is testable on macOS today.
2. `platform::build_tray`, macOS behaviour unchanged, Linux menu added.
3. `windowMode` setting: state, IPC, the four runtime flags.
4. `shortcut_supported` + Settings gating + the unit test.
5. `data-platform` plumbing and the Linux `border-radius: 0` rule.
6. CI Linux job, then the `release.yml` matrix leg and `latest.json` entry.
7. README: Linux beta section and known limitations.

Steps 1-2 are shippable on macOS with no visible change, which is the point — the port
lands as ordinary refactors before any Linux-only code exists.

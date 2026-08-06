#!/bin/sh
# Install Branch Visualizer on Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/wannabeepolymath/git-branch-Visualizer/main/install.sh | sh
#
# Picks the package format your system actually uses: .deb where dpkg exists,
# .rpm where rpm does, AppImage otherwise. Set BV_VERSION to install a specific
# release — needed for prereleases, which GitHub's "latest" endpoint skips:
#
#   BV_VERSION=v1.1.0-rc1 sh install.sh
#
# Nothing here reads stdin. That is deliberate: stdin is the script itself when
# piped from curl, so a prompt would eat the rest of this file.
set -eu

REPO=wannabeepolymath/git-branch-Visualizer

die() { printf 'install.sh: %s\n' "$1" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }
say() { printf '==> %s\n' "$1"; }

[ "$(uname -s)" = Linux ] ||
  die "this installer is Linux-only. On macOS: brew install wannabeepolymath/tap/branch-visualizer"

arch=$(uname -m)
[ "$arch" = x86_64 ] ||
  die "only x86_64 is published today (this machine is $arch). Build from source: https://github.com/$REPO"

have curl || die "curl is required"

if [ -n "${BV_VERSION:-}" ]; then
  api="https://api.github.com/repos/$REPO/releases/tags/$BV_VERSION"
else
  api="https://api.github.com/repos/$REPO/releases/latest"
fi

say "Looking up ${BV_VERSION:-latest} release"
json=$(curl -fsSL "$api") || die "could not reach the GitHub API (or ${BV_VERSION:-latest} does not exist)"

version=$(printf '%s' "$json" | grep -o '"tag_name": *"[^"]*"' | sed 's/.*"\([^"]*\)"$/\1/' | head -1)
[ -n "$version" ] || die "no release found"

# Match on the exact suffix: the release also carries .AppImage.sig and
# .AppImage.tar.gz, and a loose match would happily download the signature.
asset_url() {
  _all=$(printf '%s' "$json" |
    grep -o '"browser_download_url": *"[^"]*"' |
    sed 's/.*"\(https[^"]*\)"$/\1/' |
    grep -e "${1}\$" || true)
  [ -n "$_all" ] || return 0
  # More than one artifact for a format means several architectures are
  # published. Taking the first would silently install the wrong one, so narrow
  # to x86_64 — which Tauri spells "amd64" for deb/AppImage and "x86_64" for rpm.
  if [ "$(printf '%s\n' "$_all" | wc -l)" -gt 1 ]; then
    _all=$(printf '%s\n' "$_all" | grep -e amd64 -e x86_64 || true)
  fi
  printf '%s\n' "$_all" | head -1
}

# dpkg/rpm rather than apt/dnf: the package database is what decides whether a
# .deb or .rpm is the right artifact, not which frontend happens to be installed.
if have dpkg; then fmt=deb
elif have rpm; then fmt=rpm
else fmt=appimage
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

case "$fmt" in
  deb)
    url=$(asset_url '\.deb') || true
    [ -n "${url:-}" ] || die "no .deb in release $version"
    say "Downloading $version (.deb)"
    curl -fsSL "$url" -o "$tmp/bv.deb"
    say "Installing (sudo may prompt for your password)"
    # apt resolves dependencies; dpkg -i would leave them unmet.
    if have apt-get; then sudo apt-get install -y "$tmp/bv.deb"
    else sudo dpkg -i "$tmp/bv.deb"
    fi
    ;;
  rpm)
    url=$(asset_url '\.rpm') || true
    [ -n "${url:-}" ] || die "no .rpm in release $version"
    say "Downloading $version (.rpm)"
    curl -fsSL "$url" -o "$tmp/bv.rpm"
    say "Installing (sudo may prompt for your password)"
    if have dnf; then sudo dnf install -y "$tmp/bv.rpm"
    elif have zypper; then sudo zypper --non-interactive install "$tmp/bv.rpm"
    else sudo rpm -U "$tmp/bv.rpm"
    fi
    ;;
  appimage)
    url=$(asset_url '\.AppImage') || true
    [ -n "${url:-}" ] || die "no .AppImage in release $version"
    bindir=$HOME/.local/bin
    appdir=$HOME/.local/share/applications
    target=$bindir/branch-visualizer
    mkdir -p "$bindir" "$appdir"
    say "Downloading $version (AppImage)"
    curl -fsSL "$url" -o "$target"
    chmod +x "$target"

    # An AppImage ships no launcher entry, and on Linux the applications menu is
    # the app's primary entry point — it is a window, not a tray popover. Without
    # this the download would be effectively unlaunchable for most users.
    icon=""
    if (cd "$tmp" && "$target" --appimage-extract .DirIcon >/dev/null 2>&1) &&
       [ -f "$tmp/squashfs-root/.DirIcon" ]; then
      icon=$HOME/.local/share/icons/branch-visualizer.png
      mkdir -p "$(dirname "$icon")"
      cp "$tmp/squashfs-root/.DirIcon" "$icon"
    fi
    cat > "$appdir/branch-visualizer.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Branch Visualizer
Comment=Git branches, worktrees and commit graph
Exec=$target
${icon:+Icon=$icon}
Terminal=false
Categories=Development;RevisionControl;
EOF
    have update-desktop-database && update-desktop-database "$appdir" 2>/dev/null || true

    case ":$PATH:" in
      *":$bindir:"*) ;;
      *) say "Note: $bindir is not on your PATH" ;;
    esac
    ;;
esac

say "Installed Branch Visualizer $version"
say "Launch it from your applications menu, or run: branch-visualizer"
printf '\n%s\n' "Linux support is beta — built by CI, not tested on hardware."
printf '%s\n' "Issues: https://github.com/$REPO/issues"

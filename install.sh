#!/bin/sh
# SideKit installer for macOS and Linux — no Rust toolchain required.
#
#   curl -fsSL https://raw.githubusercontent.com/pratik-chakravorty/sidekit/master/install.sh | sh
#
# Options (flags, or the matching environment variables when piping into sh):
#   --version vX.Y.Z   SIDEKIT_VERSION   install a specific release (default: latest)
#   --prefix DIR       SIDEKIT_PREFIX    Linux: install under DIR (default: ~/.local)
#   --uninstall        SIDEKIT_UNINSTALL=1
#   --archive FILE     SIDEKIT_ARCHIVE   install from a local release archive (offline)
set -eu

REPO="pratik-chakravorty/sidekit"
VERSION="${SIDEKIT_VERSION:-latest}"
PREFIX="${SIDEKIT_PREFIX:-$HOME/.local}"
UNINSTALL="${SIDEKIT_UNINSTALL:-0}"
ARCHIVE="${SIDEKIT_ARCHIVE:-}"

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --prefix) PREFIX="$2"; shift 2 ;;
    --archive) ARCHIVE="$2"; shift 2 ;;
    --uninstall) UNINSTALL=1; shift ;;
    -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
  esac
done

say() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

OS="$(uname -s)"
case "$(uname -m)" in
  x86_64|amd64) ARCH=x86_64 ;;
  arm64|aarch64) ARCH=aarch64 ;;
  *) die "unsupported CPU architecture: $(uname -m)" ;;
esac

case "$OS" in
  Darwin) ASSET="sidekit-macos-universal.zip" ;;
  Linux) ASSET="sidekit-linux-$ARCH.tar.gz" ;;
  *) die "unsupported OS: $OS (on Windows use install.ps1)" ;;
esac

# Where things live.
if [ "$OS" = Darwin ]; then
  if [ -w /Applications ]; then APP_DIR=/Applications; else APP_DIR="$HOME/Applications"; fi
  APP="$APP_DIR/SideKit.app"
else
  BIN="$PREFIX/bin/sidekit"
  DESKTOP="${XDG_DATA_HOME:-$HOME/.local/share}/applications/sidekit.desktop"
  ICON="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/512x512/apps/sidekit.png"
fi

refresh_menus() {
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "$(dirname "$DESKTOP")" 2>/dev/null || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor" 2>/dev/null || true
  fi
}

uninstall() {
  if [ "$OS" = Darwin ]; then
    for dir in /Applications "$HOME/Applications"; do
      if [ -d "$dir/SideKit.app" ]; then
        rm -rf "$dir/SideKit.app"
        say "Removed $dir/SideKit.app"
      fi
    done
    if [ -L "$HOME/.local/bin/sidekit" ]; then rm -f "$HOME/.local/bin/sidekit"; fi
  else
    rm -f "$BIN" "$DESKTOP" "$ICON"
    refresh_menus
    say "Removed $BIN and its launcher entry"
  fi
  say "Your settings are kept in ${XDG_CONFIG_HOME:-$HOME/.config}/SideKit (delete it to reset)."
}

if [ "$UNINSTALL" = 1 ]; then
  uninstall
  exit 0
fi

fetch() { # url dest
  if command -v curl >/dev/null 2>&1; then
    curl -fL --progress-bar "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then
    wget -q --show-progress -O "$2" "$1"
  else
    die "curl or wget is required"
  fi
}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

if [ -n "$ARCHIVE" ]; then
  [ -f "$ARCHIVE" ] || die "archive not found: $ARCHIVE"
  cp "$ARCHIVE" "$TMP/$ASSET"
  say "Installing SideKit from $ARCHIVE"
else
  if [ "$VERSION" = latest ]; then
    BASE="https://github.com/$REPO/releases/latest/download"
  else
    case "$VERSION" in v*) ;; *) VERSION="v$VERSION" ;; esac
    BASE="https://github.com/$REPO/releases/download/$VERSION"
  fi
  say "Downloading SideKit ($VERSION) for $OS $ARCH"
  fetch "$BASE/$ASSET" "$TMP/$ASSET" || die "download failed: $BASE/$ASSET"

  # Verify against the release checksums when a hashing tool is available.
  if fetch "$BASE/SHA256SUMS" "$TMP/SHA256SUMS" 2>/dev/null; then
    expected="$(grep " $ASSET\$" "$TMP/SHA256SUMS" | cut -d' ' -f1)"
    if command -v sha256sum >/dev/null 2>&1; then actual="$(sha256sum "$TMP/$ASSET" | cut -d' ' -f1)"
    elif command -v shasum >/dev/null 2>&1; then actual="$(shasum -a 256 "$TMP/$ASSET" | cut -d' ' -f1)"
    else actual=""; fi
    if [ -n "$expected" ] && [ -n "$actual" ]; then
      [ "$expected" = "$actual" ] || die "checksum mismatch for $ASSET"
      say "Checksum verified"
    fi
  fi
fi

if [ "$OS" = Darwin ]; then
  (cd "$TMP" && ditto -x -k "$ASSET" .) || die "could not unpack $ASSET"
  [ -d "$TMP/SideKit.app" ] || die "SideKit.app missing from the archive"
  mkdir -p "$APP_DIR"
  pkill -x sidekit 2>/dev/null || true
  rm -rf "$APP"
  mv "$TMP/SideKit.app" "$APP"
  # The app is not notarized; drop the download quarantine so Gatekeeper lets it open.
  xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true
  mkdir -p "$HOME/.local/bin"
  ln -sf "$APP/Contents/MacOS/sidekit" "$HOME/.local/bin/sidekit"
  say "Installed $APP"
  say "Open it from Launchpad or Spotlight, or run: open -a SideKit"
else
  tar -xzf "$TMP/$ASSET" -C "$TMP" || die "could not unpack $ASSET"
  [ -f "$TMP/sidekit/sidekit" ] || die "sidekit binary missing from the archive"
  mkdir -p "$(dirname "$BIN")" "$(dirname "$DESKTOP")" "$(dirname "$ICON")"
  # Replace atomically so a running copy keeps working until it exits.
  cp "$TMP/sidekit/sidekit" "$BIN.new" && chmod 755 "$BIN.new" && mv -f "$BIN.new" "$BIN"
  cp "$TMP/sidekit/sidekit.png" "$ICON"
  sed "s|^Exec=.*|Exec=$BIN|" "$TMP/sidekit/sidekit.desktop" > "$DESKTOP"
  refresh_menus
  say "Installed $BIN"

  # Point out missing shared libraries (Vulkan, xkbcommon, fontconfig…).
  if command -v ldd >/dev/null 2>&1; then
    missing="$(ldd "$BIN" 2>/dev/null | awk '/not found/ {print $1}' | tr '\n' ' ')"
    if [ -n "$missing" ]; then warn "missing system libraries: $missing
  Debian/Ubuntu: sudo apt install libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 libfontconfig1 libvulkan1 mesa-vulkan-drivers
  Fedora:        sudo dnf install libxkbcommon libxkbcommon-x11 wayland fontconfig vulkan-loader mesa-vulkan-drivers
  Arch:          sudo pacman -S libxkbcommon libxkbcommon-x11 wayland fontconfig vulkan-icd-loader"
    fi
  fi
  case ":$PATH:" in
    *":$(dirname "$BIN"):"*) ;;
    *) warn "$(dirname "$BIN") is not on your PATH; add it to run 'sidekit' from a terminal." ;;
  esac
  say "Launch SideKit from your app menu, or run: sidekit"
fi

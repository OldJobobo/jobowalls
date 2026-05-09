#!/usr/bin/env bash
set -euo pipefail

SCRIPT_SOURCE="${0:-}"
if [[ -n "$SCRIPT_SOURCE" && "$SCRIPT_SOURCE" != "bash" && -f "$SCRIPT_SOURCE" ]]; then
  ROOT_DIR="$(cd "$(dirname "$SCRIPT_SOURCE")" && pwd)"
else
  ROOT_DIR="$(pwd)"
fi

REPO_URL="${REPO_URL:-https://github.com/OldJobobo/jobowalls.git}"
REPO_OWNER="${REPO_OWNER:-OldJobobo}"
REPO_NAME="${REPO_NAME:-jobowalls}"
INSTALL_VERSION="${INSTALL_VERSION:-latest}"
BUILD_FROM_SOURCE="${BUILD_FROM_SOURCE:-0}"
PREFIX="${PREFIX:-"$HOME/.local"}"
BINDIR="${BINDIR:-"$PREFIX/bin"}"
APPDIR="${APPDIR:-"$PREFIX/share/applications"}"
PROFILE="${PROFILE:-release}"
TARGET="${TARGET:-x86_64-unknown-linux-gnu}"
ARTIFACT_NAME="${ARTIFACT_NAME:-jobowalls-${TARGET}.tar.gz}"

usage() {
  cat <<EOF
Usage: ./install.sh

Or:
  curl -fsSL https://raw.githubusercontent.com/OldJobobo/jobowalls/refs/heads/master/install.sh | bash

Environment:
  PREFIX             Install prefix. Default: $HOME/.local
  BINDIR             Binary directory. Default: \$PREFIX/bin
  APPDIR             Desktop entry directory. Default: \$PREFIX/share/applications
  INSTALL_VERSION    Release tag to install, or latest. Default: latest
  BUILD_FROM_SOURCE  Build from source instead of downloading release binaries. Default: 0
  REPO_URL           Source clone URL for source builds.
                     Default: https://github.com/OldJobobo/jobowalls.git
  PROFILE            Cargo profile for source builds: release or debug. Default: release

Installs:
  \$BINDIR/jobowalls
  \$BINDIR/jobowalls-gui
  \$APPDIR/dev.jobowalls.picker.desktop
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

install_optional_backends() {
  if command -v omarchy >/dev/null 2>&1; then
    echo "installing live wallpaper backend with Omarchy"
    if ! omarchy pkg aur add mpvpaper; then
      echo "warning: failed to install mpvpaper through Omarchy; live wallpapers may not work" >&2
    fi
  else
    echo "omarchy not found; skipping automatic mpvpaper install"
  fi
}

latest_download_url() {
  if [[ "$INSTALL_VERSION" == "latest" ]]; then
    printf 'https://github.com/%s/%s/releases/latest/download/%s\n' \
      "$REPO_OWNER" "$REPO_NAME" "$ARTIFACT_NAME"
  else
    printf 'https://github.com/%s/%s/releases/download/%s/%s\n' \
      "$REPO_OWNER" "$REPO_NAME" "$INSTALL_VERSION" "$ARTIFACT_NAME"
  fi
}

install_from_release() {
  need curl
  need tar

  local url tmpdir archive
  url="$(latest_download_url)"
  tmpdir="$(mktemp -d)"
  archive="$tmpdir/$ARTIFACT_NAME"

  echo "downloading $url"
  if ! curl -fL "$url" -o "$archive"; then
    rm -rf "$tmpdir"
    return 1
  fi

  tar -xzf "$archive" -C "$tmpdir"

  install -d "$BINDIR" "$APPDIR"
  install -m 0755 "$tmpdir/bin/jobowalls" "$BINDIR/jobowalls"
  install -m 0755 "$tmpdir/bin/jobowalls-gui" "$BINDIR/jobowalls-gui"
  install -m 0644 "$tmpdir/share/applications/dev.jobowalls.picker.desktop" \
    "$APPDIR/dev.jobowalls.picker.desktop"

  rm -rf "$tmpdir"
}

ensure_checkout_for_source_build() {
  if [[ -f "$ROOT_DIR/Cargo.toml" && -f "$ROOT_DIR/gui/package.json" ]]; then
    return
  fi

  need git

  local tmpdir
  tmpdir="$(mktemp -d)"
  trap 'rm -rf "$tmpdir"' EXIT

  echo "install.sh is not running from a jobowalls checkout"
  echo "cloning $REPO_URL"
  git clone --depth 1 "$REPO_URL" "$tmpdir/jobowalls"

  echo "running source installer from cloned checkout"
  exec env \
    REPO_URL="$REPO_URL" \
    REPO_OWNER="$REPO_OWNER" \
    REPO_NAME="$REPO_NAME" \
    INSTALL_VERSION="$INSTALL_VERSION" \
    BUILD_FROM_SOURCE=1 \
    PREFIX="$PREFIX" \
    BINDIR="$BINDIR" \
    APPDIR="$APPDIR" \
    PROFILE="$PROFILE" \
    TARGET="$TARGET" \
    ARTIFACT_NAME="$ARTIFACT_NAME" \
    "$tmpdir/jobowalls/install.sh" "$@"
}

install_from_source() {
  ensure_checkout_for_source_build "$@"

  local cargo_args=()
  if [[ "$PROFILE" == "release" ]]; then
    cargo_args+=(--release)
  elif [[ "$PROFILE" != "debug" ]]; then
    echo "unsupported PROFILE: $PROFILE" >&2
    exit 1
  fi

  local cli_bin="$ROOT_DIR/target/$PROFILE/jobowalls"
  local gui_bin="$ROOT_DIR/gui/src-tauri/target/$PROFILE/jobowalls-gui"

  need cargo
  need npm

  echo "building jobowalls CLI ($PROFILE)"
  cargo build "${cargo_args[@]}" --manifest-path "$ROOT_DIR/Cargo.toml"

  echo "installing GUI dependencies"
  npm --prefix "$ROOT_DIR/gui" install

  echo "building GUI frontend"
  npm --prefix "$ROOT_DIR/gui" run build

  echo "building jobowalls GUI ($PROFILE)"
  cargo build "${cargo_args[@]}" --manifest-path "$ROOT_DIR/gui/src-tauri/Cargo.toml"

  install -d "$BINDIR" "$APPDIR"
  install -m 0755 "$cli_bin" "$BINDIR/jobowalls"
  install -m 0755 "$gui_bin" "$BINDIR/jobowalls-gui"
  install -m 0644 "$ROOT_DIR/packaging/linux/dev.jobowalls.picker.desktop" \
    "$APPDIR/dev.jobowalls.picker.desktop"
}

install_optional_backends

if [[ "$BUILD_FROM_SOURCE" == "1" ]]; then
  install_from_source "$@"
else
  if ! install_from_release; then
    echo "release binary install failed; falling back to source build"
    install_from_source "$@"
  fi
fi

echo "installed:"
echo "  $BINDIR/jobowalls"
echo "  $BINDIR/jobowalls-gui"
echo "  $APPDIR/dev.jobowalls.picker.desktop"

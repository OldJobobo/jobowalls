#!/usr/bin/env bash
set -euo pipefail

SCRIPT_SOURCE="${0:-}"
if [[ -n "$SCRIPT_SOURCE" && "$SCRIPT_SOURCE" != "bash" && -f "$SCRIPT_SOURCE" ]]; then
  ROOT_DIR="$(cd "$(dirname "$SCRIPT_SOURCE")" && pwd)"
else
  ROOT_DIR="$(pwd)"
fi
REPO_URL="${REPO_URL:-https://github.com/OldJobobo/jobowalls.git}"
PREFIX="${PREFIX:-"$HOME/.local"}"
BINDIR="${BINDIR:-"$PREFIX/bin"}"
APPDIR="${APPDIR:-"$PREFIX/share/applications"}"
PROFILE="${PROFILE:-release}"

CLI_BIN="$ROOT_DIR/target/$PROFILE/jobowalls"
GUI_BIN="$ROOT_DIR/gui/src-tauri/target/$PROFILE/jobowalls-gui"

usage() {
  cat <<EOF
Usage: ./install.sh

Or:
  curl -fsSL https://raw.githubusercontent.com/OldJobobo/jobowalls/refs/heads/master/install.sh | bash

Environment:
  REPO_URL Install source when run outside a checkout.
           Default: https://github.com/OldJobobo/jobowalls.git
  PREFIX   Install prefix. Default: $HOME/.local
  BINDIR   Binary directory. Default: \$PREFIX/bin
  APPDIR   Desktop entry directory. Default: \$PREFIX/share/applications
  PROFILE  Cargo profile: release or debug. Default: release

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

run_from_checkout_or_clone() {
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

  echo "running installer from cloned checkout"
  exec env \
    REPO_URL="$REPO_URL" \
    PREFIX="$PREFIX" \
    BINDIR="$BINDIR" \
    APPDIR="$APPDIR" \
    PROFILE="$PROFILE" \
    "$tmpdir/jobowalls/install.sh" "$@"
}

run_from_checkout_or_clone "$@"

cargo_args=()
if [[ "$PROFILE" == "release" ]]; then
  cargo_args+=(--release)
elif [[ "$PROFILE" != "debug" ]]; then
  echo "unsupported PROFILE: $PROFILE" >&2
  exit 1
fi

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

install -d "$BINDIR"
install -m 0755 "$CLI_BIN" "$BINDIR/jobowalls"
install -m 0755 "$GUI_BIN" "$BINDIR/jobowalls-gui"

install -d "$APPDIR"
install -m 0644 "$ROOT_DIR/packaging/linux/dev.jobowalls.picker.desktop" \
  "$APPDIR/dev.jobowalls.picker.desktop"

echo "installed:"
echo "  $BINDIR/jobowalls"
echo "  $BINDIR/jobowalls-gui"
echo "  $APPDIR/dev.jobowalls.picker.desktop"

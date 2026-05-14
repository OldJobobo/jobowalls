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
CONFIGDIR="${CONFIGDIR:-"$HOME/.config/jobowalls"}"
INSTALL_CONFIG="${INSTALL_CONFIG:-1}"
PROFILE="${PROFILE:-release}"
TARGET="${TARGET:-x86_64-unknown-linux-gnu}"
ARTIFACT_NAME="${ARTIFACT_NAME:-jobowalls-${TARGET}.tar.gz}"
GUI_WRAPPER_SOURCE="$ROOT_DIR/packaging/linux/jobowalls-gui"

usage() {
  cat <<EOF
Usage: ./install.sh

Or:
  curl -fsSL https://raw.githubusercontent.com/OldJobobo/jobowalls/refs/heads/master/install.sh | bash

Environment:
  PREFIX             Install prefix. Default: $HOME/.local
  BINDIR             Binary directory. Default: \$PREFIX/bin
  APPDIR             Desktop entry directory. Default: \$PREFIX/share/applications
  CONFIGDIR          Config directory. Default: \$HOME/.config/jobowalls
  INSTALL_CONFIG     Create default config when missing. Default: 1
  INSTALL_VERSION    Release tag to install, or latest. Default: latest
  BUILD_FROM_SOURCE  Build from source instead of downloading release binaries. Default: 0
  REPO_URL           Source clone URL for source builds.
                     Default: https://github.com/OldJobobo/jobowalls.git
  PROFILE            Cargo profile for source builds: release or debug. Default: release

Installs:
  \$BINDIR/jobowalls
  \$BINDIR/jobowalls-gui
  \$BINDIR/jobowalls-shell
  \$APPDIR/dev.jobowalls.picker.desktop
  \$CONFIGDIR/config.toml
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

explain_permission_failure() {
  local path="$1"
  cat >&2 <<EOF
permission denied while installing: $path

jobowalls installs to user-writable paths by default:
  BINDIR=$BINDIR
  APPDIR=$APPDIR

If an earlier install created root-owned files, remove or chown these paths:
  $BINDIR/jobowalls
  $BINDIR/jobowalls-gui
  $BINDIR/jobowalls-gui-bin
  $BINDIR/jobowalls-shell
  $APPDIR/dev.jobowalls.picker.desktop

You can also install somewhere else with:
  PREFIX=/some/writable/path ./install.sh
EOF
}

install_or_permission_error() {
  local destination="${!#}"
  if ! install "$@"; then
    explain_permission_failure "$destination"
    return 2
  fi
}

prepare_install_dirs() {
  if ! install -d "$BINDIR" "$APPDIR" "$CONFIGDIR"; then
    explain_permission_failure "$BINDIR, $APPDIR, or $CONFIGDIR"
    return 2
  fi
}

install_default_config() {
  if [[ "$INSTALL_CONFIG" == "0" ]]; then
    return
  fi

  local config="$CONFIGDIR/config.toml"
  if [[ -e "$config" ]]; then
    echo "config exists, leaving unchanged: $config"
    return
  fi

  if ! install -d "$CONFIGDIR"; then
    explain_permission_failure "$CONFIGDIR"
    return 2
  fi

  if ! cat >"$config" <<'EOF'
[general]
static_backend = "auto"
live_backend = "mpvpaper"
restore_on_startup = true

[monitors]
default = "all"

[live.pause]
on_battery = true
on_fullscreen = true
on_idle = true
resume_on_ac = true
resume_on_unfullscreen = true
resume_on_activity = true

[mpvpaper]
mode = "per-monitor"
extra_args = ["--loop", "--no-audio", "--panscan=1.0"]
readiness_timeout_ms = 5000

[awww]
enabled = false
transition_type = "grow"
transition_duration = 2.4
transition_fps = 60
transition_pos = "center"
transition_bezier = ".42,0,.2,1"
transition_wave = "28,12"

[gui]
default_monitor = "all"
preview_quality = "balanced"
remember_last_folder = true
use_omarchy_theme = true
window_width = 1040
window_height = 620
live_preview = true

[gui.theme_collections]
enabled = false
include_stock_themes = false
user_themes_dir = "~/.config/omarchy/themes"
stock_themes_dir = "~/.local/share/omarchy/themes"
user_backgrounds_dir = "~/.config/omarchy/backgrounds"
add_target = "user-backgrounds"

[gui.theme_collections.author]
enabled = false
theme_roots = ["~/Projects/themes"]

[shell]
monitor = "all"
position = "bottom"
layout = "horizontal"
height = 340
live_preview = true
EOF
  then
    explain_permission_failure "$config"
    return 2
  fi

  chmod 0644 "$config"
}

path_contains_dir() {
  local dir="$1"
  case ":${PATH:-}:" in
    *":$dir:"*) return 0 ;;
    *) return 1 ;;
  esac
}

append_profile_path() {
  local profile="$HOME/.profile"
  local marker="# jobowalls: add user-local binaries to PATH"

  if [[ -f "$profile" ]] && grep -F "$marker" "$profile" >/dev/null 2>&1; then
    return
  fi

  cat >>"$profile" <<EOF

$marker
case ":\$PATH:" in
  *":\$HOME/.local/bin:"*) ;;
  *) export PATH="\$HOME/.local/bin:\$PATH" ;;
esac
EOF
}

install_fish_path_config() {
  local fish_dir="$HOME/.config/fish/conf.d"
  local fish_config="$fish_dir/jobowalls-path.fish"

  install -d "$fish_dir"
  cat >"$fish_config" <<'EOF'
if test -d "$HOME/.local/bin"
    fish_add_path -U "$HOME/.local/bin"
end
EOF

  if command -v fish >/dev/null 2>&1; then
    # shellcheck disable=SC2016
    fish -c 'fish_add_path -U -- $argv[1]' -- "$HOME/.local/bin" >/dev/null 2>&1 || true
  fi
}

ensure_user_path() {
  local default_bindir="$HOME/.local/bin"

  if [[ "$BINDIR" != "$default_bindir" ]]; then
    if ! path_contains_dir "$BINDIR"; then
      echo "warning: $BINDIR is not in PATH; add it before running jobowalls by name" >&2
    fi
    return
  fi

  if path_contains_dir "$default_bindir"; then
    return
  fi

  local updated_profile=0
  if append_profile_path; then
    updated_profile=1
  else
    echo "warning: failed to update ~/.profile; add $default_bindir to PATH manually" >&2
  fi

  if [[ "${SHELL:-}" == */fish ]] || [[ -d "$HOME/.config/fish" ]] || command -v fish >/dev/null 2>&1; then
    if install_fish_path_config; then
      echo "added $default_bindir to fish PATH via ~/.config/fish/conf.d/jobowalls-path.fish"
    else
      echo "warning: failed to update fish PATH; add $default_bindir with fish_add_path" >&2
    fi
  fi

  if [[ "$updated_profile" -eq 1 ]]; then
    echo "added $default_bindir to PATH via ~/.profile"
  fi
  if [[ "${SHELL:-}" == */fish ]]; then
    echo "restart your shell, or run: fish_add_path -U $default_bindir"
  else
    echo "restart your shell, or run: export PATH=\"$default_bindir:\$PATH\""
  fi
}

install_optional_backends() {
  if command -v mpvpaper >/dev/null 2>&1; then
    return
  fi

  local install_command="omarchy pkg aur add mpvpaper"
  echo "mpvpaper is required for live/video wallpapers."

  if ! command -v omarchy >/dev/null 2>&1; then
    echo "omarchy not found; skipping mpvpaper install."
    echo "Install mpvpaper after setup to use live wallpapers."
    echo "Omarchy command: $install_command"
    return
  fi

  if [[ ! -t 0 ]]; then
    echo "Skipping mpvpaper install because stdin is not interactive."
    echo "Install mpvpaper after setup to use live wallpapers."
    echo "Omarchy command: $install_command"
    return
  fi

  echo "JoboWalls can install it the Omarchy way:"
  echo "  $install_command"
  if ! read -r -p "Install mpvpaper now? [y/N] " answer; then
    answer=""
  fi
  case "$answer" in
    y|Y|yes|YES)
      if ! omarchy pkg aur add mpvpaper; then
        echo "warning: failed to install mpvpaper through Omarchy; live wallpapers may not work" >&2
      fi
      ;;
    *)
      echo "Skipping mpvpaper install. Live wallpapers need mpvpaper."
      echo "Run later: $install_command"
      ;;
  esac
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

  prepare_install_dirs || {
    rm -rf "$tmpdir"
    return 2
  }

  echo "downloading $url"
  if ! curl -fL "$url" -o "$archive"; then
    rm -rf "$tmpdir"
    return 1
  fi

  if ! tar -xzf "$archive" -C "$tmpdir"; then
    rm -rf "$tmpdir"
    return 1
  fi

  install_or_permission_error -m 0755 "$tmpdir/bin/jobowalls" "$BINDIR/jobowalls" || {
    rm -rf "$tmpdir"
    return 2
  }
  if [[ -x "$tmpdir/bin/jobowalls-gui-bin" ]]; then
    install_or_permission_error -m 0755 "$tmpdir/bin/jobowalls-gui-bin" "$BINDIR/jobowalls-gui-bin" || {
      rm -rf "$tmpdir"
      return 2
    }
    install_or_permission_error -m 0755 "$tmpdir/bin/jobowalls-gui" "$BINDIR/jobowalls-gui" || {
      rm -rf "$tmpdir"
      return 2
    }
  else
    install_or_permission_error -m 0755 "$tmpdir/bin/jobowalls-gui" "$BINDIR/jobowalls-gui-bin" || {
      rm -rf "$tmpdir"
      return 2
    }
    install_gui_wrapper "$BINDIR/jobowalls-gui" || {
      rm -rf "$tmpdir"
      return 2
    }
  fi
  if [[ -x "$tmpdir/bin/jobowalls-shell" ]]; then
    install_or_permission_error -m 0755 "$tmpdir/bin/jobowalls-shell" "$BINDIR/jobowalls-shell" || {
      rm -rf "$tmpdir"
      return 2
    }
  fi
  install_desktop_entry "$tmpdir/share/applications/dev.jobowalls.picker.desktop" \
    "$APPDIR/dev.jobowalls.picker.desktop" || {
    rm -rf "$tmpdir"
    return 2
  }

  rm -rf "$tmpdir"
}

install_gui_wrapper() {
  local destination="$1"
  if [[ -f "$GUI_WRAPPER_SOURCE" ]]; then
    install_or_permission_error -m 0755 "$GUI_WRAPPER_SOURCE" "$destination"
    return
  fi

  if ! cat >"$destination" <<'EOF'
#!/usr/bin/env bash
set -uo pipefail

bin="${JOBOWALLS_GUI_BIN:-$(dirname "$0")/jobowalls-gui-bin}"

if [[ ! -x "$bin" ]]; then
  echo "jobowalls-gui binary not found: $bin" >&2
  exit 127
fi

export WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}"
export WEBKIT_DISABLE_COMPOSITING_MODE="${WEBKIT_DISABLE_COMPOSITING_MODE:-1}"

if [[ -n "${GDK_BACKEND:-}" ]]; then
  exec "$bin" "$@"
fi

if [[ -n "${WAYLAND_DISPLAY:-}" ]]; then
  GDK_BACKEND=wayland "$bin" "$@"
  status=$?
  if [[ "$status" -ne 0 && -n "${DISPLAY:-}" ]]; then
    echo "jobowalls-gui: Wayland launch failed with status $status; retrying with GDK_BACKEND=x11" >&2
    GDK_BACKEND=x11 "$bin" "$@"
    exit $?
  fi
  exit "$status"
fi

GDK_BACKEND=x11 "$bin" "$@"
status=$?

if [[ "$status" -ne 0 ]]; then
  echo "jobowalls-gui: X11 launch failed with status $status" >&2
  exit "$status"
fi

exit "$status"
EOF
  then
    explain_permission_failure "$destination"
    return 2
  fi
  if ! chmod 0755 "$destination"; then
    explain_permission_failure "$destination"
    return 2
  fi
}

install_desktop_entry() {
  local source="$1"
  local destination="$2"
  local tmpfile
  tmpfile="$(mktemp)" || return 1

  if ! {
    while IFS= read -r line; do
      if [[ "$line" == Exec=* ]]; then
        printf 'Exec=%s/jobowalls-gui\n' "$BINDIR"
      else
        printf '%s\n' "$line"
      fi
    done <"$source" >"$tmpfile"
  }; then
    rm -f "$tmpfile"
    return 1
  fi

  if ! install_or_permission_error -m 0644 "$tmpfile" "$destination"; then
    rm -f "$tmpfile"
    return 2
  fi
  rm -f "$tmpfile"
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
    CONFIGDIR="$CONFIGDIR" \
    INSTALL_CONFIG="$INSTALL_CONFIG" \
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
  local shell_bin="$ROOT_DIR/target/$PROFILE/jobowalls-shell"
  local gui_bin="$ROOT_DIR/gui/src-tauri/target/$PROFILE/jobowalls-gui"

  need cargo
  need npm

  echo "building jobowalls CLI ($PROFILE)"
  cargo build "${cargo_args[@]}" --manifest-path "$ROOT_DIR/Cargo.toml"

  echo "installing GUI dependencies"
  npm --prefix "$ROOT_DIR/gui" install

  echo "building jobowalls GUI ($PROFILE)"
  if [[ "$PROFILE" == "release" ]]; then
    npm --prefix "$ROOT_DIR/gui" run tauri:build -- --no-bundle
  else
    npm --prefix "$ROOT_DIR/gui" run tauri:build -- --debug --no-bundle
  fi

  prepare_install_dirs
  install_or_permission_error -m 0755 "$cli_bin" "$BINDIR/jobowalls"
  if [[ -x "$shell_bin" ]]; then
    install_or_permission_error -m 0755 "$shell_bin" "$BINDIR/jobowalls-shell"
  fi
  install_or_permission_error -m 0755 "$gui_bin" "$BINDIR/jobowalls-gui-bin"
  install_gui_wrapper "$BINDIR/jobowalls-gui"
  install_desktop_entry "$ROOT_DIR/packaging/linux/dev.jobowalls.picker.desktop" \
    "$APPDIR/dev.jobowalls.picker.desktop"
}

if [[ "$BUILD_FROM_SOURCE" == "1" ]]; then
  install_from_source "$@"
else
  set +e
  install_from_release
  status=$?
  set -e
  if [[ "$status" -eq 1 ]]; then
    echo "release binary download or unpack failed; falling back to source build"
    install_from_source "$@"
  elif [[ "$status" -ne 0 ]]; then
    exit "$status"
  fi
fi

install_optional_backends
ensure_user_path
install_default_config

echo "installed:"
echo "  $BINDIR/jobowalls"
echo "  $BINDIR/jobowalls-gui"
if [[ -x "$BINDIR/jobowalls-shell" ]]; then
  echo "  $BINDIR/jobowalls-shell"
fi
echo "  $APPDIR/dev.jobowalls.picker.desktop"
if [[ "$INSTALL_CONFIG" != "0" ]]; then
  echo "  $CONFIGDIR/config.toml"
fi

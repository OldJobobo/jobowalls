# jobowalls

`jobowalls` is a Hyprland wallpaper tool that uses one command model for both
static and live wallpapers.

It chooses the right backend underneath:

- `swaybg` for static images
- `mpvpaper` for live/video wallpapers
- optional `awww` support for static image transitions

The repo also includes `jobowalls-gui`, a minimal Tauri film-roll picker, and
`jobowalls-shell`, a compact GTK layer-shell picker for keyboard-driven desktop
wallpaper switching. Both frontends apply wallpapers through the `jobowalls`
CLI.

## Status

This is early software. The CLI is usable, the GUI picker is functional, and
the package/install layout is being prepared for later AUR packaging.

## Features

- Static and live wallpaper support through one CLI.
- Automatic media type detection.
- Backend selection with optional overrides.
- Multi-monitor targeting.
- State file tracking for current wallpaper, backend, monitor mapping, and
  owned live wallpaper PIDs.
- Safe live wallpaper stop behavior based on recorded ownership.
- Collection navigation with `next`, `previous`, and `shuffle`.
- Restore support for the last recorded state.
- Visual GUI picker with a film-roll strip and large preview.
- Compact shell-layer picker with keyboard, mouse, and wheel navigation.
- Live wallpaper preview animations in the GUI using cached generated previews.

## Requirements

Normal install requirements:

- `curl`
- `tar`

Source build requirements, only needed when `BUILD_FROM_SOURCE=1` or when no
release binary is available:

- `git`
- Rust/Cargo
- Node.js and npm, for the GUI

Runtime backends:

- `swaybg` for Omarchy/default static wallpapers
- `mpvpaper` for live wallpapers
- `awww` and `awww-daemon` if you want optional static transitions

GUI preview helpers:

- `ffmpeg` for animated live wallpaper previews
- `ffmpegthumbnailer` for faster live wallpaper poster generation

On Arch-like systems, the GUI also needs the normal Tauri/WebKitGTK runtime
stack, including GTK and WebKitGTK.

## Install

One-command install to `~/.local`:

```bash
curl -fsSL https://raw.githubusercontent.com/OldJobobo/jobowalls/refs/heads/master/install.sh | bash
```

The installer downloads the latest precompiled Linux x86_64 release by default.
If no release binary is available, it falls back to building from source.
It also creates `~/.config/jobowalls/config.toml` with default settings when the
file does not already exist.

Or clone and run the installer from a checkout:

```bash
git clone https://github.com/OldJobobo/jobowalls.git
cd jobowalls
./install.sh
```

This installs:

```text
~/.local/bin/jobowalls
~/.local/bin/jobowalls-shell
~/.local/bin/jobowalls-gui
~/.local/share/applications/dev.jobowalls.picker.desktop
~/.config/jobowalls/config.toml
```

When installing to the default prefix, the installer adds `~/.local/bin` to
your user `PATH` when it is missing. Fish users get a small
`~/.config/fish/conf.d/jobowalls-path.fish` snippet; other shells get a
`~/.profile` update. Restart your shell after install if `jobowalls` is not
found immediately.

To install somewhere else:

```bash
curl -fsSL https://raw.githubusercontent.com/OldJobobo/jobowalls/refs/heads/master/install.sh | PREFIX=/usr/local bash
```

To force a source build:

```bash
curl -fsSL https://raw.githubusercontent.com/OldJobobo/jobowalls/refs/heads/master/install.sh | BUILD_FROM_SOURCE=1 bash
```

Or from a checkout:

```bash
BUILD_FROM_SOURCE=1 ./install.sh
```

To install to a custom prefix from a checkout:

```bash
PREFIX=/usr/local ./install.sh
```

Uninstall:

```bash
curl -fsSL https://raw.githubusercontent.com/OldJobobo/jobowalls/refs/heads/master/uninstall.sh | bash
```

Or from a checkout:

```bash
./uninstall.sh
```

Or, for a custom prefix:

```bash
curl -fsSL https://raw.githubusercontent.com/OldJobobo/jobowalls/refs/heads/master/uninstall.sh | PREFIX=/usr/local bash
```

Or from a checkout:

```bash
PREFIX=/usr/local ./uninstall.sh
```

## CLI Usage

Set a static wallpaper:

```bash
jobowalls set ~/Pictures/wallpapers/sakura.png
```

Set a live wallpaper:

```bash
jobowalls set ~/Videos/wallpapers/rain.mp4
```

Target all monitors:

```bash
jobowalls set ~/Videos/wallpapers/rain.mp4 --monitor all
```

Target one monitor:

```bash
jobowalls set ~/Pictures/wallpapers/main.png --monitor DP-1
```

Preview the plan without applying:

```bash
jobowalls set ~/Pictures/wallpapers/main.png --dry-run
```

Force a static backend:

```bash
jobowalls set ~/Pictures/wallpapers/main.png --backend swaybg
```

Machine-readable dry run:

```bash
jobowalls set ~/Pictures/wallpapers/main.png --dry-run --json
```

Check current state:

```bash
jobowalls status
jobowalls status --json
```

Stop owned live wallpaper processes:

```bash
jobowalls stop-live
```

Restore the last recorded wallpaper state:

```bash
jobowalls restore
```

Navigate a collection:

```bash
jobowalls next ~/Pictures/wallpapers
jobowalls previous ~/Pictures/wallpapers
jobowalls shuffle ~/Pictures/wallpapers
```

Run diagnostics:

```bash
jobowalls doctor
```

Print the default config:

```bash
jobowalls config print-default
```

Create a config file:

```bash
jobowalls config init
```

The installer creates `~/.config/jobowalls/config.toml` by default. Existing
config files are left untouched. Command-line flags override config values, and
config values override built-in defaults. Runtime state is kept separately under
`~/.local/state/jobowalls/`.

GUI and shell settings are part of the same config:

```toml
[gui]
default_monitor = "all"
preview_quality = "balanced"
remember_last_folder = true
use_omarchy_theme = true
window_width = 1040
window_height = 620
live_preview = true

[shell]
monitor = "all"
position = "bottom"
height = 340
live_preview = true
```

## GUI Picker

Launch the picker:

```bash
jobowalls-gui
```

Launch with a specific folder:

```bash
jobowalls-gui ~/Pictures/wallpapers
```

The picker resolves its startup folder in this order:

1. Folder argument passed to `jobowalls-gui`.
2. Last folder saved by the GUI, when `[gui].remember_last_folder` is enabled.
3. `~/.config/omarchy/current/theme/backgrounds`.
4. `~/Pictures/Wallpapers`.

The GUI uses `[gui].default_monitor`, `[gui].preview_quality`,
`[gui].use_omarchy_theme`, `[gui].window_width`, `[gui].window_height`, and
`[gui].live_preview` as startup defaults.

Keyboard controls:

```text
Left / H       previous wallpaper
Right / L      next wallpaper
Enter          apply selected wallpaper
S              shuffle selection
O              open/change folder
R              rescan current folder
Escape         close picker
```

The GUI does not manage wallpaper backends directly. It calls `jobowalls set`
underneath, so backend choice and process ownership stay in the CLI.

## Shell Picker

Launch the compact shell-layer picker:

```bash
jobowalls-shell
```

Launch with a specific folder:

```bash
jobowalls-shell ~/Pictures/wallpapers
```

Useful options:

```bash
jobowalls-shell --monitor all ~/Pictures/wallpapers
jobowalls-shell --no-live-preview ~/Pictures/wallpapers
jobowalls-shell --debug-window ~/Pictures/wallpapers
```

The shell uses `[shell].monitor`, `[shell].position`, `[shell].height`, and
`[shell].live_preview` as defaults. Flags such as `--monitor`,
`--position`, `--height`, and `--no-live-preview` override config values for
that launch.

Keyboard controls:

```text
Left / H       previous wallpaper
Right / L      next wallpaper
Enter          apply selected wallpaper and close
S              shuffle selection
R              rescan current folder
Escape         restore original preview and close
```

Mouse wheel or trackpad scrolling moves the carousel. Clicking the left or
right side moves selection, and double-clicking applies the selected wallpaper.

## Supported Wallpaper Types

Static:

```text
jpg
jpeg
png
webp
bmp
gif
```

Live/video:

```text
mp4
webm
mkv
mov
avi
```

## Config And State

Config:

```text
~/.config/jobowalls/config.toml
```

State:

```text
~/.local/state/jobowalls/state.json
```

GUI state:

```text
~/.local/state/jobowalls/gui.json
```

Shell state:

```text
~/.local/state/jobowalls/shell.json
```

GUI preview cache:

```text
~/.cache/jobowalls/gui-thumbnails/
```

## Development

Development, validation, and packaging notes live in
[DEVELOPMENT.md](DEVELOPMENT.md).

## Notes

`jobowalls` is designed to avoid editing Hyprland or Omarchy config for normal
wallpaper changes. If you want a keybind for a picker, add one in your own
Hyprland config, for example:

```text
bind = SUPER, W, exec, jobowalls-gui
bind = SUPER CTRL ALT, SPACE, exec, jobowalls-shell
```

The tool only stops live wallpaper processes that it recorded as owned in its
state file.

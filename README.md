# jobowalls

`jobowalls` is a Hyprland wallpaper tool that uses one command model for both
static and live wallpapers.

It chooses the right backend underneath:

- `hyprpaper` for static images
- `mpvpaper` for live/video wallpapers
- optional `awww` support for static image transitions

The repo also includes `jobowalls-gui`, a minimal Tauri film-roll picker that
lets you visually browse a folder, preview wallpapers, and apply the selected
wallpaper through the `jobowalls` CLI.

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
- Live wallpaper preview animations in the GUI using cached generated previews.

## Requirements

Core build requirements:

- Rust/Cargo
- Node.js and npm, for the GUI

Runtime backends:

- `hyprpaper` for static wallpapers
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
curl -fsSL https://raw.githubusercontent.com/OldJobobo/jobowalls/master/install.sh | bash
```

Or clone and install from a checkout:

```bash
git clone https://github.com/OldJobobo/jobowalls.git
cd jobowalls
./install.sh
```

This installs:

```text
~/.local/bin/jobowalls
~/.local/bin/jobowalls-gui
~/.local/share/applications/dev.jobowalls.picker.desktop
```

Make sure `~/.local/bin` is in your `PATH`.

To install somewhere else:

```bash
curl -fsSL https://raw.githubusercontent.com/OldJobobo/jobowalls/master/install.sh | PREFIX=/usr/local bash
```

Or from a checkout:

```bash
PREFIX=/usr/local ./install.sh
```

Uninstall:

```bash
./uninstall.sh
```

Or, for a custom prefix:

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
2. Last folder saved by the GUI.
3. `~/.config/omarchy/current/theme/backgrounds`.
4. `~/Pictures/Wallpapers`.

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

GUI preview cache:

```text
~/.cache/jobowalls/gui-thumbnails/
```

## Development

Run CLI tests:

```bash
cargo test
```

Run the CLI from source:

```bash
cargo run -- set /path/to/wallpaper.png --dry-run
```

Run the GUI in development:

```bash
cd gui
npm install
npm run tauri:dev
```

Build the GUI frontend:

```bash
cd gui
npm run build
```

Check the Tauri backend:

```bash
cd gui/src-tauri
cargo check
```

## Packaging

The repository includes packaging starters:

```text
packaging/linux/dev.jobowalls.picker.desktop
packaging/arch/PKGBUILD
packaging/arch/README.md
```

The Arch packaging files are a starting point for a future AUR package. They
are not a submitted AUR package yet.

## Notes

`jobowalls` is designed to avoid editing Hyprland or Omarchy config for normal
wallpaper changes. If you want a keybind for the GUI picker, add one in your own
Hyprland config, for example:

```text
bind = SUPER, W, exec, jobowalls-gui
```

The tool only stops live wallpaper processes that it recorded as owned in its
state file.

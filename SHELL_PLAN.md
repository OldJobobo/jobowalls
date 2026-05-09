# jobowalls-shell Plan

## Goal

Build `jobowalls-shell`: a minimal native Rust/GTK shell-layer wallpaper picker
for `jobowalls`.

The shell picker should feel like a desktop-native overlay rather than a normal
application window. It exists for one fast workflow: summon a compact wallpaper
carousel, move left or right, apply, and disappear.

`PLAN.md` remains authoritative for backend behavior, state ownership, config,
and process management. `GUI_PLAN.md` remains authoritative for the full visual
Tauri picker. This plan covers only the lightweight shell-layer frontend.

## Product Shape

Public commands:

```bash
jobowalls          # main CLI
jobowalls-gui      # full Tauri picker
jobowalls-shell    # compact shell-layer picker
```

Expected flow:

```bash
jobowalls-shell
jobowalls-shell /path/to/backgrounds
jobowalls-shell --monitor DP-1 /path/to/backgrounds
```

Behavior:

- Open as a Wayland layer-shell overlay.
- Anchor near the bottom center of the current monitor.
- Show only a compact wallpaper carousel.
- Keep the selected wallpaper visually dominant.
- Show at most a few neighboring wallpapers.
- Apply with `Enter`.
- Close with `Escape`.
- Use existing `jobowalls set` behavior for actual wallpaper changes.
- Never manage wallpaper backend processes directly from the shell UI.

## Non-Goals For MVP

- No full-screen preview.
- No Tauri/WebView dependency.
- No dashboard layout.
- No config editor.
- No permanent sidebar.
- No playlist editor.
- No tag, rating, or metadata system.
- No direct `swaybg`, `hyprpaper`, `mpvpaper`, or `awww` process control.
- No Hyprland config edits during normal app execution.
- No requirement that users install AGS.

Future collection, playlist, and automation features should be implemented in
the CLI/core first, then surfaced here only as compact picker actions.

## Recommended Stack

Use a native GTK4 Rust binary:

```text
Rust
GTK4 via gtk4-rs
Relm4 for component/message architecture
gtk4-layer-shell for Wayland layer-shell behavior
CSS for visual styling
```

Suggested dependencies:

```toml
[dependencies]
gtk = { package = "gtk4", version = "0.11" }
relm4 = "0.10"
gtk4-layer-shell = "0.8"
gio = "0.22"
glib = "0.22"
```

Avoid the older `gtk-layer-shell` crate. It targets GTK3 and is no longer the
right dependency for a new GTK4 shell surface.

Do not use AGS for the shipped implementation. AGS is good for local shell
customization, but it would add a runtime dependency and make distribution less
clean.

## Repository Shape

Add a new binary target inside the existing main Cargo package rather than
creating a workspace member:

```text
src/bin/jobowalls-shell.rs
src/shell/
  mod.rs
  app.rs
  cli.rs
  model.rs
  scanner.rs
  preview.rs
  apply.rs
  layer.rs
  widgets/
    carousel.rs
    thumbnail.rs
    empty.rs
  style.css
```

Keep reusable wallpaper concepts in the existing library where practical:

```text
src/media.rs          # media classification
src/orchestrator.rs   # set planning
src/config.rs         # config/default folder support
src/state.rs          # active wallpaper state
```

The shell code should not duplicate backend selection or process ownership
logic. It should ask the existing core/CLI path to apply wallpapers.

## Architecture

High-level flow:

```text
jobowalls-shell
  parse shell-specific flags
  resolve wallpaper folder
  scan supported media
  load current jobowalls state
  build GTK layer-shell window
  render compact carousel
  handle keyboard/mouse input
  apply selected wallpaper through core command path
  close on success
```

Suggested module responsibilities:

```text
shell::cli
  Parses --monitor, --folder, --position, --preview-quality, and debug flags.

shell::app
  Owns Relm4 application model, messages, update loop, and startup flow.

shell::scanner
  Resolves folder and returns sorted WallpaperItem values.

shell::preview
  Generates and caches thumbnail paths for GTK image widgets.

shell::apply
  Calls the existing jobowalls apply path or runs the same internal set logic.

shell::layer
  Configures gtk4-layer-shell anchors, margins, keyboard mode, namespace, and
  fallback behavior.

shell::widgets::carousel
  Renders selected, previous, and next items with stable sizing and transforms.

shell::widgets::thumbnail
  Renders individual static/live preview thumbnails.
```

## Invocation Model

MVP:

```bash
jobowalls-shell [folder]
jobowalls-shell --monitor all [folder]
jobowalls-shell --monitor DP-1 [folder]
```

Near-term flags:

```bash
jobowalls-shell --position bottom
jobowalls-shell --position center
jobowalls-shell --width 640
jobowalls-shell --height 170
jobowalls-shell --no-live-preview
jobowalls-shell --debug-window
```

Debug mode should intentionally open as a normal decorated GTK window. That
makes layout and event debugging easier outside a layer-shell context.

## Default Folder Resolution

Use the same folder resolution strategy as the Tauri picker:

1. Positional path passed to `jobowalls-shell`.
2. Saved last GUI/shell folder from JoboWalls state.
3. Current theme backgrounds directory:

   ```text
   ~/.config/omarchy/current/theme/backgrounds
   ```

4. Fallback wallpaper directory:

   ```text
   ~/Pictures/Wallpapers
   ```

If no valid folder exists, show a compact empty overlay with a short message and
close on `Escape`.

## Window And Layer Behavior

Target behavior:

```text
Layer: overlay or top
Anchor: bottom
Horizontal placement: centered
Exclusive zone: 0
Keyboard mode: on-demand or exclusive while visible
Namespace: jobowalls-shell
Decorations: none
Background: transparent
Taskbar visibility: no normal app chrome
```

The shell picker should not reserve screen space like a panel. It should float
over the desktop briefly.

Layer setup sequence:

1. Create `gtk::ApplicationWindow`.
2. Set it undecorated and transparent.
3. Call `init_layer_shell()` before presenting.
4. Set namespace to `jobowalls-shell`.
5. Set layer to `Layer::Overlay` if stable, otherwise `Layer::Top`.
6. Anchor to bottom.
7. Set bottom margin, default `48`.
8. Set exclusive zone to `0`.
9. Enable keyboard input while focused.
10. Present window.

Fallback behavior:

- If layer-shell is unavailable, open a small undecorated normal GTK window.
- In fallback mode, still preserve the same compact layout.
- Optionally print a warning in `--debug-window` mode only.

## Visual Design

The UI should be a compact carousel, not a full film strip.

Default shell shape:

```text
                 desktop remains visible

             ┌──────────────────────────┐
             │   prev  SELECTED  next   │
             └──────────────────────────┘
```

Recommended dimensions:

```text
Window width:         560-700px
Window height:        150-190px
Selected thumbnail:   240x135
Neighbor thumbnail:   132x74
Bottom margin:        48px
Border radius:        8px max
Visible items:        3 by default
Wide mode max items:  5 later
```

Styling direction:

- Transparent root window.
- Subtle smoked-glass carousel container.
- No oversized labels.
- No top toolbar.
- No full preview panel.
- No nested cards.
- No explanatory text in normal operation.
- Selected item gets the strongest border and scale.
- Neighbors sit slightly behind selected item.
- Current active wallpaper gets a tiny accent marker.
- Applying state uses a restrained warm border or spinner.

Color direction:

- Neutral dark glass base.
- One small accent color reused from JoboWalls GUI.
- Avoid a one-note purple or blue theme.
- Keep contrast high enough over varied wallpapers.

## Carousel Behavior

MVP visible items:

```text
previous
selected
next
```

Optional later wide layout:

```text
far previous
previous
selected
next
far next
```

Rules:

- Keep selected item centered.
- Render only visible items for speed.
- Use fixed thumbnail dimensions.
- Do not let filenames resize the overlay.
- Do not show long filenames by default.
- Use opacity, scale, and depth to imply carousel movement.
- Mouse wheel and trackpad move left/right.
- Click neighbor selects it.
- Double-click selected applies it.
- Enter applies current selection.
- Escape closes without applying.

Animation:

```text
Selection transition: 150-190ms
Transform: translate + scale
Opacity: selected 1.0, neighbors 0.68
Easing: cubic-bezier(.2, 0, .2, 1)
```

Avoid layout animation. Keep widget sizes fixed and animate transforms or CSS
classes where GTK supports it cleanly.

## Keyboard And Mouse Controls

MVP controls:

```text
Left / H       previous wallpaper
Right / L      next wallpaper
Enter          apply selected wallpaper and close
Escape         close without applying
S              shuffle selected wallpaper
R              rescan current folder
O              open folder prompt or file chooser
```

Mouse controls:

```text
Wheel up/left      previous
Wheel down/right   next
Click neighbor     select neighbor
Double-click       apply
```

Keep the shell picker usable entirely from the keyboard. This is essential for a
Hyprland keybind workflow.

## Media And Preview Support

Supported static formats should match the CLI:

```text
png
jpg
jpeg
webp
bmp
gif where reasonable
```

Supported live formats should match the CLI:

```text
mp4
webm
mkv
mov
avi
```

MVP preview policy:

- Thumbnail cards are always static.
- Static images use GTK-loadable thumbnail files.
- Every live wallpaper gets a generated poster thumbnail.
- Live preview means applying the selected wallpaper to the actual desktop
  background while browsing.
- Desktop live preview is debounced so fast scrolling does not spawn a backend
  command for every intermediate card.
- `Enter` keeps the currently previewed wallpaper and closes.
- `Escape` restores the original wallpaper when a live desktop preview changed
  it, then closes.

Reasoning: shell picker must appear fast, and the desktop itself is the preview
surface. Thumbnail cards should stay stable and cheap while static and live
wallpapers are previewed through the real backend path.

## Preview Cache

Reuse the existing cache directory:

```text
~/.cache/jobowalls/gui-thumbnails/
```

Add shell-specific cache variants only when dimensions differ:

```text
shell-static-v1.jpg
shell-poster-v1.jpg
```

MVP shell thumbnail profile:

```text
Static/poster thumbnail: 1080px wide
Neighbor previews: same poster cache, rendered smaller
```

Cache key should include:

- source path
- file length
- modified timestamp
- preview profile/version

Do not block the UI thread during thumbnail generation. Show a stable placeholder
and swap in the poster when ready. Queue preview work in priority order:
selected poster, then immediate neighbor posters.

## Applying Wallpapers

The shell picker should apply through the installed `jobowalls` binary.

MVP apply command:

```text
jobowalls set <path> --monitor <monitor>
```

Subprocess requirements:

- Capture stderr for user-facing error display.
- Do not shell-concatenate paths.
- Pass arguments structurally.
- Close only after success.
- Keep the window open on failure with a compact error state.

## State Model

The shell picker should read existing state to identify the active wallpaper:

```text
~/.local/state/jobowalls/state.json
```

The shell picker should persist shell-only UI state separately:

```text
~/.local/state/jobowalls/shell.json
```

Suggested shell state:

```json
{
  "version": 1,
  "last_folder": "/home/user/Pictures/Wallpapers",
  "last_monitor": "all",
  "last_index_by_folder": {
    "/home/user/Pictures/Wallpapers": 12
  }
}
```

Do not overload backend state with UI-only fields unless there is already an
established shared GUI state file.

Explanation:

- `state.json` is backend/runtime truth: current wallpaper, active backend,
  monitor entries, owned PIDs, and last successful wallpaper command.
- `shell.json` is frontend convenience state: last shell folder, last monitor,
  and remembered selection index.
- Keeping them separate avoids polluting backend state with UI preferences.
- Sharing a future GUI state file could be useful later, but `shell.json` is the
  clean MVP because `jobowalls-shell` is intentionally a separate frontend with
  different layout and behavior.

## Configuration

MVP should work without config.

Future config:

```toml
[shell]
position = "bottom"
visible_items = 3
width = 640
height = 170
bottom_margin = 48
close_after_apply = true
live_desktop_preview = true

[shell.preview]
static_width = 1080
```

Config must never be required for the default Omarchy use case.

## Hyprland Integration

The shell picker should not require Hyprland window rules if layer-shell works.

Recommended user keybind:

```conf
bindd = SUPER CTRL ALT, SPACE, JoboWalls Shell, exec, jobowalls-shell
```

Optional alternate bind:

```conf
bindd = SUPER CTRL ALT, SPACE, JoboWalls Shell, exec, jobowalls-shell ~/.config/omarchy/current/theme/backgrounds
```

Do not write these rules from the installer by default. Document them and let
users opt in, unless the user explicitly asks the installer to configure a bind.

## Packaging

The release artifact should eventually include:

```text
jobowalls
jobowalls-gui
jobowalls-shell
install.sh
uninstall.sh
README.md
packaging/arch/
```

Runtime dependencies for shell:

```text
gtk4
gtk4-layer-shell
ffmpeg or ffmpegthumbnailer for generated previews
mpvpaper for live/video wallpapers
```

Arch/Omarchy package dependency names should be verified before release work.

The installer should:

- Install `jobowalls-shell` if present in the release artifact.
- Not fail older releases that do not include it.
- Print a dependency hint if GTK/layer-shell libs are missing.
- Avoid compiling from source for normal curl installs when binaries exist.
- Prompt before installing the `mpvpaper` live wallpaper prerequisite.
- Show the exact Omarchy command before prompting:
  `omarchy pkg aur add mpvpaper`.
- Install `mpvpaper` only after explicit opt-in.
- If the user skips, explain that live wallpapers require `mpvpaper` and print
  the same command to run later.
- Offer an optional Hyprland keybind setup prompt for
  `SUPER CTRL ALT, SPACE -> jobowalls-shell`.
- Make the keybind prompt opt-in and avoid overwriting an existing binding
  without confirmation.

The uninstaller should:

- Remove `jobowalls-shell` alongside the other installed binaries.
- Leave user state/config/cache unless explicitly asked by a future purge flag.

## Implementation Milestones

### Phase 1: Skeleton Binary

- Add `jobowalls-shell` binary target.
- Start a GTK4 application.
- Open a small undecorated window.
- Load CSS.
- Render a static placeholder carousel.
- Add `--debug-window` mode.

Exit criteria:

- `cargo run --bin jobowalls-shell -- --debug-window` opens a test window.
- `Escape` closes it.

### Phase 2: Layer-Shell Window

- Add `gtk4-layer-shell`.
- Initialize the window as a layer surface.
- Anchor bottom center.
- Set transparent background.
- Set keyboard mode.
- Add fallback normal-window path.

Exit criteria:

- `jobowalls-shell` appears as a bottom overlay in Hyprland.
- It does not tile like a normal window.
- It closes cleanly.

### Phase 3: Folder Scan And Model

- Parse optional folder and monitor flags.
- Resolve default folder.
- Scan supported wallpapers.
- Sort files consistently.
- Load active wallpaper state.
- Select active wallpaper if present in folder.

Exit criteria:

- Shell picker opens on the current theme backgrounds folder.
- Active wallpaper is selected when present.
- Empty folder shows compact empty state.

### Phase 4: Carousel UI

- Render previous, selected, and next wallpaper.
- Add selected/neighbor visual states.
- Add active/applying states.
- Add stable thumbnail dimensions.
- Add keyboard and mouse navigation.

Exit criteria:

- Left/right navigation is smooth.
- Overlay size does not shift while browsing.
- Selected item is obvious without extra text.

### Phase 5: Preview Cache

- Generate static/poster thumbnails off the UI thread.
- Reuse existing cache directory.
- Show placeholders while generating.
- Use live posters for video wallpapers.
- Keep all thumbnail cards static.
- Avoid loading full-resolution images directly into tiny widgets.

Exit criteria:

- Large wallpaper folders remain responsive.
- Video wallpapers show poster thumbnails.
- Reopening is fast after cache generation.

### Phase 6: Apply Flow

- Debounce selection changes and apply the selected wallpaper to the desktop
  through existing set behavior.
- Show compact preview/applying state only when useful.
- `Enter` keeps the current live desktop preview and closes.
- `Escape` restores the original wallpaper if browsing changed it, then closes.
- Keep open and show compact error state on failure.

Exit criteria:

- `Enter` applies static and live wallpapers.
- Backend behavior matches `jobowalls set`.
- Failures do not leave the UI silently stuck.

### Phase 7: Polish

- Tune dimensions and opacity.
- Add selected filename only if it does not clutter the UI.
- Add shuffle and rescan.
- Add optional folder prompt.
- Improve error and empty states.
- Check behavior on single-monitor and multi-monitor setups.

Exit criteria:

- The picker feels fast enough to bind to a hotkey.
- It has no normal app-window feeling in the default path.

### Phase 8: Packaging

- Include `jobowalls-shell` in release builds.
- Update install and uninstall scripts.
- Update README.
- Update Arch packaging notes.
- Add release artifact verification.

Exit criteria:

- Curl install places `jobowalls-shell` on PATH.
- `jobowalls-shell --version` or equivalent smoke command works.
- README documents launch and keybind.

## Testing Plan

Unit tests:

- CLI flag parsing.
- Folder resolution.
- Media filtering.
- Sort order.
- Selection wraparound.
- State read/write for shell UI state.
- Apply command construction if using subprocess.

Manual tests:

```bash
cargo run --bin jobowalls-shell -- --debug-window
cargo run --bin jobowalls-shell
cargo run --bin jobowalls-shell ~/.config/omarchy/current/theme/backgrounds
cargo run --bin jobowalls-shell -- --monitor DP-1
```

Behavior checks:

- Opens as layer overlay.
- Does not tile.
- Does not reserve space.
- Keyboard navigation works.
- Mouse wheel navigation works.
- Enter applies.
- Escape closes.
- Static wallpapers apply through configured backend.
- Live wallpapers apply through `mpvpaper`.
- Missing preview tools degrade gracefully.
- Empty folder does not crash.

Release checks:

```bash
cargo test --locked
cargo build --release --locked --bin jobowalls-shell
```

If the shell binary shares release packaging with the GUI:

```bash
npm --prefix gui run typecheck
npm --prefix gui run tauri:build -- --no-bundle
```

## Open Decisions

Resolved decisions:

- `jobowalls-shell` lives in the main Cargo package as a new binary target.
- Applying wallpapers spawns the installed `jobowalls` binary for MVP
  simplicity.
- Live desktop preview is included in the shell MVP: selected wallpapers are
  applied to the actual desktop while browsing.
- Thumbnail cards remain static; live wallpapers use poster thumbnails.
- Shell UI state uses `shell.json`, separate from backend runtime state.
- The installer prompts before installing `mpvpaper`, shows
  `omarchy pkg aur add mpvpaper`, and prints that command again if skipped.
- The installer should offer an optional Hyprland keybind setup prompt.
- Installer keybind setup should offer `SUPER+CTRL+ALT+SPACE` for
  `jobowalls-shell` and only write config after explicit opt-in.
- The overlay exits immediately after successful apply.

Remaining decisions:

- None.

## Recommended MVP Decision Set

For the first implementation:

- Main Cargo package, new `src/bin/jobowalls-shell.rs`.
- Relm4 + GTK4 + `gtk4-layer-shell`.
- Five visible rolling carousel items.
- Static thumbnail cards for both static and live wallpapers.
- Live desktop preview by spawning the installed `jobowalls` binary with
  structured arguments after debounced selection changes.
- `Enter` keeps the previewed wallpaper and closes.
- `Escape` restores the original wallpaper and closes.
- `--debug-window` for normal-window development.
- Optional installer keybind setup prompt.

This keeps the MVP small while still proving the important product idea: a fast,
native, shell-layer wallpaper picker that feels like part of the desktop.

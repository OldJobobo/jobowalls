# jobowalls GUI Picker Plan

## Goal

Build a minimal Tauri-based visual wallpaper picker for `jobowalls`.

The GUI should behave like a fast desktop overlay: launch it, browse wallpapers
as a slick film-roll strip, preview the selected item, apply it through
`jobowalls`, and close. It is not a dashboard, configuration center, or second
wallpaper engine.

`PLAN.md` remains authoritative for backend behavior, state ownership, config,
and process management. This GUI is a frontend picker over that command model.

## Product Shape

The MVP should feel closer to a visual launcher than a full application.

Expected flow:

```bash
jobowalls-gui
jobowalls-gui /path/to/backgrounds
jobowalls-gui --monitor DP-1 /path/to/backgrounds
```

Behavior:

- Open over the current desktop.
- Load wallpapers from a folder.
- Show a horizontal film-roll strip.
- Keep the selected wallpaper centered and prominent.
- Show a larger preview of the selected wallpaper.
- Apply with `Enter`.
- Close with `Escape`.
- Use `jobowalls set` for the actual wallpaper change.

The default folder should target the current theme/backgrounds workflow first,
while still allowing custom invocation paths.

## Non-Goals For MVP

- No full collection dashboard.
- No permanent sidebar.
- No full config editor.
- No direct `swaybg`, `mpvpaper`, or `awww` integration.
- No direct process killing.
- No playlist editor.
- No tag/rating system.
- No complex metadata browser.

Future playlist and collection management should align with the main CLI
roadmap rather than being invented only in the GUI.

## Default Folder Resolution

Resolve the folder in this order:

1. Positional path passed to `jobowalls-gui`.
2. Saved last folder from GUI state.
3. Current theme backgrounds directory:

   ```text
   ~/.config/omarchy/current/theme/backgrounds
   ```

4. Fallback wallpaper directory:

   ```text
   ~/Pictures/Wallpapers
   ```

If no valid folder is found, open a minimal empty state with an `Open Folder`
action.

## Invocation Model

MVP command shape:

```bash
jobowalls-gui [folder]
```

Near-term optional flags:

```bash
jobowalls-gui --monitor all [folder]
jobowalls-gui --monitor DP-1 [folder]
jobowalls-gui --preview [folder]
jobowalls-gui --no-preview [folder]
```

Hyprland keybind target:

```text
bind = SUPER, W, exec, jobowalls-gui
bind = SUPER SHIFT, W, exec, jobowalls-gui ~/Pictures/Wallpapers
```

This repository should not edit Hyprland or Omarchy config as part of the GUI
implementation unless explicitly requested.

## UX Layout

Primary screen:

```text
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│                  focused wallpaper preview                  │
│                                                              │
│                                                              │
│      <    small   medium   SELECTED   medium   small    >    │
│                                                              │
│          filename.png      static/live      monitor          │
│                                                              │
│              Apply        Shuffle        Open Folder         │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

Visual direction:

- Dark translucent overlay.
- Large focused preview.
- Film-roll strip across the lower portion.
- Selected item centered.
- Side items scaled down and dimmed.
- Smooth transform-based animation.
- Minimal text.
- Small icon-forward controls.
- No large app chrome.
- No card-heavy dashboard.

## Film-Roll Behavior

The film roll is the core interaction.

Requirements:

- Horizontal navigation.
- Selected item is centered and visually dominant.
- Nearby items remain readable.
- Farther items fade and scale down.
- Movement uses CSS transforms instead of layout shifts.
- Mouse wheel, trackpad horizontal scroll, arrow keys, and click selection work.
- Double click applies the selected wallpaper.

Suggested item states:

```text
selected
near
far
active-current-wallpaper
loading-thumbnail
thumbnail-failed
applying
```

Suggested animation baseline:

```css
.film-item {
  transition:
    transform 180ms ease,
    opacity 180ms ease,
    filter 180ms ease;
}

.film-item.selected {
  transform: scale(1);
  opacity: 1;
}

.film-item.near {
  transform: scale(0.78);
  opacity: 0.72;
}

.film-item.far {
  transform: scale(0.58);
  opacity: 0.35;
}
```

Keep layout dimensions stable so thumbnail loading and active/applying states do
not resize the strip.

## Keyboard Controls

MVP controls:

```text
Left / H       previous wallpaper
Right / L      next wallpaper
Enter          apply selected wallpaper
Space          preview or apply, depending configured mode
S              shuffle selection
O              open/change folder
R              rescan current folder
Escape         close picker
```

Keyboard behavior should be reliable enough for the picker to work well from a
single Hyprland keybind.

## Wallpaper Support

Use the same media categories as the CLI.

Static wallpapers:

```text
png
jpg
jpeg
webp
```

Live wallpapers:

```text
mp4
webm
mov
mkv
```

The GUI may scan folders itself for the MVP, but the supported extension list
must stay aligned with `src/media.rs`. A later phase can move scanning behind a
`jobowalls collection scan --json` command.

## CLI Contract Needed For MVP

Keep the CLI additions narrow.

Required:

```bash
jobowalls status --json
jobowalls set /path/to/wallpaper --dry-run --json
```

Actual apply command:

```bash
jobowalls set /path/to/wallpaper
jobowalls set /path/to/wallpaper --monitor DP-1
jobowalls set /path/to/wallpaper --monitor all
```

The GUI should call `jobowalls set`; it should not call wallpaper backends
directly.

Example dry-run JSON:

```json
{
  "wallpaper": "/home/user/Pictures/Wallpapers/wall.png",
  "media_kind": "static",
  "backend": "swaybg",
  "monitor": "all"
}
```

Example status JSON:

```json
{
  "state_exists": true,
  "active_backend": "swaybg",
  "mode": "static",
  "wallpaper": "/home/user/Pictures/Wallpapers/wall.png",
  "monitors": {
    "all": {
      "backend": "swaybg",
      "wallpaper": "/home/user/Pictures/Wallpapers/wall.png",
      "pid": null
    }
  },
  "updated_at": "2026-05-08T00:00:00-07:00"
}
```

## Tauri Architecture

Place the GUI under:

```text
gui/
```

Suggested stack:

```text
Tauri 2
React
TypeScript
Vite
lucide-react
```

Suggested frontend structure:

```text
gui/src/
  App.tsx
  components/
    FilmRoll.tsx
    PreviewStage.tsx
    PickerControls.tsx
    EmptyState.tsx
  lib/
    invoke.ts
    keyboard.ts
    paths.ts
    types.ts
```

Suggested Tauri backend structure:

```text
gui/src-tauri/src/
  lib.rs
  jobowalls.rs
  scan.rs
  thumbnails.rs
  gui_state.rs
```

Tauri commands:

```rust
resolve_startup_folder(input_path: Option<String>) -> Result<Option<String>, String>
scan_folder(path: String) -> Result<Vec<WallpaperItem>, String>
get_status() -> Result<JobowallsStatus, String>
preview_plan(path: String, monitor: Option<String>) -> Result<SetPlanPreview, String>
apply_wallpaper(path: String, monitor: Option<String>) -> Result<(), String>
choose_folder() -> Result<Option<String>, String>
get_thumbnail(path: String) -> Result<ThumbnailResult, String>
save_last_folder(path: String) -> Result<(), String>
```

All process calls must use argument arrays through `std::process::Command`, not
shell-interpolated strings.

## Binary Resolution

The Tauri backend should find `jobowalls` in this order:

1. GUI setting override, when that exists.
2. Development binary:

   ```text
   ../target/debug/jobowalls
   ```

3. `jobowalls` from `PATH`.
4. Packaged sidecar binary in release builds.

MVP can start with development binary plus `PATH` resolution.

## GUI State

Store GUI-only state separately:

```text
~/.local/state/jobowalls/gui.json
```

Suggested shape:

```json
{
  "version": 1,
  "last_folder": "/home/user/.config/omarchy/current/theme/backgrounds",
  "last_monitor": "all",
  "preview_mode": false
}
```

Do not store backend PIDs, active backend truth, or monitor ownership here.
Those belong to the existing `jobowalls` state file.

## Thumbnail Cache

Use:

```text
~/.cache/jobowalls/gui-thumbnails/
```

Static images:

- Generate small thumbnails.
- Prefer a Rust-side implementation for reliability.
- Cache by path, modified time, file size, and requested thumbnail size.

Live wallpapers:

- Generate a poster frame with `ffmpeg` or `ffmpegthumbnailer` when available.
- Fall back to a clean video placeholder if no tool is available.

The picker should not wait for all thumbnails before opening. It should show
placeholder tiles immediately and fill thumbnails as they are ready.

## Preview Behavior

MVP preview:

- Static images show in the focused preview stage.
- Videos show a poster frame.
- Active wallpaper is highlighted in the film roll.

Later:

- Muted video playback for selected live wallpapers.
- Optional temporary wallpaper preview while browsing.
- Revert-on-close behavior for preview mode.

Temporary preview mode must be designed carefully because it changes real
wallpaper state while browsing.

## Apply Behavior

When the user applies a wallpaper:

1. Set selected item to applying state.
2. Run `jobowalls set`.
3. Refresh `jobowalls status --json`.
4. Mark the new current wallpaper active.
5. Close the picker by default.

Apply command examples:

```bash
jobowalls set /path/to/wallpaper
jobowalls set /path/to/wallpaper --monitor DP-1
```

If the apply command fails, keep the picker open and show a compact error.

## Error Handling

Common error states:

- No valid folder found.
- Folder contains no supported wallpapers.
- `jobowalls` binary not found.
- `jobowalls set` failed.
- `jobowalls status --json` failed or returned invalid JSON.
- Thumbnail generation failed.
- Selected file disappeared.

Error UI should be compact and local:

- Empty state for folder problems.
- Small inline message for apply failures.
- Thumbnail placeholder for thumbnail failures.

Avoid modal-heavy flows in the MVP.

## MVP Build Order

1. Add `status --json` to the CLI.
2. Add `set --dry-run --json` to the CLI.
3. Scaffold `gui/` with Tauri, React, TypeScript, and Vite.
4. Implement startup folder resolution.
5. Implement folder scan in the Tauri backend.
6. Build the film-roll UI with placeholder tiles.
7. Add keyboard navigation.
8. Add focused preview for images.
9. Add `Enter` apply through `jobowalls set`.
10. Refresh active state from `status --json`.
11. Add static image thumbnail cache.
12. Add video poster thumbnails.
13. Add folder picker and `O` shortcut.
14. Add shuffle and rescan.
15. Polish animation, overlay styling, and launch speed.

## Validation

CLI validation:

```bash
cargo fmt
cargo test
cargo run -- status --json
cargo run -- set /path/to/wall.png --dry-run --json
```

GUI validation:

```bash
cd gui
npm run typecheck
npm run tauri dev
```

Manual scenarios:

- Launch with no arguments and load the current theme backgrounds folder.
- Launch with an explicit folder.
- Navigate with arrow keys.
- Apply a static image.
- Apply a live wallpaper.
- Confirm active wallpaper highlight updates.
- Open an empty folder and see a useful empty state.
- Run without `jobowalls` in `PATH` and see a useful error.

## Future Work

These should stay aligned with the main CLI roadmap:

- Playlist support.
- Collection history.
- Recursive collection scanning.
- Favorites.
- Recently used wallpapers.
- Per-monitor visual picker.
- Preview mode with revert-on-close.
- Video hover playback.
- Transition controls for `awww`.
- Theme-aware folder profiles.
- Packaged release with bundled `jobowalls` sidecar.

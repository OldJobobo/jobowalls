# jobowalls Planning Notes

## Goal

`jobowalls` should make wallpaper changes feel seamless from the user's point of
view, regardless of whether the target wallpaper is a static image or a live
video. The user should issue one command or use one UI entry point, while
`jobowalls` chooses and coordinates the correct wallpaper backend.

Primary backend targets:

- `swaybg` for static wallpapers.
- `mpvpaper` for live/video wallpapers.
- Optional `awww` support for animated static-image transitions.
- Optional `awww-daemon` lifecycle management when `awww` is enabled.

## User Experience

The public behavior should be simple:

```bash
jobowalls set ~/Pictures/wallpapers/sakura.png
jobowalls set ~/Videos/wallpapers/rain.mp4
jobowalls next
jobowalls previous
jobowalls status
jobowalls stop-live
```

Expected behavior:

- Static image changes unload or hide live wallpaper processes cleanly.
- Live wallpaper changes stop static-only daemons only when necessary.
- Switching between static and live wallpapers should not flash to a blank
  screen if a backend can avoid it.
- Multi-monitor handling should work by default, with monitor-specific override
  support.
- Backend differences should be hidden behind one command model.
- The last selected wallpaper should be persisted and restorable after login.

## Design Principles

- One user command, multiple backend adapters.
- Explicit process ownership: `jobowalls` should only kill processes it started
  or processes matching configured ownership rules.
- Backend detection should be automatic, but configurable.
- State should be stored in a small readable file rather than inferred entirely
  from process lists.
- No Hyprland config edits should be required for the basic CLI workflow.

## Proposed Architecture

```text
jobowalls
  CLI
    parses user intent
    validates media paths
    resolves monitor selection
    calls orchestrator

  Orchestrator
    determines media type
    selects backend
    coordinates backend shutdown/startup
    persists current state

  Backend adapters
    swaybg
    mpvpaper
    awww optional

  State store
    current wallpaper
    backend in use
    monitor mapping
    owned process IDs
    last successful command

  Config
    backend preference order
    monitor policy
    transition settings
    live wallpaper mpv options
    startup restore behavior
```

## Backend Responsibilities

### swaybg

Use for static wallpapers when `awww` is disabled or unavailable.

Responsibilities:

- Start `swaybg` for the requested static wallpaper.
- Target one monitor with `-o` when requested, or all outputs otherwise.
- Track `jobowalls`-owned process IDs in state.
- Stop only previously recorded `jobowalls`-owned `swaybg` processes for the
  target monitor set before starting a replacement.

Likely commands:

```bash
swaybg -i /path/to/image.png -m fill
swaybg -i /path/to/image.png -m fill -o DP-1
```

### mpvpaper

Use for live/video wallpapers.

Responsibilities:

- Start one `mpvpaper` process per monitor, unless a global mode is configured.
- Pass consistent mpv flags for looping, muted playback, no audio, and GPU-safe
  behavior.
- Track process IDs in the state file.
- Stop prior `mpvpaper` processes before changing live wallpapers.
- Stop live wallpaper processes when switching back to static images.

Default mpvpaper profile:

```text
--loop
--no-audio
--panscan=1.0
```

Exact flags should be verified against local `mpvpaper --help` before
implementation.

### awww

Use optionally for static wallpapers when transitions matter. The
out-of-the-box static path should prefer `swaybg` so the tool works with
Omarchy defaults. If `awww.enabled = true`, auto static backend selection should
prefer `awww` when available. A user can still force it for a single command
with `--backend awww`.

Responsibilities:

- Detect or start `awww-daemon`.
- Apply image wallpapers with configured transition options.
- Defer to `swaybg` if `awww` is disabled or unavailable.
- Stop owned live wallpaper processes after the new static wallpaper is visible.

Example static transition intent:

```bash
awww img /path/to/image.png --transition-type grow --transition-duration 2.4 \
  --transition-fps 60 --transition-pos center \
  --transition-bezier .42,0,.2,1 --transition-wave 28,12
```

Exact flags should be verified against local `awww img --help` before
implementation.

## State Model

Suggested path:

```text
~/.local/state/jobowalls/state.json
```

Suggested structure:

```json
{
  "version": 1,
  "active_backend": "mpvpaper",
  "mode": "live",
  "wallpaper": "/home/user/Videos/wallpapers/rain.mp4",
  "monitors": {
    "DP-1": {
      "backend": "mpvpaper",
      "wallpaper": "/home/user/Videos/wallpapers/rain.mp4",
      "pid": 12345
    }
  },
  "last_command": "set /home/user/Videos/wallpapers/rain.mp4 --monitor DP-1 --backend mpvpaper",
  "updated_at": "2026-05-08T00:00:00-07:00"
}
```

## Config Model

Suggested path:

```text
~/.config/jobowalls/config.toml
```

Initial shape:

```toml
[general]
static_backend = "auto" # auto, swaybg, awww
live_backend = "mpvpaper"
restore_on_startup = true

[monitors]
default = "all"

[monitors.profiles.DP-1]
wallpaper = "/home/user/Pictures/wallpapers/main.png"
backend = "auto"

[monitors.profiles.HDMI-A-1]
wallpaper = "/home/user/Videos/wallpapers/side.mp4"
backend = "auto"

[mpvpaper]
mode = "per-monitor"
extra_args = ["--loop", "--no-audio"]
readiness_timeout_ms = 5000

[live.pause]
on_battery = true
on_fullscreen = true
on_idle = true
resume_on_ac = true
resume_on_unfullscreen = true
resume_on_activity = true

[awww]
enabled = false
transition_type = "grow"
transition_duration = 2.4
transition_fps = 60
transition_pos = "center"
transition_bezier = ".42,0,.2,1"
transition_wave = "28,12"
```

## Media Detection

Classify by extension first, then optionally by MIME type if available.

Static:

- `.jpg`
- `.jpeg`
- `.png`
- `.webp`
- `.bmp`
- `.gif` as a static fallback when `awww` is not active.

Live:

- `.mp4`
- `.webm`
- `.mkv`
- `.mov`
- `.avi`

Do not guess remote URLs in the first version. Require local files.

## CLI Surface

Initial commands:

```text
jobowalls set <path> [--monitor <name|all>] [--backend <auto|swaybg|mpvpaper|awww>]
jobowalls status
jobowalls stop
jobowalls restore
jobowalls list-monitors
jobowalls doctor
```

Later commands:

```text
jobowalls next [collection]
jobowalls previous [collection]
jobowalls shuffle [collection]
jobowalls daemon
```

## Future Feature Goals

Collections and playlists should be future feature goals rather than first
version scope. The eventual collection workflow should support `next`,
`previous`, and `shuffle` commands with persisted progress, while the first
version should stay focused on reliable single-wallpaper static/live switching,
restore, status, and doctor behavior.

## Remaining Work Checklist

Core v1 completion:

- [x] Verify `mpvpaper` argument order and supported flags against local
  `mpvpaper --help`.
- [x] Verify `awww img` transition and output flags against local
  `awww img --help`.
- [x] Add optional MIME-based media detection after extension classification.
- [x] Decide whether `.gif` without `awww` should be unsupported or treated as a
  static fallback.
- [x] Add real desktop validation for static-to-static switching.
- [x] Add real desktop validation for static-to-live switching.
- [x] Add real desktop validation for live-to-static switching.
- [x] Add real desktop validation for multi-monitor live wallpaper.
- [x] Confirm no unrelated user wallpaper processes are killed during backend
  switches.

Per-monitor wallpaper support:

- [ ] Make restore use per-monitor backend and wallpaper entries as
  authoritative for mixed-monitor state instead of cloning top-level state
  values across every monitor.
- [x] Add config parsing for `[monitors.profiles.<name>]` entries.
- [x] Preserve existing monitor state when `set --monitor <name>` targets one
  monitor.
- [x] Allow mixed static and live wallpapers across different monitors.
- [x] Make `restore` apply monitor profiles for active outputs.
- [x] Fall back to the global monitor/default policy when an active monitor has
  no profile.
- [x] Add tests for profile restore.

Live wallpaper pause support:

- [x] Add `jobowalls daemon` or an equivalent watcher process.
- [x] Add config parsing for `[live.pause]` options.
- [x] Implement pause-on-battery detection.
- [x] Implement resume-on-AC behavior.
- [x] Implement pause-on-fullscreen detection.
- [x] Implement resume when fullscreen exits.
- [x] Implement pause-on-idle detection.
- [x] Implement resume-on-activity behavior.
- [x] Ensure pause/resume only affects `jobowalls`-owned live wallpaper
  processes.
- [x] Add tests or mocked integration coverage for pause trigger decisions.

Architecture and maintenance:

- [ ] Verify a newly spawned `swaybg` process is still running before recording
  static wallpaper state as successfully applied.
- [ ] Improve static backend availability errors so missing `swaybg`/`awww`
  reports a clear install or config action.
- [ ] Document Omarchy current-background `swaybg` handling as an explicit
  configured ownership rule, including tests that show the allowed process
  matching boundary.
- [x] Extract monitor discovery into `src/monitors.rs`.
- [x] Introduce the planned `WallpaperBackend` trait once backend
  behavior is stable enough to benefit from the abstraction.
- [x] Add a user service template for
  `~/.config/systemd/user/jobowalls-restore.service`.
- [x] Keep collection and playlist commands as future feature goals unless they
  become first-version scope.

## Implementation Language

`jobowalls` will be implemented in Rust.

Rust is the planned language because this tool needs reliable process
ownership, explicit state transitions, structured config parsing, and a clean
single-binary CLI. The extra setup cost is worth it for a wallpaper orchestrator
that has to coordinate multiple external backends without killing unrelated
user processes.

Expected crate choices:

- `clap` for CLI parsing.
- `serde`, `serde_json`, and `toml` for config and state files.
- `anyhow` for application-level errors.
- `thiserror` for typed backend/orchestrator errors if needed.
- `dirs` or `etcetera` for XDG config/state paths.

Shell snippets may still be useful while validating backend commands manually,
but the product implementation should be Rust from the start.

## Suggested Rust Module Layout

```text
src/
  main.rs
  cli.rs
  config.rs
  state.rs
  media.rs
  monitors.rs
  orchestrator.rs
  backends/
    mod.rs
    swaybg.rs
    mpvpaper.rs
    awww.rs
```

Core traits:

```rust
trait WallpaperBackend {
    fn name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    fn supports(&self, media: &MediaKind) -> bool;
    fn apply(&self, request: &ApplyRequest) -> Result<BackendState>;
    fn stop(&self, state: &State) -> Result<()>;
}
```

## Orchestration Flow

For `jobowalls set <path>`:

1. Canonicalize the path.
2. Classify media as static or live.
3. Read config.
4. Read current state if present.
5. Discover monitors through `hyprctl monitors -j`.
6. Select backend.
7. If changing backend mode, stop owned processes from the previous backend.
   When replacing an existing `mpvpaper` live wallpaper with another live
   wallpaper, start the new owned `mpvpaper` process first, wait for mpv IPC to
   report video output readiness, then stop the previous owned live PIDs to
   avoid revealing the underlying static wallpaper between videos.
8. Apply the new wallpaper.
9. Verify success where possible.
10. Persist new state.

## Monitor Handling

Use `hyprctl monitors -j` as the source of truth.

Policies:

- `all`: apply to every active monitor.
- named monitor: apply to one monitor.
- per-monitor profiles in config should be first-class.

When setting a wallpaper for a named monitor, preserve the existing state for
other monitors unless the user explicitly targets `all`. This lets users mix
static and live wallpapers across outputs without rebuilding the full monitor
map on every command.

Per-monitor profile support should allow saved wallpaper/backend preferences per
output. Restore should apply each active monitor's profile when present and fall
back to the global default policy for monitors without a profile.

For `mpvpaper`, launch one process per monitor:

```bash
mpvpaper DP-1 /path/to/video.mp4 --loop --no-audio
mpvpaper HDMI-A-1 /path/to/video.mp4 --loop --no-audio
```

Exact argument order should be verified locally.

## Live Wallpaper Pause

Live wallpapers should support configurable pause behavior for power and focus
conditions. This requires a long-running `jobowalls daemon` or equivalent
watcher rather than only one-shot CLI commands.

Initial pause triggers:

- battery power
- fullscreen windows
- idle state

Resume behavior should mirror the trigger when possible:

- resume on AC power
- resume when fullscreen exits
- resume on user activity

The daemon should pause only live wallpapers owned by `jobowalls`, using the
state file's recorded process IDs and monitor map. Static wallpapers should be
left untouched.

When switching from live wallpaper to static wallpaper through `awww`, bypass
the animated transition. Apply the new static wallpaper instantly while the live
wallpaper is still covering the output, then stop the owned live process. This
avoids revealing the stale static wallpaper that was underneath the live layer.

## Startup Integration

Provide a user service later:

```text
~/.config/systemd/user/jobowalls-restore.service
```

First version can expose:

```bash
jobowalls restore
```

Then users can wire it into Hyprland or systemd manually.

Possible Hyprland integration:

```text
exec-once = jobowalls restore
```

## Doctor Checks

`jobowalls doctor` should report:

- Hyprland session detected.
- `hyprctl` available.
- `swaybg` available.
- `mpvpaper` available.
- `awww` and `awww-daemon` availability.
- Active monitors.
- Current state file readability.
- Missing stale process IDs.

## First Milestone

Build a minimal CLI that supports:

- `jobowalls set <static-image>` via `swaybg`.
- `jobowalls set <video>` via `mpvpaper`.
- `jobowalls stop` to stop owned live wallpaper processes.
- `jobowalls status` to print the state file.
- `jobowalls doctor` for backend availability.

Definition of done:

- Static-to-static switch works.
- Static-to-live switch works.
- Live-to-static switch works.
- Multi-monitor live wallpaper works.
- State survives shell restarts.
- No unrelated user wallpaper processes are killed.

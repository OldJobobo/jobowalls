# jobowalls Test Plan

## Goal

Make `jobowalls` safe to change by covering user-facing CLI behavior, process
ownership, GUI bridge behavior, installer packaging, and the existing pure Rust
logic.

Current baseline:

- `cargo test` passes with 73 tests.
- Existing coverage is strongest around media classification, config defaults,
  backend command construction, state serialization, collection ordering,
  monitor output parsing, and shell picker layout helpers.
- The largest gaps are integration behavior, process safety, Tauri backend
  helpers, frontend validation, and installer scripts.

## Phase 1: CLI Integration Harness

Add integration tests under `tests/cli.rs`.

Build a reusable harness that:

- Creates temp config and state paths.
- Creates fake backend binaries in a temp `PATH`.
- Records every fake backend invocation to a log file.
- Runs the built `jobowalls` binary with `--config` and `--state`.
- Uses temp wallpaper files with real-looking extensions or signatures.
- Avoids real Hyprland, `mpvpaper`, `swaybg`, `awww`, or `kill` calls where
  possible.

Core test cases:

- `set static --dry-run --json` emits valid JSON with canonical path, backend,
  monitor, and media kind.
- `set live --dry-run --json` emits `mpvpaper`.
- `set missing-file` fails before backend calls.
- `status --json` returns `state_exists: false` when no state exists.
- `config print-default` emits parseable TOML.
- `config init` writes config and refuses overwrite without `--force`.
- `list-monitors` uses fake `hyprctl -j monitors`.

## Phase 2: Backend Sequencing

Use fake binaries to verify command order and state writes.

Static `swaybg`:

- Setting static with auto backend chooses fake-available `swaybg`.
- Replaces only recorded owned `swaybg` PID for the targeted monitor.
- Writes state with backend `swaybg`, monitor entry, PID, and last command.

Static `hyprpaper`:

- Forced `--backend hyprpaper` uses daemon reachability check, preload,
  wallpaper, and unload in the expected order.
- `--monitor all` expands active monitors from fake `hyprctl`.
- `hyprpaper.unload_unused = false` skips unload.

Static `awww`:

- Forced `--backend awww` starts/checks daemon and runs `awww img`.
- Live-to-static uses the instant transition command.
- Static-to-static uses configured transition settings.

Live `mpvpaper`:

- `--monitor all` starts one process per active monitor.
- Records one PID per monitor.
- Previous live PIDs are terminated only after the new live wallpaper is ready.
- On readiness failure, new PIDs are terminated and old state remains usable.

## Phase 3: Process Ownership Safety

Add focused unit tests where functions are pure enough, plus CLI integration
tests for risky flows.

Test cases:

- `owned_live_pids_for_monitors` returns only `Mpvpaper` entries.
- `owned_swaybg_pids_for_monitors` returns only `Swaybg` entries and de-dupes
  duplicate PIDs.
- Monitor `all` targets all owned matching backend PIDs.
- A named monitor targets only that monitor.
- `stop-live` clears only live PIDs and leaves static `swaybg` PIDs untouched.
- Stale or nonexistent PIDs are cleared from state without counting as stopped.
- No process-killing path touches unrelated backend entries.

Implementation note: consider refactoring PID termination behind a trait or
small injectable function so tests do not need to fake `/bin/kill`.

## Phase 4: Restore Coverage

Add tests for restore behavior.

Test cases:

- Restoring static state rebuilds a set plan and records
  `last_command = "restore"`.
- Restoring live state starts fresh `mpvpaper` processes per saved monitor.
- Old live PIDs are terminated only after new live processes are ready.
- Failed live restore terminates new PIDs and reports a useful error.
- Restore preserves `collections`.
- Monitor profiles override saved state when configured.
- Missing profile wallpaper fails with monitor name and path in the error.

## Phase 5: Collection CLI Persistence

Existing collection selection is tested. Add full CLI/state behavior.

Test cases:

- `next` from no collection state selects the expected item after wrap logic.
- `previous` wraps backward.
- `shuffle` records shuffle history.
- Exhausted shuffle history allows reuse.
- Collection progress is preserved across later `set` commands.
- Named monitor defaults from single-monitor state when collection command omits
  `--monitor`.
- Multi-monitor state defaults collection command back to `all`.

## Phase 6: Monitor Discovery

Parsing is tested; fallback behavior is not.

Test cases with fake commands:

- Valid `hyprctl -j monitors` wins.
- Empty JSON falls back to text parsing.
- Failing JSON command falls back to `hyprctl monitors`.
- Failing Hyprland commands fall back to `mpvpaper --help-output`.
- All failures return an error containing both backend failures.
- Empty outputs return a clear no-monitor-names error.

## Phase 7: GUI Tauri Backend Tests

Add Rust tests in `gui/src-tauri/src/lib.rs`, or split helpers into a testable
module.

Test cases:

- `expand_home("~")` and `expand_home("~/Pictures")`.
- Startup folder precedence: explicit input, argv, saved last folder, Omarchy
  backgrounds, Pictures/Wallpapers, then none.
- `scan_folder` returns only supported files and sorts case-insensitively.
- GUI media classification matches CLI classification.
- `mime_for_path` returns correct MIME types.
- Cache path changes when file size or mtime changes.
- Cache path is stable when metadata is unchanged.
- `data_source` returns valid `data:mime;base64,...`.
- `resolve_jobowalls_binary` honors `JOBOWALLS_BIN`.
- `preview_plan` and `apply_wallpaper` build expected CLI args, preferably
  after refactoring command execution behind an injectable helper.

## Phase 8: Frontend Tests

At minimum, make these CI gates:

- `npm --prefix gui run typecheck`
- `npm --prefix gui run build`

If adding Vitest and React Testing Library:

- Renders empty state when no wallpapers exist.
- Renders film roll items from scan results.
- Keyboard navigation changes selection.
- Apply action calls the Tauri command with selected path and monitor.
- Preview mode toggles the expected media-source command.
- Live preview warmup is called only for live items.
- Error states are displayed without breaking selection.

## Phase 9: Installer and Packaging Tests

Use `bats` or `shellspec` with temp directories and fake commands.

`install.sh` tests:

- `--help` exits successfully.
- Release URL is correct for latest and explicit version.
- Release install copies `jobowalls`, `jobowalls-gui`, optional
  `jobowalls-shell`, and desktop file.
- Old release shape with GUI binary only creates `jobowalls-gui-bin` plus
  wrapper.
- Desktop `Exec=` line is rewritten to installed wrapper path.
- Release download failure falls back to source build.
- Permission failure returns status `2` and prints recovery guidance.
- Custom `PREFIX`, `BINDIR`, and `APPDIR` are respected.
- PATH update is idempotent.
- Fish path config is written only when appropriate.
- Noninteractive optional `mpvpaper` install is skipped safely.

`uninstall.sh` tests:

- Removes all installed files.
- Succeeds when files are already absent.
- Respects custom `PREFIX`, `BINDIR`, and `APPDIR`.

## Phase 10: Packaging Smoke Tests

Add lightweight checks for:

- `cargo fmt --check`
- `cargo test`
- `cargo build --release`
- `cargo build --bin jobowalls-shell`
- `npm --prefix gui run typecheck`
- `npm --prefix gui run build`
- Tauri build when CI has the required system dependencies.
- Installer shell tests.
- `shellcheck install.sh uninstall.sh packaging/linux/jobowalls-gui`

## Additional Tests

- Config migration and forward compatibility: unknown fields, missing sections,
  and older state without `collections`.
- State corruption: invalid JSON returns a helpful error and does not overwrite
  automatically.
- Paths with spaces, quotes, commas, and Unicode filenames.
- Monitor names with punctuation, especially for mpv IPC socket sanitization.
- Media signature tests for extensionless MP4, WebM, JPEG, and WebP.
- Backend override rejection for every invalid pair: `mpvpaper` with image and
  `hyprpaper`/`awww`/`swaybg` with video.
- `doctor` output smoke test with fake commands, especially stale PID reporting.
- Concurrency around GUI thumbnail lock: two callers requesting the same cache
  path should not run generation twice after a cached file appears.
- README command examples smoke-tested where feasible with `--dry-run`.

## Suggested Order

1. Add the CLI integration harness.
2. Cover `set`, `status`, `config`, and `list-monitors` dry-run paths.
3. Add process ownership tests before changing more backend logic.
4. Add restore and collection persistence tests.
5. Add GUI backend helper tests.
6. Add installer shell tests.
7. Wire validation into CI or a local `make test-all` script.

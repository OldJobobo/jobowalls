# Repository Guidelines

## Project Structure & Module Organization

This is an early Rust CLI. `PLAN.md` is the source of truth.

Planned runtime paths from the design:

- CLI entry point: `jobowalls`
- User config: `~/.config/jobowalls/config.toml`
- State file: `~/.local/state/jobowalls/state.json`
- Backends: `hyprpaper` for static images, `mpvpaper` for video/live wallpapers,
  and optional `awww` support for static transitions.

Code lives under `src/`: `cli.rs` parses commands, `orchestrator.rs` chooses
backends, `config.rs` and `state.rs` own serialization, and `src/backends/`
holds backend types.

## Build, Test, and Development Commands

Use Cargo for normal development:

- `cargo fmt` formats Rust sources.
- `cargo test` runs unit tests.
- `cargo run -- set /path/to/wall.png --dry-run` prints planned commands.
- `cargo run -- set /path/to/wall.png` applies a static wallpaper.
- `cargo run -- set /path/to/wall.mp4 --monitor all` targets every monitor.
- `cargo run -- stop-live` stops recorded live wallpaper PIDs.
- `cargo run -- restore` reapplies the last recorded wallpaper state.
- `cargo run -- next /path/to/collection --dry-run` previews collection advance;
  `previous` and `shuffle` use the same flags.
- `cargo run -- doctor` reports paths, backends, and state.
- `cargo run -- config print-default` prints the default TOML config.

`list-monitors` needs a running Hyprland or mpvpaper output context.

Verify backend flags with `mpvpaper --help` or `awww img --help`.

## Coding Style & Naming Conventions

Keep modules focused on one responsibility: CLI parsing, orchestration, backend
adapters, config loading, and state persistence. Use design names such as
`active_backend`, `restore_on_startup`, and `owned_process_ids`.

Persist structured data with real parsers. Use JSON for state and TOML for
config as described in `PLAN.md`; avoid shell-style parsing for these files.

## Testing Guidelines

Add tests alongside each implementation slice. Current tests cover media
classification, backend selection, config defaults, and state serialization.
Mock backend commands instead of starting real services.

Name tests after behavior, for example `selects_mpvpaper_for_mp4` or
`does_not_kill_unowned_processes`.

## Commit & Pull Request Guidelines

Local Git history is unavailable here, so no existing convention can be
inferred. Use imperative messages such as `Add initial CLI skeleton`.

Pull requests should include a summary, validation steps, linked issues when
applicable, and screenshots or recordings for visible wallpaper behavior. Call
out config, state-file, or backend process-management changes.

## Agent-Specific Instructions

Treat `PLAN.md` as authoritative unless the user gives newer direction. Avoid
editing Hyprland or Omarchy config as part of this repository unless explicitly
requested. Do not add process-killing behavior without tests or clear ownership
guards.

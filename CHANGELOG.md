# Changelog

All notable user-facing changes are tracked here. Versions follow semantic
versioning while the project is pre-1.0.

## 0.1.15 - 2026-05-11

- Added vertical shell picker layout support with left and right side
  placements.
- Added `--layout` and `[shell].layout` so horizontal and vertical picker
  orientation can be configured.
- Added `Ctrl+P` placement cycling between bottom, left, top, and right picker
  positions.
- Improved shell layer-shell placement so the picker panel uses edge anchors
  and calculated margins instead of a fullscreen panel surface.
- Reduced shell carousel twitching during repeated key navigation by snapping
  rapid same-direction repeats instead of restarting every transition.
- Reduced preview churn while navigating live wallpapers and kept selection
  borders in a fixed overlay so image loading does not resize cards.

## 0.1.14 - 2026-05-11

- Added `VERSION`, `CHANGELOG.md`, and `RELEASE_NOTES.md`.
- Added version consistency checks across root Cargo, GUI Cargo, npm package
  metadata, and Tauri config.
- Made root and GUI Cargo builds fail when `VERSION` drifts from package
  metadata.
- Made release jobs check version metadata and publish release notes from
  `RELEASE_NOTES.md`.
- Added `[gui]` and `[shell]` config sections to `~/.config/jobowalls/config.toml`.
- Made GUI and shell startup defaults configurable while keeping CLI flags as
  launch-time overrides.
- Made the GUI consume active Omarchy theme colors from
  `~/.config/omarchy/current/theme/colors.toml`.
- Documented `jobowalls stop-live` as the command to shut down
  `jobowalls`-managed live wallpapers before using external wallpaper pickers.

## 0.1.13 - 2026-05-11

- Fixed release packaging so `jobowalls-gui-bin` is rebuilt after tests and
  cannot ship as a placeholder script.
- Added package-time ELF validation for all shipped Linux binaries.
- Updated the release workflow to Node 24-native GitHub Actions.

## 0.1.12 - 2026-05-11

- Fixed CI monitor discovery assumptions in restore integration tests.
- Isolated release CLI builds from test artifacts.

## 0.1.11 - 2026-05-11

- Polished GUI and shell preview behavior.
- Added live preview quality presets and native folder selection in the GUI.
- Improved shell thumbnail overlap, spacing, and preview behavior.

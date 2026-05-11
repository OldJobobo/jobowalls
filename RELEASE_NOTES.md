# Release Notes

## Current Release: 0.1.14

This release adds explicit project versioning and improves GUI theme
integration.

### Highlights

- `VERSION` is now the canonical project version file.
- Root Cargo, GUI Cargo, npm package metadata, and Tauri config are checked for
  version drift.
- Cargo builds now fail when package metadata and `VERSION` disagree.
- GUI startup defaults can now be configured under `[gui]`.
- Shell picker startup defaults can now be configured under `[shell]`.
- GUI controls now respect Omarchy theme colors from the active theme palette.
- Release tarballs include `VERSION`, `CHANGELOG.md`, and `RELEASE_NOTES.md`
  under `share/doc/jobowalls`.

### Upgrade

Install the latest release with the curl installer from the README, or set
`INSTALL_VERSION=v0.1.14` to pin this release.

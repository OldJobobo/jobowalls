# Release Notes

## Current Release: 0.1.15

This release focuses on the shell picker rendering path, especially carousel
animation, selection borders, and vertical placement.

### Highlights

- Shell picker supports vertical layout on the left and right edges.
- `--layout` and `[shell].layout` select horizontal or vertical picker
  orientation.
- `Ctrl+P` cycles the picker through bottom, left, top, and right placements.
- The shell panel now uses focused layer-shell anchors and margins for
  placement instead of making the panel fullscreen.
- Rapid held-key navigation snaps repeated same-direction carousel movement to
  avoid bounce after the key repeat burst stops.
- Selection borders render as a fixed overlay, reducing card size shifts while
  thumbnails load.
- Live desktop preview requests are debounced more conservatively during
  navigation to reduce preview churn.

### Upgrade

Install the latest release with the curl installer from the README, or set
`INSTALL_VERSION=v0.1.15` to pin this release.

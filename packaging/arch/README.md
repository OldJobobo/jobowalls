# Arch Packaging

This directory is a starter AUR packaging skeleton.

For an eventual AUR submission, copy `PKGBUILD` into a clean AUR package repo,
update `url`, maintainer metadata, and regenerate `.SRCINFO`:

```bash
makepkg --printsrcinfo > .SRCINFO
```

The package builds both binaries:

- `/usr/bin/jobowalls`
- `/usr/bin/jobowalls-gui`

Runtime wallpaper backend integrations remain optional dependencies because
`jobowalls` can report availability and choose configured backends at runtime.


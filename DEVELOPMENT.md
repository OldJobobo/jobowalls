# Development

Developer-facing setup, validation, and packaging notes for `jobowalls`.

## Commands

Run CLI and library tests:

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

Run the shell picker in a normal debug window:

```bash
cargo run --bin jobowalls-shell -- --debug-window ~/Pictures/wallpapers
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

## Release Validation

Useful checks before tagging a release:

```bash
bash scripts/check-version.sh
git diff --check
cargo test --locked
npm --prefix gui run typecheck
npm --prefix gui run build
cargo test --locked --manifest-path gui/src-tauri/Cargo.toml
cargo check --release --locked --manifest-path gui/src-tauri/Cargo.toml
cargo build --release --locked --target-dir target/release-build
npm --prefix gui run tauri:build -- --no-bundle
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

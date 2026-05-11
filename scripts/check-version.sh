#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
expected="$(tr -d '[:space:]' <"$root/VERSION")"

if [[ ! "$expected" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "VERSION must contain a semantic version like 0.1.13; got '$expected'" >&2
  exit 1
fi

root_cargo="$(
  cargo metadata --no-deps --format-version 1 --manifest-path "$root/Cargo.toml" |
    node -e 'let raw=""; process.stdin.on("data", c => raw += c); process.stdin.on("end", () => console.log(JSON.parse(raw).packages[0].version));'
)"
gui_cargo="$(
  cargo metadata --no-deps --format-version 1 --manifest-path "$root/gui/src-tauri/Cargo.toml" |
    node -e 'let raw=""; process.stdin.on("data", c => raw += c); process.stdin.on("end", () => console.log(JSON.parse(raw).packages[0].version));'
)"
gui_package="$(
  node -e 'console.log(require(process.argv[1]).version)' "$root/gui/package.json"
)"
tauri_config="$(
  node -e 'console.log(require(process.argv[1]).version)' "$root/gui/src-tauri/tauri.conf.json"
)"

check_version() {
  local label="$1"
  local actual="$2"

  if [[ "$actual" != "$expected" ]]; then
    echo "$label version mismatch: expected $expected, got $actual" >&2
    exit 1
  fi
}

check_version "Cargo.toml" "$root_cargo"
check_version "gui/src-tauri/Cargo.toml" "$gui_cargo"
check_version "gui/package.json" "$gui_package"
check_version "gui/src-tauri/tauri.conf.json" "$tauri_config"

echo "version metadata OK: $expected"

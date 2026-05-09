#!/usr/bin/env bash
set -euo pipefail

PREFIX="${PREFIX:-"$HOME/.local"}"
BINDIR="${BINDIR:-"$PREFIX/bin"}"
APPDIR="${APPDIR:-"$PREFIX/share/applications"}"

usage() {
  cat <<EOF
Usage: ./uninstall.sh

Or:
  curl -fsSL https://raw.githubusercontent.com/OldJobobo/jobowalls/refs/heads/master/uninstall.sh | bash

Environment:
  PREFIX   Install prefix. Default: $HOME/.local
  BINDIR   Binary directory. Default: \$PREFIX/bin
  APPDIR   Desktop entry directory. Default: \$PREFIX/share/applications

Removes:
  \$BINDIR/jobowalls
  \$BINDIR/jobowalls-gui
  \$APPDIR/dev.jobowalls.picker.desktop
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

removed=0
for path in \
  "$BINDIR/jobowalls" \
  "$BINDIR/jobowalls-gui" \
  "$APPDIR/dev.jobowalls.picker.desktop"
do
  if [[ -e "$path" || -L "$path" ]]; then
    rm -f "$path"
    echo "removed $path"
    removed=1
  fi
done

if [[ "$removed" -eq 0 ]]; then
  echo "nothing to remove"
fi

#!/usr/bin/env bash
# ensure-pinned-bun.sh — Downloads and caches the pinned bun version from scripts/const.py.
# Prints the path to the binary. Used by dev scripts to match the shipped runtime.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PINNED_VERSION=$(grep '^BUN_VERSION' "$REPO_ROOT/scripts/const.py" | head -1 | sed 's/.*"\(.*\)".*/\1/')

# Cache in .bun-pinned/<version>/bun
CACHE_DIR="$REPO_ROOT/.bun-pinned/$PINNED_VERSION"
BUN_BIN="$CACHE_DIR/bun"

if [ -x "$BUN_BIN" ] && [ "$("$BUN_BIN" --version 2>/dev/null)" = "$PINNED_VERSION" ]; then
  echo "$BUN_BIN"
  exit 0
fi

# Determine platform and arch
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
  x86_64)  ARCH="x64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
esac

ZIPNAME="bun-${OS}-${ARCH}.zip"
URL="https://github.com/oven-sh/bun/releases/download/bun-v${PINNED_VERSION}/${ZIPNAME}"

mkdir -p "$CACHE_DIR"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading bun v${PINNED_VERSION} (${OS}-${ARCH})..." >&2
curl -sL "$URL" -o "$TMPDIR/$ZIPNAME"
unzip -qo "$TMPDIR/$ZIPNAME" -d "$TMPDIR"
cp "$TMPDIR/bun-${OS}-${ARCH}/bun" "$BUN_BIN"
chmod +x "$BUN_BIN"

echo "Cached bun v${PINNED_VERSION} at $BUN_BIN" >&2
echo "$BUN_BIN"

#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <bun-version>" >&2
  echo "Example: $0 1.3.10" >&2
  exit 1
fi

VERSION="$1"
TMPDIR=$(mktemp -d)

cleanup() {
  rm -rf "$TMPDIR"
}
trap cleanup EXIT

for arch in x64 aarch64; do
  for os in darwin linux; do
    fname="bun-${os}-${arch}.zip"
    curl -sL "https://github.com/oven-sh/bun/releases/download/bun-v${VERSION}/${fname}" -o "${TMPDIR}/${fname}"
    echo "${fname}=$(shasum -a 256 "${TMPDIR}/${fname}" | cut -d' ' -f1)"
  done
done

fname="bun-windows-x64.zip"
curl -sL "https://github.com/oven-sh/bun/releases/download/bun-v${VERSION}/${fname}" -o "${TMPDIR}/${fname}"
echo "${fname}=$(shasum -a 256 "${TMPDIR}/${fname}" | cut -d' ' -f1)"

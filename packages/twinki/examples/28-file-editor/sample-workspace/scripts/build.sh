#!/usr/bin/env bash
set -euo pipefail

# Minimal "build": copy sources into dist/.
echo "Building sample app..."
mkdir -p dist
cp -r src/* dist/
echo "Done -> dist/"

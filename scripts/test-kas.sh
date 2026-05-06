#!/bin/bash
# Test KAS agent engine end-to-end.
# Usage: ./scripts/test-kas.sh [--skip-rust-build]
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Launching KAS via TUI dev mode ==="
cd packages/tui
KIRO_AGENT_ENGINE=kas exec bun run dev "${@:---skip-rust-build}"

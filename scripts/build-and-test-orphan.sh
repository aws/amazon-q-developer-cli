#!/bin/bash
# Build Rust + TUI, then run orphan fix tests
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)

echo "=== Building TUI ==="
cd packages/tui && bun run build && cd "$ROOT"

echo "=== Building Rust (debug) ==="
cargo build -p chat_cli_v2

echo "=== Running orphan fix tests ==="
export KIRO_TEST_TUI_JS_PATH="packages/tui/dist/tui.js"
bash scripts/test-orphan-fix.sh ./target/debug/chat_cli

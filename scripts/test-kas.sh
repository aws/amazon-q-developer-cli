#!/bin/bash
# Test KAS agent engine end-to-end.
# Usage: ./scripts/test-kas.sh
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== 1/4 CodeArtifact auth ==="
if ! bun install --dry-run 2>&1 | grep -q "@kiro/agent"; then
  echo "@kiro/agent already installed, skipping auth"
else
  ./scripts/codeartifact-login.sh
fi

echo "=== 2/4 Install dependencies ==="
bun install

echo "=== 3/4 Build TUI ==="
cd packages/tui && bun run build && cd ../..

echo "=== 4/4 Launch KAS ==="
KIRO_TEST_TUI_JS_PATH=$(pwd)/packages/tui/dist/tui.js \
  cargo run -p chat_cli -- chat --agent-engine=kas "$@"

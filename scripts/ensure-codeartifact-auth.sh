#!/bin/bash
# Ensures CodeArtifact auth is configured before bun install.
# Skips if .npmrc already has a non-expired token.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$SCRIPT_DIR/.."
NPMRC="$REPO_ROOT/.npmrc"

# Skip if no @kiro packages in any workspace package.json
if ! grep -rq '"@kiro/' "$REPO_ROOT"/packages/*/package.json 2>/dev/null; then
  exit 0
fi

# Check if token exists and is not expired
if [ -f "$NPMRC" ] && grep -q "_authToken=" "$NPMRC" 2>/dev/null; then
  TOKEN=$(grep '_authToken=' "$NPMRC" | sed 's/.*_authToken=//')
  EXP=$(echo "$TOKEN" | cut -d. -f1 | base64 -D 2>/dev/null | grep -o '"exp":[0-9]*' | cut -d: -f2)
  NOW=$(date +%s)
  if [ -n "$EXP" ] && [ "$EXP" -gt "$NOW" ] 2>/dev/null; then
    exit 0
  fi
fi

echo "CodeArtifact auth required for @kiro packages. Running codeartifact-login.sh..."
"$SCRIPT_DIR/codeartifact-login.sh"

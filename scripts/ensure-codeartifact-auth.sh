#!/bin/bash
# Ensures CodeArtifact auth is configured before bun install.
# Wired up as the root package.json `preinstall` hook.
#
# Behavior:
# - No-op if no workspace package depends on @kiro/* packages.
# - No-op if .npmrc already has a non-expired token (CI composite action
#   writes this before `bun install`, so CI is a no-op fast path).
# - In CI without a valid token: fails with a clear message pointing at
#   the codeartifact-login action, since we can't drive AWS OIDC from bash.
# - Locally without a valid token: runs codeartifact-login.sh (uses `ada`),
#   or fails with a helpful message if `ada` isn't installed.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$SCRIPT_DIR/.."
NPMRC="$REPO_ROOT/.npmrc"

# Skip if no @kiro packages in any workspace package.json
if ! grep -rq '"@kiro/' "$REPO_ROOT"/packages/*/package.json 2>/dev/null; then
  exit 0
fi

# Skip if .npmrc already has a non-expired token
if [ -f "$NPMRC" ] && grep -q "_authToken=" "$NPMRC" 2>/dev/null; then
  TOKEN=$(grep '_authToken=' "$NPMRC" | sed 's/.*_authToken=//')
  EXP=$(echo "$TOKEN" | cut -d. -f1 | base64 -D 2>/dev/null | grep -o '"exp":[0-9]*' | cut -d: -f2)
  NOW=$(date +%s)
  if [ -n "$EXP" ] && [ "$EXP" -gt "$NOW" ] 2>/dev/null; then
    exit 0
  fi
fi

# In CI, the codeartifact-login composite action should have run already.
# We don't drive AWS OIDC from bash - require the action to have set up .npmrc.
if [ -n "${CI:-}" ] || [ -n "${GITHUB_ACTIONS:-}" ]; then
  echo "Error: CodeArtifact token missing or expired in CI." >&2
  echo "       Add '- uses: ./.github/actions/codeartifact-login' before 'bun install'." >&2
  exit 1
fi

# Local dev path
if ! command -v ada >/dev/null 2>&1; then
  echo "Error: 'ada' CLI not found. Required for CodeArtifact auth to fetch @kiro packages." >&2
  echo "       Install from https://w.amazon.com/bin/view/DevAccount/Ada" >&2
  exit 1
fi

echo "CodeArtifact auth required for @kiro packages. Running codeartifact-login.sh..."
"$SCRIPT_DIR/codeartifact-login.sh"

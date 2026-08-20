#!/usr/bin/env bash
# Regression tests for the KAS freshness warning hooks. Builds a scratch repo
# and verifies the check warns exactly when an installed @kiro/* package
# diverges from an exact pin, and never fails the git operation.
set -eu

REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)
SCRATCH=$(mktemp -d)
trap 'rm -rf "$SCRATCH"' EXIT

git -C "$SCRATCH" init -q
mkdir -p "$SCRATCH/.githooks" "$SCRATCH/packages/tui"
cp "$REPO_ROOT/.githooks/check-kas-freshness" "$SCRATCH/.githooks/"
cp "$REPO_ROOT/.githooks/post-merge" "$SCRATCH/.githooks/"
cp "$REPO_ROOT/.githooks/post-checkout" "$SCRATCH/.githooks/"

failures=0

write_pin() {
  cat > "$SCRATCH/packages/tui/package.json" <<EOF
{
  "name": "@kiro/tui",
  "dependencies": {
    "@kiro/acp-type-covenant": "$1",
    "@kiro/client": "$1"
  },
  "devDependencies": {
    "@kiro/agent": "$1"
  }
}
EOF
}

write_installed() {
  local name="$1" version="$2"
  mkdir -p "$SCRATCH/packages/tui/node_modules/@kiro/$name"
  printf '{\n  "name": "@kiro/%s",\n  "version": "%s"\n}\n' "$name" "$version" \
    > "$SCRATCH/packages/tui/node_modules/@kiro/$name/package.json"
}

check() {
  local name="$1" expect_warning="$2"
  shift 2
  local output status=0
  output=$(cd "$SCRATCH" && "$@" 2>&1) || status=$?
  local warned=false
  case "$output" in *"Stale KAS install"*) warned=true ;; esac
  if [ "$status" -ne 0 ]; then
    echo "✗ $name: expected exit 0, got $status"
    failures=$((failures + 1))
  elif [ "$warned" != "$expect_warning" ]; then
    echo "✗ $name: expected warning=$expect_warning, output was: $output"
    failures=$((failures + 1))
  else
    echo "✓ $name"
  fi
}

# 1. No node_modules at all: silent (fresh worktree).
write_pin "1.0.0"
check "silent without any install" false ./.githooks/check-kas-freshness

# 2. All three installed and matching: silent.
write_installed agent 1.0.0
write_installed client 1.0.0
write_installed acp-type-covenant 1.0.0
check "silent when installs match pins" false ./.githooks/check-kas-freshness

# 3. One package stale: warns, names it.
write_installed agent 0.9.0
check "warns when one package is stale" true ./.githooks/check-kas-freshness
output=$(cd "$SCRATCH" && ./.githooks/check-kas-freshness)
case "$output" in
  *"@kiro/agent: installed 0.9.0, pinned 1.0.0"*) echo "✓ warning names the stale package" ;;
  *) echo "✗ warning does not name the stale package: $output"; failures=$((failures + 1)) ;;
esac

# 4. Range pin: silent even on divergence (exact pins only).
write_pin "^1.0.0"
check "silent for range pins" false ./.githooks/check-kas-freshness
write_pin "1.0.0"

# 5. post-merge wrapper propagates the warning.
check "post-merge warns when stale" true ./.githooks/post-merge

# 6. post-checkout: file checkout (flag 0) is silent, branch checkout warns.
check "post-checkout flag 0 is silent" false ./.githooks/post-checkout a b 0
check "post-checkout flag 1 warns" true ./.githooks/post-checkout a b 1

if [ "$failures" -gt 0 ]; then
  echo "$failures test(s) failed"
  exit 1
fi
echo "All KAS freshness hook tests passed"

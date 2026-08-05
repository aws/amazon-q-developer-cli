#!/usr/bin/env bash
# Regression tests for the pre-commit hook's changelog fragment gate.
# Builds a scratch repo and verifies the hook judges STAGED bytes, not the
# working tree, in both partial-staging directions.
set -eu

REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)
SCRATCH=$(mktemp -d)
trap 'rm -rf "$SCRATCH"' EXIT

git -C "$SCRATCH" init -q
git -C "$SCRATCH" config user.email test@example.com
git -C "$SCRATCH" config user.name test
mkdir -p "$SCRATCH/.githooks" "$SCRATCH/scripts" "$SCRATCH/.changes"
cp "$REPO_ROOT/.githooks/pre-commit" "$SCRATCH/.githooks/"
cp "$REPO_ROOT/scripts/lint-changelog.sh" "$SCRATCH/scripts/"
git -C "$SCRATCH" config core.hooksPath .githooks

VALID='{"type":"fixed","description":"Restored the visible cursor in the message composer"}'
INVALID='{"type":"fixed","description":"the steer/respond thing"}'

failures=0

check() {
  local name="$1" expected="$2"
  if git -C "$SCRATCH" commit -q -m "$name" >/dev/null 2>&1; then
    actual=pass
  else
    actual=fail
  fi
  # Every case starts from the seed commit with a clean tree.
  git -C "$SCRATCH" reset -q --hard "$SEED" >/dev/null 2>&1
  git -C "$SCRATCH" clean -qfd >/dev/null 2>&1
  mkdir -p "$SCRATCH/.changes"
  if [ "$actual" = "$expected" ]; then
    echo "✓ $name"
  else
    echo "✗ $name: expected commit to $expected, it did $actual"
    failures=$((failures + 1))
  fi
}

# Seed an initial commit containing the hook and linter so per-case hard
# resets and cleans never remove them.
touch "$SCRATCH/seed"
git -C "$SCRATCH" add seed .githooks scripts
git -C "$SCRATCH" commit -q -m seed --no-verify
SEED=$(git -C "$SCRATCH" rev-parse HEAD)

# 1. Invalid staged, valid working tree — the commit would contain the
#    invalid bytes, so the hook must block it.
echo "$INVALID" > "$SCRATCH/.changes/frag.json"
git -C "$SCRATCH" add .changes/frag.json
echo "$VALID" > "$SCRATCH/.changes/frag.json"
check "blocks invalid staged content behind a valid working copy" fail

# 2. Valid staged, invalid working tree — the commit is clean, so the
#    hook must not block it.
echo "$VALID" > "$SCRATCH/.changes/frag.json"
git -C "$SCRATCH" add .changes/frag.json
echo "$INVALID" > "$SCRATCH/.changes/frag.json"
check "passes valid staged content behind an invalid working copy" pass

# 3. Plain invalid fragment.
echo "$INVALID" > "$SCRATCH/.changes/frag.json"
git -C "$SCRATCH" add .changes/frag.json
check "blocks an invalid fragment" fail

# 4. Plain valid fragment.
echo "$VALID" > "$SCRATCH/.changes/frag.json"
git -C "$SCRATCH" add .changes/frag.json
check "passes a valid fragment" pass

# 5. No fragments staged.
echo x > "$SCRATCH/other.txt"
git -C "$SCRATCH" add other.txt
check "skips when no fragments are staged" pass

if [ "$failures" -gt 0 ]; then
  echo "$failures case(s) failed"
  exit 1
fi
echo "All cases passed"

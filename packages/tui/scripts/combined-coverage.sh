#!/usr/bin/env bash
set -euo pipefail

# Combined coverage: merge bun (tui) + vitest (tui) + vitest (twinki) lcov reports.
# Exclusion patterns come from coverage-config.json (single source of truth).
# Per-runner lcovFilters are applied individually before merging.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TUI_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$TUI_DIR/../.." && pwd)"
TWINKI_DIR="$REPO_ROOT/packages/twinki"
CONFIG="$TUI_DIR/coverage-config.json"
OUT_DIR="$TUI_DIR/coverage/combined"

mkdir -p "$OUT_DIR"

# --- Build exclude regex from config (global safety net) ---
# excludePatterns: used as-is
_PAT_RE=$(jq -r '[.excludePatterns[].pattern] | join("|")' "$CONFIG")

# excludeDirectories: INTENTIONALLY hardcoded as a subset of the 10 entries in
# coverage-config.json's excludeDirectories.
#
# coverage-config.json lists 10 directories. Of those:
#   - 4 (hooks, components, contexts, twinki) are NOT globally excluded here
#     because they ARE included in vitest coverage. They are only stripped from
#     bun's lcov via per-runner lcovFilters.bun above -- vitest legitimately
#     covers code in those directories.
#   - The remaining 6 below are truly non-production directories that should
#     never appear in ANY coverage report (build output, vendored deps, test
#     scaffolding).
#
# MAINTENANCE: if a new directory is added to excludeDirectories in the config
# and it should be excluded from ALL runners (not just bun), add it here too.
_GLOBAL_DIRS="dist|node_modules|test-utils|e2e_tests|storybook|__tests__"
_DIR_RE="(^|/)($_GLOBAL_DIRS)/"

# excludeFiles: leaf filenames
_FILE_RE=$(jq -r '[.excludeFiles[].name] | map("(^|/)" + . + "$") | join("|")' "$CONFIG")

# Combine all three into one regex
EXCLUDE_RE="$_PAT_RE|$_DIR_RE|$_FILE_RE"

# --- Build per-runner filter regexes from lcovFilters ---
BUN_FILTER_RE=$(jq -r '.lcovFilters.bun | join("|")' "$CONFIG")
TUI_VITEST_FILTER_RE=$(jq -r '.lcovFilters.tuiVitest | join("|")' "$CONFIG")
TWINKI_VITEST_FILTER_RE=$(jq -r '.lcovFilters.twinkiVitest | join("|")' "$CONFIG")

# --- filter_lcov: remove SF records matching a regex from an lcov stream ---
# Usage: filter_lcov <lcov_file> <filter_regex>
# Reads the lcov file, and for each SF record checks if the path matches
# the filter regex. If it matches, the entire record (SF through end_of_record)
# is skipped. Otherwise it is passed through.
filter_lcov() {
  local lcov_file="$1"
  local filter_re="$2"

  if [[ -z "$filter_re" ]]; then
    cat "$lcov_file"
    return
  fi

  awk -v filter="$filter_re" '
    /^SF:/ {
      sf_line = $0
      path = substr($0, 4)
      if (path ~ filter) {
        skip = 1
      } else {
        skip = 0
        print sf_line
      }
      next
    }
    /^end_of_record/ {
      if (!skip) print
      skip = 0
      next
    }
    !skip { print }
  ' "$lcov_file"
}

# --- Run test suites ---
echo "=== Running bun tests (tui) ==="
cd "$TUI_DIR"
bun test --coverage 2>&1 | tail -5 || true

echo ""
echo "=== Running vitest (twinki) ==="
cd "$TWINKI_DIR"
npx vitest run --coverage --coverage.enabled \
  --coverage.reporter=lcov \
  --coverage.reportsDirectory="$TWINKI_DIR/coverage" 2>&1 | tail -5 || true

# If tui vitest config exists, run it too
if [[ -f "$TUI_DIR/vitest.config.ts" ]]; then
  echo ""
  echo "=== Running vitest (tui) ==="
  cd "$TUI_DIR"
  npx vitest run --config vitest.config.ts --coverage 2>&1 | tail -5 || true
fi

# --- Collect and filter lcov sources ---
BUN_LCOV="$TUI_DIR/coverage/lcov.info"
TUI_VITEST_LCOV="$TUI_DIR/coverage/vitest/lcov.info"
TWINKI_VITEST_LCOV="$TWINKI_DIR/coverage/lcov.info"

FILTERED_STREAMS=()
TMPDIR_FILTER=$(mktemp -d)
trap 'rm -rf "$TMPDIR_FILTER"' EXIT

if [[ -f "$BUN_LCOV" ]]; then
  echo "Found: $BUN_LCOV"
  filter_lcov "$BUN_LCOV" "$BUN_FILTER_RE" > "$TMPDIR_FILTER/bun.lcov"
  FILTERED_STREAMS+=("$TMPDIR_FILTER/bun.lcov")
else
  echo "Warning: missing $BUN_LCOV"
fi

if [[ -f "$TUI_VITEST_LCOV" ]]; then
  echo "Found: $TUI_VITEST_LCOV"
  filter_lcov "$TUI_VITEST_LCOV" "$TUI_VITEST_FILTER_RE" > "$TMPDIR_FILTER/tui-vitest.lcov"
  FILTERED_STREAMS+=("$TMPDIR_FILTER/tui-vitest.lcov")
else
  echo "Warning: missing $TUI_VITEST_LCOV"
fi

if [[ -f "$TWINKI_VITEST_LCOV" ]]; then
  echo "Found: $TWINKI_VITEST_LCOV"
  filter_lcov "$TWINKI_VITEST_LCOV" "$TWINKI_VITEST_FILTER_RE" > "$TMPDIR_FILTER/twinki-vitest.lcov"
  FILTERED_STREAMS+=("$TMPDIR_FILTER/twinki-vitest.lcov")
else
  echo "Warning: missing $TWINKI_VITEST_LCOV"
fi

if [[ ${#FILTERED_STREAMS[@]} -eq 0 ]]; then
  echo "ERROR: No lcov files found" >&2
  exit 1
fi

# --- Merge with dedup ---
# 1. Apply global excludePatterns as additional safety filter
# 2. Merge overlapping files by taking max hit count per line (DA dedup)
# 3. Recompute LF/LH per file from merged DA lines

awk -v exclude="$EXCLUDE_RE" '
  /^SF:/ {
    sf = substr($0, 4)
    skip = (sf ~ exclude)
    next
  }
  skip { next }
  /^DA:/ {
    line = substr($0, 4)
    comma = index(line, ",")
    lnum = substr(line, 1, comma - 1) + 0
    hits = substr(line, comma + 1) + 0
    key = sf SUBSEP lnum
    if (!(key in data) || hits > data[key]) data[key] = hits
    files[sf] = 1
    next
  }
  /^end_of_record/ { next }

  END {
    for (key in data) {
      split(key, parts, SUBSEP)
      printf "%s\t%d\t%d\n", parts[1], parts[2], data[key]
    }
  }
' "${FILTERED_STREAMS[@]}" | sort -t$'\t' -k1,1 -k2,2n | awk -F'\t' '
  function flush() {
    if (cur == "") return
    printf "SF:%s\n", cur
    for (i = 1; i <= n; i++) {
      printf "DA:%d,%d\n", lnums[i], hits[i]
      lf++
      if (hits[i] > 0) lh++
    }
    printf "LF:%d\nLH:%d\nend_of_record\n", lf, lh
  }
  $1 != cur { flush(); cur = $1; n = 0; lf = 0; lh = 0 }
  { n++; lnums[n] = $2; hits[n] = $3 }
  END { flush() }
' > "$OUT_DIR/lcov.info"

# --- Summary ---
GOAL=$(jq -r '.coverageGoal' "$CONFIG")
awk -v goal="$GOAL" '
  /^LF:/ { total += substr($0, 4) }
  /^LH:/ { hit   += substr($0, 4) }
  END {
    pct = (total > 0) ? hit / total * 100 : 0
    gap = goal - pct
    if (gap < 0) gap = 0
    printf "\n=== Combined Coverage ===\n"
    printf "Lines: %d / %d (%.1f%%)\n", hit, total, pct
    printf "Gap to %d%%: %.1f pp\n", goal, gap
  }
' "$OUT_DIR/lcov.info"

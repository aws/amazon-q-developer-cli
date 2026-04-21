#!/usr/bin/env bash
set -euo pipefail

# Combined coverage: merge bun (tui) + vitest (twinki) lcov reports.
# Exclusion patterns come from coverage-config.json (single source of truth).
# When tui vitest tests are added later, their lcov merges in automatically.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TUI_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$TUI_DIR/../.." && pwd)"
TWINKI_DIR="$REPO_ROOT/packages/twinki"
CONFIG="$TUI_DIR/coverage-config.json"
OUT_DIR="$TUI_DIR/coverage/combined"

mkdir -p "$OUT_DIR"

# --- Build exclude regex from config ---
EXCLUDE_RE=$(jq -r '[.excludePatterns[].pattern] | join("|")' "$CONFIG")

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

# --- Collect lcov sources ---
LCOV_FILES=()
for f in \
  "$TUI_DIR/coverage/lcov.info" \
  "$TWINKI_DIR/coverage/lcov.info" \
  "$TUI_DIR/coverage/vitest/lcov.info" \
; do
  if [[ -f "$f" ]]; then
    LCOV_FILES+=("$f")
    echo "Found: $f"
  fi
done

if [[ ${#LCOV_FILES[@]} -eq 0 ]]; then
  echo "ERROR: No lcov files found" >&2
  exit 1
fi

# --- Filter and merge ---
# 1. Filter each lcov source with the shared exclude patterns
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
' "${LCOV_FILES[@]}" | sort -t$'\t' -k1,1 -k2,2n | awk -F'\t' '
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

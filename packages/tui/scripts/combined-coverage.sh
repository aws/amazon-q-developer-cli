#!/usr/bin/env bash
set -euo pipefail

# Merge three lcov coverage sources into a single combined report.
# Exclusion patterns are defined in coverage-config.json (single source of truth).

cd "$(dirname "$0")/.."

CONFIG="coverage-config.json"
GOAL=$(jq -r '.coverageGoal' "$CONFIG")

BUN_LCOV="coverage/lcov.info"
TUI_VITEST_LCOV="coverage/vitest-lcov.info"
TWINKI_VITEST_LCOV="../../packages/twinki/coverage/lcov.info"

COMBINED="coverage/combined-lcov.info"

mkdir -p coverage

# filter_lcov FILE EXCLUDE_PATTERN
# Outputs all lcov records whose SF: line does NOT match EXCLUDE_PATTERN.
filter_lcov() {
  local file="$1"
  local exclude="$2"

  if [[ ! -f "$file" ]]; then
    echo "Warning: $file not found, skipping" >&2
    return
  fi

  awk -v pat="$exclude" '
    /^TN:/ || /^SF:/ { buf = $0 "\n"; sf = $0; in_record = 1; next }
    in_record {
      buf = buf $0 "\n"
      if ($0 == "end_of_record") {
        in_record = 0
        if (sf !~ pat) {
          printf "%s", buf
        }
        buf = ""
        sf = ""
      }
    }
  ' "$file"
}

# Build awk-compatible regex patterns from JSON config
BUN_EXCLUDE=$(jq -r '.lcovFilters.bun | join("|")' "$CONFIG")
TUI_VITEST_EXCLUDE=$(jq -r '.lcovFilters.tuiVitest | join("|")' "$CONFIG")
TWINKI_VITEST_EXCLUDE=$(jq -r '.lcovFilters.twinkiVitest | join("|")' "$CONFIG")

# Clear the combined file
: > "$COMBINED"

filter_lcov "$BUN_LCOV" "$BUN_EXCLUDE" >> "$COMBINED"
filter_lcov "$TUI_VITEST_LCOV" "$TUI_VITEST_EXCLUDE" >> "$COMBINED"
filter_lcov "$TWINKI_VITEST_LCOV" "$TWINKI_VITEST_EXCLUDE" >> "$COMBINED"

# Compute summary from the combined lcov
awk -v goal="$GOAL" '
  /^LF:/ { total += substr($0, 4) }
  /^LH:/ { hit   += substr($0, 4) }
  END {
    if (total > 0) {
      pct = hit / total * 100
      gap = goal - pct
      if (gap < 0) gap = 0
      printf "Combined coverage: %d/%d lines (%.1f%%) -- gap to %d%%: %.1f pp\n", hit, total, pct, goal, gap
    } else {
      print "No coverage data found in " FILENAME
    }
  }
' "$COMBINED"

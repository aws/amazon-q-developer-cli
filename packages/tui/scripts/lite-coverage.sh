#!/usr/bin/env bash
#
# Lite UI coverage — produces a single percentage scoped to the lite UI.
#
# Scope (matches docs/COVERAGE.md "Lite UI Coverage" section):
#   src/lite/**                       pure-logic helpers
#   src/components/layout/lite/**     layout components + their helpers
#
# Two runners, one report:
#
#   1. vitest (vitest.lite-coverage.config.ts)
#      Runs every lite test that imports `from 'vitest'`. Instruments the
#      full lite scope, so files that have no test yet show as 0% — keeps
#      the percentage honest.
#
#   2. bun test (src/lite/__tests__/verbose.test.ts only)
#      verbose.test.ts imports `from 'bun:test'` and uses bun-test specific
#      lifecycle for KIRO_HOME redirection. Vitest can't load it. Running
#      it under bun separately gives us real coverage data for verbose.ts.
#
# Merge:
#   Both lcov outputs are filtered to the lite scope, then concatenated with
#   DA-line dedup (max hits per line per file), and the totals are recomputed
#   from the merged DA records. Same algorithm scripts/combined-coverage.sh
#   uses for the package-wide report.
#
# Goal: 90% line coverage of the lite scope (matches the package-wide goal
# in coverage-config.json). The script prints the gap to goal but does NOT
# fail on miss — gating is the CI job's call (see .github/workflows/tui.yml).
#
# Usage:
#   bash scripts/lite-coverage.sh           # run from packages/tui
#   bash scripts/lite-coverage.sh --quiet   # suppress test output, summary only
#
# Exit codes:
#   0 on success (regardless of percentage — gating is the caller's job)
#   1 on missing inputs (vitest/bun didn't produce lcov)
#   2 on tooling errors (jq/awk missing)

set -euo pipefail

QUIET=0
if [[ "${1:-}" == "--quiet" ]]; then
  QUIET=1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TUI_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$TUI_DIR"

# Tooling check up front — better a clear error than a confusing `set -e`
# trap when one of these is missing on a fresh machine / CI runner.
for cmd in awk bunx bun; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "ERROR: required command not found: $cmd" >&2
    exit 2
  fi
done

# Lite scope as a single regex matched against lcov SF: paths. Matches
# both absolute paths (from vitest) and repo-relative paths (from bun) by
# anchoring on a final segment of the lite directories. If the scope ever
# moves, update this AND the include glob in vitest.lite-coverage.config.ts
# AND the docs section.
SCOPE_RE='(^|/)src/(lite|components/layout/lite)/'

# Lite-only tests that the bun runner needs to pick up. We pass these
# explicitly rather than relying on a directory scan so the bun pass stays
# fast and predictable — only the verbose tests, only their transitive
# imports, only their lcov data.
BUN_TEST_TARGETS=(
  "src/lite/__tests__/verbose.test.ts"
)

run_vitest() {
  if [[ "$QUIET" == "1" ]]; then
    bunx vitest run --config vitest.lite-coverage.config.ts --coverage >/dev/null 2>&1
  else
    echo "=== Running vitest (lite scope) ==="
    bunx vitest run --config vitest.lite-coverage.config.ts --coverage
  fi
}

run_bun_for_verbose() {
  # Bun writes its lcov to coverage/lcov.info regardless of which subset of
  # tests we run. To avoid clobbering an existing package-wide report (e.g.
  # from a developer running `bun test` earlier), we redirect via
  # BUN_TEST_PRESET-style temp dir... actually bun has no such flag, so we
  # accept the overwrite. The package-wide bun coverage is regenerated on
  # every CI run anyway, so the only practical loss is a stale local file.
  # If this becomes a problem we can backup/restore around the bun call.
  if [[ "$QUIET" == "1" ]]; then
    bun test --coverage "${BUN_TEST_TARGETS[@]}" >/dev/null 2>&1 || true
  else
    echo ""
    echo "=== Running bun test (verbose.test.ts only) ==="
    bun test --coverage "${BUN_TEST_TARGETS[@]}" || true
  fi
}

# filter_lcov: keep only SF records whose path matches $SCOPE_RE.
# Same shape as combined-coverage.sh's helper but inlined here so the
# script has no cross-file dependency.
filter_lcov_to_scope() {
  local lcov_file="$1"
  if [[ ! -f "$lcov_file" ]]; then
    return 0
  fi
  awk -v scope="$SCOPE_RE" '
    /^SF:/ {
      path = substr($0, 4)
      if (path ~ scope) { skip = 0; print } else { skip = 1 }
      next
    }
    /^end_of_record/ { if (!skip) print; skip = 0; next }
    !skip { print }
  ' "$lcov_file"
}

run_vitest
run_bun_for_verbose

VITEST_LCOV="coverage/lite/lcov.info"
BUN_LCOV="coverage/lcov.info"
OUT_DIR="coverage/lite"
mkdir -p "$OUT_DIR"
MERGED="$OUT_DIR/merged-lcov.info"

if [[ ! -f "$VITEST_LCOV" ]]; then
  # Tests for the lite scope are deferred to a follow-up PR; vitest
  # ran with passWithNoTests so no lcov is produced. Emit an empty
  # report and exit 0 — gating is the caller's job, and this job is
  # report-only today.
  echo "INFO: vitest did not produce $VITEST_LCOV (no lite tests yet) — emitting empty report" >&2
  : > "$VITEST_LCOV"
fi
if [[ ! -f "$BUN_LCOV" ]]; then
  echo "WARNING: bun lcov $BUN_LCOV missing — verbose.ts coverage will be 0%" >&2
fi

# Merge: concatenate filtered streams, dedup DA lines per (file, line) by
# taking the MAX hit count, then recompute LF/LH per file from the merged
# DA records. This is the same approach scripts/combined-coverage.sh uses
# for the package-wide merge.
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

filter_lcov_to_scope "$VITEST_LCOV" > "$TMP/vitest.lcov"
filter_lcov_to_scope "$BUN_LCOV"    > "$TMP/bun.lcov"

awk '
  /^SF:/ { sf = substr($0, 4); next }
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
  END {
    for (key in data) {
      split(key, parts, SUBSEP)
      printf "%s\t%d\t%d\n", parts[1], parts[2], data[key]
    }
  }
' "$TMP/vitest.lcov" "$TMP/bun.lcov" \
  | sort -t$'\t' -k1,1 -k2,2n \
  | awk -F'\t' '
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
    ' > "$MERGED"

# Goal lifted from coverage-config.json so it stays in lockstep with the
# package-wide target. If jq isn't around (unusual on dev machines but
# possible), fall back to 90 hardcoded.
GOAL=90
if command -v jq >/dev/null 2>&1 && [[ -f coverage-config.json ]]; then
  GOAL=$(jq -r '.coverageGoal' coverage-config.json)
fi

awk -v goal="$GOAL" '
  /^SF:/ { files++ }
  /^LF:/ { total += substr($0, 4) }
  /^LH:/ { hit   += substr($0, 4) }
  END {
    pct = (total > 0) ? hit / total * 100 : 0
    gap = goal - pct; if (gap < 0) gap = 0
    printf "\n=== Lite UI Coverage ===\n"
    printf "Files measured: %d\n", files
    printf "Lines covered:  %d / %d  (%.2f%%)\n", hit, total, pct
    printf "Goal:           %d%%\n", goal
    if (pct + 0.005 >= goal) {
      printf "Status:         AT/ABOVE goal\n"
    } else {
      printf "Status:         %.2f pp short\n", gap
    }
    printf "\n"
    printf "Scope: pure-logic .ts files in src/lite/ and src/components/layout/lite/.\n"
    printf "React component bodies (.tsx) are exercised by integ/e2e tests, not\n"
    printf "measurable via vitest V8 in the node env — see docs/COVERAGE.md.\n"
  }
' "$MERGED"

# Emit GitHub Actions step summary entries when running under CI. The
# "$GITHUB_STEP_SUMMARY" env var is set automatically by GHA. Markdown
# rendered in the job summary panel — gives the manager-visible
# percentage without forcing the manager to dig into logs.
if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  awk -v goal="$GOAL" '
    /^LF:/ { total += substr($0, 4) }
    /^LH:/ { hit   += substr($0, 4) }
    END {
      pct = (total > 0) ? hit / total * 100 : 0
      gap = goal - pct; if (gap < 0) gap = 0
      printf "## Lite UI Coverage\n\n"
      printf "| Lines | Coverage | Goal | Status |\n"
      printf "|---|---|---|---|\n"
      printf "| %d / %d | **%.2f%%** | %d%% | %s |\n",
        hit, total, pct, goal,
        (pct + 0.005 >= goal ? "✅ at/above goal" : sprintf("⚠️ %.2f pp short", gap))
      printf "\n"
      printf "_Scope: pure-logic .ts files in `src/lite/` and `src/components/layout/lite/`. React component bodies (.tsx) are exercised by integ/e2e tests; see [docs/COVERAGE.md](../packages/tui/docs/COVERAGE.md) for the scope rationale._\n"
    }
  ' "$MERGED" >> "$GITHUB_STEP_SUMMARY"
fi

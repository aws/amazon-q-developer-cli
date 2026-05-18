#!/usr/bin/env bash
#
# find-dedup-candidates.sh — structural pre-filter for the review-dedup hat.
#
# Given a source-file path (the value of a finding's `file:` frontmatter
# field), print the filenames of every finding in
# docs/review-playbook/runner/findings/ whose own `file:` frontmatter
# matches — exactly or by path prefix.
#
# The dedup hat uses this to narrow the candidate set before reading
# finding bodies, keeping its context bounded no matter how many
# findings accumulate in the committed tree.
#
# Contract:
#   - Exit 0 always (zero matches is a valid answer, meaning "no existing
#     findings on this file → the decision is COPY").
#   - Exit 2 only on usage error (missing arg).
#   - Read-only. Never mutates findings/ or anything else.
#   - Portable across macOS/BSD and GNU coreutils.
#   - Gracefully handles a missing findings/ directory (prints nothing,
#     exits 0) — dedup can still run on a brand-new repo.
#   - Matches are by literal path equality against the frontmatter
#     `file:` field. A trailing `:<line>` in the frontmatter is tolerated
#     (older findings use `file: path:line`).
#
# Usage:
#   docs/review-playbook/runner/scripts/find-dedup-candidates.sh \
#     packages/tui/src/stores/session-conversations.ts
#
# Output:
#   One filename per line (no paths), relative to findings/:
#     03-trim-caps-code-20260505-0010-caps-missing-env-overrides.md
#     03-trim-caps-code-20260505-0011-session-growth.md
#
# Empty output = zero candidates = dedup decision is COPY.

set -euo pipefail

FINDINGS_DIR="docs/review-playbook/runner/findings"

if [[ $# -ne 1 || -z "${1:-}" ]]; then
  echo "usage: $0 <source-file-path>" >&2
  echo "  e.g. $0 packages/tui/src/stores/session-conversations.ts" >&2
  exit 2
fi

TARGET="$1"

# If the findings directory does not exist, there cannot be any
# candidates — this is a valid "no matches" answer, exit 0.
if [[ ! -d "$FINDINGS_DIR" ]]; then
  exit 0
fi

# Build the expected frontmatter prefix. Findings use:
#   file: <path>
# or (older form)
#   file: <path>:<line>
# Match both with a single regex anchored on the path.
# Use word-boundary-adjacent match: end of line, or `:` followed by digits.
#
# Escape regex metacharacters in TARGET. The practical set we need to
# worry about is `.`, `/`, `+`, `-`, `*`, `(`, `)`, `[`, `]`, `?`,
# `^`, `$`, `|`, `\`. Paths usually only contain `.` and `/` but be
# defensive.
escape_regex() {
  # shellcheck disable=SC2001
  echo "$1" | sed 's/[][\\.^$*+?()|{}\/]/\\&/g'
}

ESC=$(escape_regex "$TARGET")
# Match: start of line, optional whitespace, `file:`, whitespace, TARGET,
# followed by either end-of-line or `:<digits>` (line number).
PATTERN="^[[:space:]]*file:[[:space:]]*${ESC}(:[0-9]+)?[[:space:]]*$"

# Walk every non-marker, non-sidecar finding in the committed tree and
# grep the frontmatter for a `file:` match.
# Marker files: *-done.md, *-continuation.md, *-runbook.md.
# Sidecars: _*.md (underscore-prefixed).
# We scan only the first ~40 lines of each file (frontmatter lives at the
# top; avoids reading body-sized evidence blocks).

shopt -s nullglob
for f in "$FINDINGS_DIR"/*.md; do
  name=$(basename "$f")
  case "$name" in
    _*)               continue ;;  # sidecar
    *-done.md)        continue ;;
    *-continuation.md) continue ;;
    *-runbook.md)     continue ;;
  esac

  # Scan frontmatter only. Use awk to stop at the second `---`.
  if awk '
    BEGIN { in_fm = 0; seen = 0 }
    /^---[[:space:]]*$/ {
      if (!in_fm && !seen) { in_fm = 1; seen = 1; next }
      if (in_fm)           { exit }
    }
    in_fm { print }
  ' "$f" | grep -E -q "$PATTERN"; then
    echo "$name"
  fi
done

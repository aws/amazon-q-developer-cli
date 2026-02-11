#!/usr/bin/env bash
# tmux-based wrapping tests for TUI prompt input.
# Sends keystrokes char-by-char, polls for readiness, verifies text integrity.
# Usage: scripts/test-wrap-v2.sh [binary] [--v1]
#   --v1  Use V1 prompt detection ([kiro-dev] > ) instead of V2 placeholder
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

BINARY="${1:-$PWD/target/debug/chat_cli_v2}"
V1=false
[[ "${2:-}" == "--v1" || "${1:-}" == "--v1" ]] && V1=true
[[ "$1" == "--v1" ]] && BINARY="$PWD/target/debug/chat_cli_v2"

SESSION="wrap-test-$$"
PASS=0
FAIL=0
ERRORS=""

if [[ "$V1" == true ]]; then
  READY_PATTERN="> "
  LAUNCH="$BINARY chat"
else
  READY_PATTERN="ask a question"
  LAUNCH="$BINARY chat --agent kiro-dev-v2"
fi

[[ -x "$BINARY" ]] || { echo "Binary not found: $BINARY"; exit 1; }
echo "Testing: $BINARY"
echo ""

# --- helpers ---

cleanup() { tmux kill-session -t "$SESSION" 2>/dev/null || true; }
trap cleanup EXIT

start() {
  local w=$1 h=$2
  cleanup
  tmux new-session -d -s "$SESSION" -x "$w" -y "$h"
  tmux resize-window -t "$SESSION" -x "$w" -y "$h"
  tmux send-keys -t "$SESSION" "$LAUNCH" Enter
  poll "$READY_PATTERN" 80 || { echo "TUI failed to start"; return 1; }
  sleep 0.2
}

poll() {
  local pattern=$1 attempts=${2:-80}
  for (( i=0; i<attempts; i++ )); do
    sleep 0.1
    tmux capture-pane -t "$SESSION" -p | grep -qF "$pattern" && return 0
  done
  return 1
}

type_text() {
  local text=$1
  for (( i=0; i<${#text}; i++ )); do
    tmux send-keys -t "$SESSION" -l "${text:$i:1}"
  done
  sleep 0.2
}

clear_input() {
  # Select all and delete
  tmux send-keys -t "$SESSION" C-a
  sleep 0.05
  # Delete forward to end
  tmux send-keys -t "$SESSION" C-k
  sleep 0.1
}

# Capture input lines, strip cursor, join with spaces (word-wrap eats trailing
# space at break point), normalise whitespace.
capture_input() {
  tmux capture-pane -t "$SESSION" -p \
    | sed -n '/'"$1"'/,/'"$2"'/ p' \
    | sed 's/❙//g; s/ *$//' \
    | paste -sd' ' - \
    | sed 's/  */ /g; s/^ //; s/ $//'
}

pass() { echo "  ✅ $1"; PASS=$((PASS + 1)); }
fail() { echo "  ❌ $1"; FAIL=$((FAIL + 1)); ERRORS="$ERRORS\n  - $1"; }

# --- tests ---

INPUT="i am typing a long message and hoping that it doesn't wrap weirdly. but it does not. hmm, actually let me keep going with even more text to really push this past the wrapping point and see"
MS="i am typing"
ME="and see"

# --- Group 1: Static widths (one start per width) ---

echo "=== Static width tests ==="

for w in 99 60 40; do
  start "$w" 30
  type_text "$INPUT"

  # Overflow check
  overflow=0
  while IFS= read -r line; do
    (( ${#line} > w )) && overflow=1
  done < <(tmux capture-pane -t "$SESSION" -p | sed -n '/'"$MS"'/,/'"$ME"'/ p')
  [[ $overflow -eq 0 ]] && pass "no overflow at ${w} cols" || fail "line overflow at ${w} cols"

  # Text integrity
  got=$(capture_input "$MS" "$ME")
  [[ "$got" == "$INPUT" ]] && pass "text intact at ${w} cols" || fail "text mismatch at ${w} cols"

  cleanup
done

# --- Group 2: Resize tests (reuse one session) ---

echo ""
echo "=== Resize tests ==="

start 99 30
type_text "$INPUT"

for target in 60 50 40; do
  tmux resize-window -t "$SESSION" -x "$target" -y 30
  sleep 0.5

  got=$(capture_input "$MS" "$ME")
  [[ "$got" == "$INPUT" ]] && pass "text intact after resize 99→${target}" || fail "text mismatch after resize 99→${target}"
done

# Ghost duplicate check (already at 40 from above, resize back to 60)
tmux resize-window -t "$SESSION" -x 60 -y 30
sleep 0.5
dupes=$(tmux capture-pane -t "$SESSION" -p | sed -n '/'"$MS"'/,/'"$ME"'/ p' | sort | uniq -d | wc -l | tr -d ' ')
[[ "$dupes" -eq 0 ]] && pass "no ghost duplicates after resize" || fail "$dupes ghost duplicate lines"

cleanup

# --- Group 3: Resize sweep ---

echo ""
echo "=== Resize sweep 99→45 (step 3) ==="

start 99 40
type_text "$INPUT"

sweep_fail=0
for w in $(seq 96 -3 45); do
  tmux resize-window -t "$SESSION" -x "$w" -y 40
  sleep 0.4
  got=$(capture_input "$MS" "$ME")
  if [[ "$got" != "$INPUT" ]]; then
    echo "    width $w: MISMATCH"
    sweep_fail=$((sweep_fail + 1))
  fi
done
[[ $sweep_fail -eq 0 ]] && pass "sweep: text intact at all widths" || fail "sweep: $sweep_fail widths had mismatches"

cleanup

# --- Summary ---

echo ""
echo "=== Results ==="
echo "PASS: $PASS  FAIL: $FAIL"
if [[ $FAIL -gt 0 ]]; then
  echo -e "\nFailures:$ERRORS"
  exit 1
fi

#!/bin/bash
# Test orphan bun process fix
set -uo pipefail

BINARY="${1:-./target/debug/chat_cli}"
export PATH="$HOME/.bun/bin:$PATH"
export KIRO_TEST_TUI_JS_PATH="${KIRO_TEST_TUI_JS_PATH:-packages/tui/dist/tui.js}"
PASS=0; FAIL=0

# Snapshot pre-existing bun PIDs so we don't count/kill our own session
EXISTING_PIDS=$(pgrep -f "bun.*tui.js" 2>/dev/null | tr '\n' '|')
EXISTING_PIDS="${EXISTING_PIDS%|}"  # trim trailing pipe

count_bun() {
  local pids n
  pids=$(pgrep -f "bun.*tui.js" 2>/dev/null || true)
  if [ -n "$EXISTING_PIDS" ]; then
    pids=$(echo "$pids" | grep -Ev "^($EXISTING_PIDS)$" || true)
  fi
  n=$(echo "$pids" | grep -c . 2>/dev/null || true)
  printf '%d' "${n:-0}"
}

cleanup() {
  kill "$1" 2>/dev/null || true; sleep 1
  # Only kill bun processes spawned during the test
  for pid in $(pgrep -f "bun.*tui.js" 2>/dev/null); do
    if [ -n "$EXISTING_PIDS" ] && echo "$pid" | grep -qE "^($EXISTING_PIDS)$"; then
      continue
    fi
    kill "$pid" 2>/dev/null || true
  done
  pkill -f "chat_cli.*acp" 2>/dev/null || true
  sleep 2
}

run_test() {
  local SIGNAL="$1" LABEL="$2"
  echo "--- $LABEL ---"

  local fifo="/tmp/kiro-test-fifo-$RANDOM"
  mkfifo "$fifo"
  sleep 999 > "$fifo" &
  local HOLDER=$!

  $BINARY chat --tui < "$fifo" &>/dev/null &
  local PID=$!
  echo "  Kiro PID=$PID"

  local spawned=0
  for i in $(seq 1 25); do
    if [ "$(count_bun)" -gt 0 ]; then spawned=1; echo "  Bun spawned (${i}s)"; break; fi
    sleep 1
  done
  if [ "$spawned" -eq 0 ]; then
    echo "  SKIP: bun never spawned"; FAIL=$((FAIL+1))
    kill $HOLDER 2>/dev/null; rm -f "$fifo"; cleanup $PID; echo ""; return
  fi

  echo "  Sending $SIGNAL to PID=$PID"
  kill -s "$SIGNAL" $PID 2>/dev/null || true

  # For SIGKILL, also close the pipe to simulate parent death
  if [ "$SIGNAL" = "KILL" ]; then
    kill $HOLDER 2>/dev/null || true; rm -f "$fifo"
  fi

  local cleaned=0
  for i in $(seq 1 20); do
    if [ "$(count_bun)" -eq 0 ]; then
      cleaned=1; echo "  ✅ PASS — bun exited (${i}s)"; PASS=$((PASS+1)); break
    fi
    sleep 1
  done
  if [ "$cleaned" -eq 0 ]; then
    echo "  ❌ FAIL — $(count_bun) orphan(s)"; FAIL=$((FAIL+1))
  fi

  kill $HOLDER 2>/dev/null || true; rm -f "$fifo"
  cleanup $PID; echo ""
}

echo "=== Orphan Fix Tests ==="
echo "Binary: $BINARY"
echo "TUI: $KIRO_TEST_TUI_JS_PATH"
echo ""

run_test HUP  "Test 1: SIGHUP (terminal close)"
run_test TERM "Test 2: SIGTERM (graceful kill)"
run_test KILL "Test 3: SIGKILL (parent death + stdin EOF)"

echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ] && echo "✅ All passed" || echo "❌ Some failed"
exit $FAIL

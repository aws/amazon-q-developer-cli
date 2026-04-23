#!/bin/bash
# knight-rider.sh — Safe wrapper for starting/stopping/checking Knight Rider
# Prevents agent from getting stuck with timeout guards on every step.
#
# Usage:
#   ./scripts/knight-rider.sh start                          # use default repo
#   ./scripts/knight-rider.sh start --dir ~/workplace/kiro-cli-pr-1800  # use worktree
#   ./scripts/knight-rider.sh stop
#   ./scripts/knight-rider.sh status
#   ./scripts/knight-rider.sh restart --dir /path/to/checkout

set -euo pipefail

PORT=3001
KR_URL="http://localhost:$PORT"
LOG="/tmp/knight-rider.log"
PID_FILE="/tmp/knight-rider.pid"
BOOT_TIMEOUT=30
KILL_TIMEOUT=10
RUN_TIMEOUT=300

# Parse args: first positional is action, --dir is optional
ACTION="${1:-status}"
shift || true
REPO_ROOT="$HOME/workplace/kiro-cli-review"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dir) REPO_ROOT="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done
TUI_DIR="$REPO_ROOT/packages/tui"

status() {
  if curl -s --max-time 3 "$KR_URL/api/status" 2>/dev/null | grep -q '"ready": *true'; then
    echo "✅ Knight Rider is up and ready on port $PORT"
    curl -s --max-time 3 "$KR_URL/api/status" 2>/dev/null | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'   Frames: {d[\"frameCount\"]}  Dir: {d[\"outputDir\"]}')" 2>/dev/null
    return 0
  fi
  local pids
  pids=$(lsof -ti:$PORT 2>/dev/null || true)
  if [ -n "$pids" ]; then
    echo "⏳ Port $PORT is bound but not ready yet"
    return 1
  fi
  local stuck
  stuck=$(ps -eo pid,etimes,args 2>/dev/null | grep -E "knight-rider|bun.*index\.tsx" | grep -v grep | awk '$2 > 60 {print}' || true)
  if [ -n "$stuck" ]; then
    echo "🚨 STUCK processes detected (running >60s, port not up):"
    echo "$stuck"
    return 2
  fi
  echo "❌ Knight Rider is not running"
  return 3
}

stop() {
  echo "Stopping Knight Rider..."
  if [ -f "$PID_FILE" ]; then
    local pid
    pid=$(cat "$PID_FILE")
    kill "$pid" 2>/dev/null && echo "  Killed PID $pid (from pid file)" || true
    rm -f "$PID_FILE"
  fi
  local pids
  pids=$(timeout "$KILL_TIMEOUT" lsof -ti:$PORT 2>/dev/null || true)
  if [ -n "$pids" ]; then
    echo "$pids" | xargs kill 2>/dev/null && echo "  Killed port $PORT listeners" || true
  fi
  pkill -f "knight-rider.ts" 2>/dev/null || true
  pkill -f "bun.*index\.tsx" 2>/dev/null || true
  sleep 1
  echo "Stopped."
}

start() {
  if [ ! -d "$TUI_DIR" ]; then
    echo "❌ TUI directory not found: $TUI_DIR"
    exit 1
  fi
  if [ ! -f "$REPO_ROOT/target/debug/chat_cli" ]; then
    echo "⚠️  No Rust binary at target/debug/chat_cli — building..."
    (cd "$REPO_ROOT" && timeout 120 cargo build -p chat_cli) || { echo "❌ cargo build failed"; exit 1; }
  fi
  echo "Cleaning up orphans..."
  stop 2>/dev/null || true
  echo "Starting Knight Rider (max ${RUN_TIMEOUT}s lifetime)..."
  echo "  Repo: $REPO_ROOT"
  cd "$TUI_DIR"
  timeout "$RUN_TIMEOUT" bun run knight-rider > "$LOG" 2>&1 &
  local kr_pid=$!
  echo "$kr_pid" > "$PID_FILE"
  echo "  PID: $kr_pid"
  echo "  Log: $LOG"
  echo "Waiting for ready (max ${BOOT_TIMEOUT}s)..."
  for i in $(seq 1 "$BOOT_TIMEOUT"); do
    if curl -s --max-time 2 "$KR_URL/api/status" 2>/dev/null | grep -q '"ready": *true'; then
      echo "✅ Knight Rider is ready — http://localhost:$PORT"
      return 0
    fi
    if ! kill -0 "$kr_pid" 2>/dev/null; then
      echo "❌ Knight Rider process died during boot. Log:"
      tail -20 "$LOG"
      return 1
    fi
    sleep 1
  done
  echo "❌ Timed out waiting for ready after ${BOOT_TIMEOUT}s. Log:"
  tail -20 "$LOG"
  return 1
}

case "$ACTION" in
  start)   start ;;
  stop)    stop ;;
  restart) stop; start ;;
  status)  status ;;
  *) echo "Usage: $0 {start|stop|restart|status} [--dir /path/to/repo]"; exit 1 ;;
esac

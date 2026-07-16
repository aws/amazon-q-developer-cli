#!/bin/bash
# knight-rider.sh — Safe wrapper for starting/stopping/checking Knight Rider
# Prevents agent from getting stuck with timeout guards on every step.
#
# Usage:
#   ./scripts/knight-rider.sh start                          # use default repo
#   ./scripts/knight-rider.sh start --dir ~/workplace/kiro-cli-pr-1800  # use worktree
#   ./scripts/knight-rider.sh start --out /path/to/frames   # custom output dir
#   ./scripts/knight-rider.sh start --kas                    # use KAS engine
#   ./scripts/knight-rider.sh start --port 3002 --kas       # KAS on custom port
#   ./scripts/knight-rider.sh stop --port 3002              # stop specific instance
#   ./scripts/knight-rider.sh status
#   ./scripts/knight-rider.sh restart --dir /path/to/checkout

set -euo pipefail

PORT=3001
KAS=false
BOOT_TIMEOUT=30
KILL_TIMEOUT=10
RUN_TIMEOUT=2700
WINDOWS=false
if [ "${RUNNER_OS:-}" = "Windows" ] || [ "${OS:-}" = "Windows_NT" ]; then
  WINDOWS=true
fi

# Parse args: first positional is action, rest are flags
ACTION="${1:-status}"
shift || true
REPO_ROOT="$HOME/workplace/kiro-cli-review"
if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  REPO_ROOT="$(pwd)"
fi
OUT_DIR="${SMOKE_OUTPUT_DIR:-}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dir) REPO_ROOT="$2"; shift 2 ;;
    --out) OUT_DIR="$2"; shift 2 ;;
    --port) PORT="$2"; shift 2 ;;
    --kas) KAS=true; shift ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done
[ "${SMOKE_ENGINE:-}" = "kas" ] && KAS=true
TUI_DIR="$REPO_ROOT/packages/tui"

# Derive per-port file paths so multiple instances can coexist
KR_URL="http://localhost:$PORT"
LOG="/tmp/knight-rider-${PORT}.log"
PID_FILE="/tmp/knight-rider-${PORT}.pid"

# Portable timeout: use GNU timeout if available, otherwise run without limit
_timeout() {
  local secs="$1"; shift
  if [ "$WINDOWS" = "true" ]; then
    "$@"
  elif command -v timeout &>/dev/null; then
    timeout "$secs" "$@"
  else
    "$@"
  fi
}

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
    if [ "$WINDOWS" = "true" ]; then
      taskkill.exe //PID "$pid" //T //F >/dev/null 2>&1 && echo "  Killed PID $pid (from pid file)" || true
    else
      kill "$pid" 2>/dev/null && echo "  Killed PID $pid (from pid file)" || true
    fi
    rm -f "$PID_FILE"
  fi
  if [ "$WINDOWS" = "true" ]; then
    # Git Bash may report an MSYS PID that taskkill.exe cannot use. Fall back to
    # the native Windows PID listening on Knight Rider's configured port.
    local port_pids
    port_pids=$(powershell.exe -NoProfile -NonInteractive -Command \
      "Get-NetTCPConnection -LocalPort $PORT -State Listen -ErrorAction SilentlyContinue | Select-Object -ExpandProperty OwningProcess -Unique" \
      2>/dev/null | tr -d '\r' || true)
    if [ -n "$port_pids" ]; then
      while IFS= read -r port_pid; do
        [ -z "$port_pid" ] && continue
        taskkill.exe //PID "$port_pid" //T //F >/dev/null 2>&1 && echo "  Killed PID $port_pid (port $PORT listener)" || true
      done <<< "$port_pids"
    fi
    sleep 1
    echo "Stopped."
    return
  fi
  local pids
  pids=$(_timeout "$KILL_TIMEOUT" lsof -ti:$PORT 2>/dev/null || true)
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
  local chat_cli_bin="$REPO_ROOT/target/debug/chat_cli"
  [ "$WINDOWS" = "true" ] && chat_cli_bin="${chat_cli_bin}.exe"
  if [ ! -f "$chat_cli_bin" ]; then
    echo "⚠️  No Rust binary at $chat_cli_bin — building..."
    (cd "$REPO_ROOT" && _timeout 120 cargo build -p chat_cli) || { echo "❌ cargo build failed"; exit 1; }
  fi
  export KIRO_CHAT_CLI_BIN="$chat_cli_bin"
  echo "Cleaning up orphans..."
  stop 2>/dev/null || true
  echo "Starting Knight Rider (max ${RUN_TIMEOUT}s lifetime)..."
  echo "  Repo: $REPO_ROOT"
  cd "$TUI_DIR"
  local kr_args=(knight-rider --port "$PORT")
  if [ -n "$OUT_DIR" ]; then
    kr_args+=(--out "$OUT_DIR")
    echo "  Out: $OUT_DIR"
  fi
  if [ "$KAS" = "true" ]; then
    kr_args+=(--kas)
    echo "  Engine: KAS"
  fi
  if [ "$WINDOWS" = "true" ]; then
    bun run "${kr_args[@]}" > "$LOG" 2>&1 &
  else
    _timeout "$RUN_TIMEOUT" bun run "${kr_args[@]}" > "$LOG" 2>&1 &
  fi
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

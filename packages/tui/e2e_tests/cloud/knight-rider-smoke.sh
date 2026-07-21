#!/usr/bin/env bash
# Cloud-session knight-rider smoke (batch 1) — drives the REAL binary through
# the core cloud-session flows in a live PTY with screenshot evidence.
#
# Two modes:
#   ./knight-rider-smoke.sh mock   — hermetic: mock BFF on a local port (default)
#   ./knight-rider-smoke.sh prod   — against https://app.kiro.dev (needs login)
#
# Prereqs: a chat_cli binary (target/release preferred, falls back to debug)
# with the TUI + KAS bundle embedded, `bun install` done in packages/tui.
#
# Evidence frames land in e2e_tests/test-outputs/knight-rider-<ts>/ and a
# PASS/FAIL summary prints at the end. Exit code 1 on any failure.
set -uo pipefail

MODE="${1:-mock}"
PORT="${KR_PORT:-3021}"
TUI_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
REPO_ROOT="$(cd "$TUI_DIR/../.." && pwd)"
BIN="$REPO_ROOT/target/release/chat_cli"; BIN_PROFILE="release"
[ -x "$BIN" ] || { BIN="$REPO_ROOT/target/debug/chat_cli"; BIN_PROFILE="debug"; }
[ -x "$BIN" ] || { echo "no chat_cli binary; run cargo build first"; exit 1; }

KR="http://localhost:$PORT/api"
FAILS=0
BFF_PID=""
KR_PID=""

say()  { printf '%s\n' "$*"; }
pass() { say "  PASS  $1"; }
fail() { say "  FAIL  $1"; FAILS=$((FAILS+1)); }

# Kill a stale listener on $1 only when it looks like ours (bun/node), so a
# re-run cleans up after a crashed prior run without touching unrelated apps.
kill_stale_listener() {
  local pid
  for pid in $(lsof -ti:"$1" 2>/dev/null); do
    case "$(ps -o comm= -p "$pid" 2>/dev/null)" in
      *bun*|*node*) kill "$pid" 2>/dev/null ;;
      *) say "  WARN  port $1 held by unrelated pid $pid; not killing" ;;
    esac
  done
}

cleanup() {
  [ -n "$KR_PID" ] && { pkill -P "$KR_PID" 2>/dev/null; kill "$KR_PID" 2>/dev/null; }
  [ -n "$BFF_PID" ] && kill "$BFF_PID" 2>/dev/null
}
trap cleanup EXIT INT TERM

type_text() { local m="$1" i c; for (( i=0; i<${#m}; i++ )); do c="${m:$i:1}"; case "$c" in '"') c='\"';; '\\') c='\\\\';; esac; curl -s -X POST "$KR/keys" -d "{\"keys\":\"$c\"}" >/dev/null; sleep 0.03; done; sleep 0.3; }
enter() { curl -s -X POST "$KR/enter" >/dev/null; }
frame() { curl -s -X POST "$KR/frame" -d "{\"label\":\"$1\"}" >/dev/null; }
grepscr() { curl -s "$KR/screen" | python3 -c "import sys,json;print('\n'.join(json.load(sys.stdin)['lines']))" 2>/dev/null | grep -qiE "$1"; }
wait_scr() { local p="$1" t="${2:-60}" i; for i in $(seq 1 "$t"); do grepscr "$p" && return 0; sleep 1; done; return 1; }

start_kr() { # $1 = extra chat args
  stop_kr
  kill_stale_listener "$PORT"
  sleep 1
  cd "$TUI_DIR" || return 1
  KIRO_TEST_MODE=1 KIRO_REMOTE_SESSIONS_ENDPOINT="$ENDPOINT" KIRO_API_KEY="${SMOKE_API_KEY:-}" \
    bun run knight-rider --port "$PORT" --cmd "$BIN chat $1" >/tmp/kr-cloud-smoke.log 2>&1 &
  KR_PID=$!
  cd - >/dev/null || true
  sleep 20
  curl -s "$KR/status" | grep -q '"ready": *true' || { fail "knight-rider boot ($1)"; return 1; }
}

stop_kr() {
  [ -n "$KR_PID" ] && { pkill -P "$KR_PID" 2>/dev/null; kill "$KR_PID" 2>/dev/null; wait "$KR_PID" 2>/dev/null; }
  KR_PID=""
}

# ── endpoint setup ──────────────────────────────────────────────────────────
if [ "$MODE" = "mock" ]; then
  BFF_PORT="${KR_BFF_PORT:-8793}"
  kill_stale_listener "$BFF_PORT"
  ( cd "$TUI_DIR" && MOCK_BFF_PORT=$BFF_PORT exec bun e2e_tests/cloud/mock-bff.mjs >/tmp/mock-bff-smoke.log 2>&1 ) &
  BFF_PID=$!
  sleep 2
  ENDPOINT="http://127.0.0.1:$BFF_PORT"
  SMOKE_API_KEY="smoke-mock-key"
else
  ENDPOINT="https://app.kiro.dev"
  SMOKE_API_KEY=""
fi
say "mode=$MODE endpoint=$ENDPOINT binary=$BIN"

# ── --cloud boot ────────────────────────────────────────────────────────────
if start_kr "--cloud"; then
  if wait_scr "Cloud session created" 60; then pass "cloud boot"; else fail "cloud boot"; fi
  wait_scr "repositories found" 30 && pass "repo count row" || fail "repo count row"
  frame "cloud-boot"

  # /repo picker
  type_text "/repo"; enter
  if wait_scr "space to toggle|Provider|Search" 30; then pass "/repo picker"; else fail "/repo picker"; fi
  frame "repo-picker"
  curl -s -X POST "$KR/escape" >/dev/null; sleep 1

  # /disconnect
  type_text "/disconnect"; enter
  if wait_scr "work continues while" 30; then pass "disconnect"; else fail "disconnect"; fi
  frame "disconnect"
fi
stop_kr

# ── /quit keep-running prompt ───────────────────────────────────────────────
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 60
  type_text "/quit"; enter
  if wait_scr "continue working" 30; then pass "quit prompt"; else fail "quit prompt"; fi
  frame "quit-prompt"
  enter  # Yes (agent continues)
  wait_scr "work continues while" 20 && pass "quit keep-running" || fail "quit keep-running"
fi
stop_kr

# ── --repo skips picker ─────────────────────────────────────────────────────
if start_kr "--cloud --repo kiro-team/banana-service"; then
  wait_scr "Cloud session created" 60
  if wait_scr "banana-service" 20; then
    pass "--repo footer bind"
    # Only meaningful once the repo is actually bound: a bound --repo must not
    # auto-clone. Gated behind the positive above so it can't pass vacuously
    # when the cloud session never booted.
    if grepscr "cloning"; then fail "premature clone"; else pass "clone deferred"; fi
  else
    fail "--repo footer bind"
  fi
  frame "repo-flag"
fi
stop_kr

# ── headless listing (no PTY needed) ────────────────────────────────────────
# KIRO_KAS_SERVER_PATH/NODE_PATH are inherited (dev binaries have no embedded
# KAS bundle); only the TUI-path override is dropped for a headless run.
LIST_OUT=$(env -u KIRO_TEST_TUI_JS_PATH \
  KIRO_TEST_MODE=1 KIRO_REMOTE_SESSIONS_ENDPOINT="$ENDPOINT" KIRO_API_KEY="${SMOKE_API_KEY:-}" \
  "$BIN" chat --list-sessions 2>&1 | sed 's/\x1b\[[0-9;]*m//g')
echo "$LIST_OUT" | grep -q "Chat SessionId" && pass "listing runs" || fail "listing runs"
if [ "$MODE" = "mock" ]; then
  echo "$LIST_OUT" | grep -q "| cloud |" && pass "cloud rows" || fail "cloud rows"
fi

# ── dark-ship: released build must hide an injected cloud row ───────────────
# Only sound on a RELEASE binary: a debug build force-enables every rollout
# feature (cfg!(debug_assertions)), so the released shape is unobservable —
# this mirrors the Rust suite's #[cfg_attr(debug_assertions, ignore)] guard.
# We INJECT a cloud row via the test seam (so a broken gate would visibly leak
# it), drop every cloud env, and use a throwaway HOME so the developer's real
# sessions can't skew the result. With the rollout off, the cloud row must be
# filtered out; only cloud-specific markers are treated as a leak (a plain
# local row is expected and must not fail the check).
if [ "$BIN_PROFILE" = "release" ]; then
  DARK_HOME="$(mktemp -d)"
  DARK_MOCK='[{"sessionId":"sess_local1111-2222-4333-8444-555555555555","cwd":"'"$PWD"'","title":"KR local session","updatedAt":"2026-01-01T00:00:00Z"},{"sessionId":"cloudaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee","cwd":"'"$PWD"'","title":"KR cloud row","updatedAt":"2026-01-02T00:00:00Z","executionTarget":"cloud-sandbox","status":"in_progress"}]'
  DARK_OUT=$(env -u KIRO_TEST_MODE -u KIRO_REMOTE_SESSIONS_ENDPOINT -u KIRO_TEST_TUI_JS_PATH -u KIRO_KAS_SERVER_PATH -u KIRO_KAS_NODE_PATH \
    HOME="$DARK_HOME" KIRO_TEST_MOCK_KAS_SESSIONS="$DARK_MOCK" \
    "$BIN" chat --list-sessions 2>&1 | sed 's/\x1b\[[0-9;]*m//g')
  rm -rf "$DARK_HOME"
  if echo "$DARK_OUT" | grep -q "KR cloud row"; then
    fail "dark-ship leak (injected cloud row listed on released build)"
  elif echo "$DARK_OUT" | grep -qE "\| cloud \|"; then
    fail "dark-ship leak (cloud environment tag on released build)"
  else
    pass "dark-ship listing clean (injected cloud row hidden on released build)"
  fi
else
  say "  SKIP  dark-ship check (needs a release binary; debug force-enables the rollout)"
fi

# ── summary (process cleanup runs via the EXIT trap) ────────────────────────
stop_kr
say ""
if [ "$FAILS" -eq 0 ]; then say "ALL PASS"; exit 0; else say "$FAILS FAILURE(S)"; exit 1; fi

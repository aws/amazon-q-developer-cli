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
# Binary resolution: explicit override first (KIRO_CHAT_CLI_BIN — also what
# the E2E harness reads, and needed when cargo uses a shared target-dir),
# then the workspace-local target dirs.
if [ -n "${KIRO_CHAT_CLI_BIN:-}" ] && [ -x "$KIRO_CHAT_CLI_BIN" ]; then
  BIN="$KIRO_CHAT_CLI_BIN"
  case "$BIN" in */release/*) BIN_PROFILE="release";; *) BIN_PROFILE="debug";; esac
else
  BIN="$REPO_ROOT/target/release/chat_cli"; BIN_PROFILE="release"
  [ -x "$BIN" ] || { BIN="$REPO_ROOT/target/debug/chat_cli"; BIN_PROFILE="debug"; }
fi
[ -x "$BIN" ] || { echo "no chat_cli binary; run cargo build first (or set KIRO_CHAT_CLI_BIN)"; exit 1; }

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

# JSON payloads built with jq -n --arg so a quote/backslash can't break them.
type_text() { local m="$1" i c; for (( i=0; i<${#m}; i++ )); do c="${m:$i:1}"; curl -s -X POST "$KR/keys" -d "$(jq -cn --arg k "$c" '{keys:$k}')" >/dev/null; sleep 0.03; done; sleep 0.3; }
enter() { curl -s -X POST "$KR/enter" >/dev/null; }
frame() { curl -s -X POST "$KR/frame" -d "$(jq -cn --arg l "$1" '{label:$l}')" >/dev/null; }
grepscr() { curl -s "$KR/screen" | python3 -c "import sys,json;print('\n'.join(json.load(sys.stdin)['lines']))" 2>/dev/null | grep -qiE "$1"; }
wait_scr() { local p="$1" t="${2:-60}" i; for i in $(seq 1 "$t"); do grepscr "$p" && return 0; sleep 1; done; return 1; }

start_kr() { # $1 = extra chat args, $2 = optional --workspace dir for the PTY
  stop_kr
  kill_stale_listener "$PORT"
  sleep 1
  cd "$TUI_DIR" || return 1
  # bash 3.2 (macOS default) + `set -u` treats an empty array expansion as
  # unbound; the `+` parameter-expansion guard keeps it legal on both.
  local ws_args=()
  [ -n "${2:-}" ] && ws_args=(--workspace "$2")
  KIRO_TEST_MODE=1 KIRO_REMOTE_SESSIONS_ENDPOINT="$ENDPOINT" KIRO_API_KEY="${SMOKE_API_KEY:-}" \
    bun run knight-rider --port "$PORT" ${ws_args[@]+"${ws_args[@]}"} --cmd "$BIN chat $1" >/tmp/kr-cloud-smoke.log 2>&1 &
  KR_PID=$!
  cd - >/dev/null || true
  sleep 20
  curl -s "$KR/status" | grep -q '"ready": *true' || { fail "knight-rider boot ($1)"; return 1; }
}

stop_kr() {
  [ -n "$KR_PID" ] && { pkill -P "$KR_PID" 2>/dev/null; kill "$KR_PID" 2>/dev/null; wait "$KR_PID" 2>/dev/null; }
  KR_PID=""
}

# Prod teardown: each `start_kr "--cloud"` creates a REAL sandbox session,
# and killing the PTY detaches with the agent still running. Sections that
# don't test detach semantics end through here — /quit → "No (agent stops)"
# (second option) — so a prod run doesn't strand one live sandbox per
# section. Mock mode skips it (mock sessions are free; the extra keystrokes
# just slow the run). The /quit-semantics section keeps its deliberate
# keep-running exit.
end_cloud_session() {
  if [ "$MODE" = "prod" ] && [ -n "$KR_PID" ]; then
    type_text "/quit"; enter
    if wait_scr "continue working" 20; then
      curl -s -X POST "$KR/keys" -d '{"keys":"Down"}' >/dev/null; sleep 0.5
      enter
      sleep 3
    fi
  fi
  stop_kr
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

# ── resume with history + prompt-after-resume (mock only) ───────────────────
# Guards Pippin bugs #29 (resume stops replaying), #32 (first prompt after
# resume re-replays history), and #30 (startup tools labelled Cancelled).
# Needs the mock's canned transcript: restart the BFF with MOCK_BFF_HISTORY=1
# (the env var must reach the BFF process, not the TUI).
scr_dump() { curl -s "$KR/screen" | python3 -c "import sys,json;print('\n'.join(json.load(sys.stdin)['lines']))" 2>/dev/null; }
if [ "$MODE" = "mock" ]; then
  kill "$BFF_PID" 2>/dev/null; wait "$BFF_PID" 2>/dev/null
  kill_stale_listener "$BFF_PORT"
  ( cd "$TUI_DIR" && MOCK_BFF_PORT=$BFF_PORT MOCK_BFF_HISTORY=1 exec bun e2e_tests/cloud/mock-bff.mjs >/tmp/mock-bff-smoke.log 2>&1 ) &
  BFF_PID=$!
  sleep 2
  BANANA_ID="aaaaaaa1-0001-4001-8001-000000000001"
  if start_kr "--cloud --resume-id $BANANA_ID"; then
    # (The "Resuming…/✓ Cloud session resumed" checklist wording is pinned by
    # the E2E suite — its 50ms polling catches the transient checklist; this
    # script's 1s screen polls race the replay scrolling it away.)
    if wait_scr "Repo cloned: 120 files at HEAD" 60; then pass "resume replays history"; else fail "resume replays history"; fi
    frame "resume-history"
    # No pre-prompt "Cancelled" rows (bug #30):
    if grepscr "Cancelled"; then fail "startup tools labelled Cancelled"; else pass "no Cancelled startup rows"; fi
    # First prompt after resume must not duplicate the replay (bug #32):
    type_text "hello again"; enter
    sleep 8
    COUNT=$(scr_dump | grep -c "clone the repo and list the files" || true)
    if [ "${COUNT:-0}" -le 1 ]; then pass "no duplicate replay after prompt"; else fail "no duplicate replay after prompt (count=$COUNT)"; fi
    frame "resume-prompt"
  fi
  stop_kr
  # Restore the plain (history-less) BFF for the remaining sections.
  kill "$BFF_PID" 2>/dev/null; wait "$BFF_PID" 2>/dev/null
  kill_stale_listener "$BFF_PORT"
  ( cd "$TUI_DIR" && MOCK_BFF_PORT=$BFF_PORT exec bun e2e_tests/cloud/mock-bff.mjs >>/tmp/mock-bff-smoke.log 2>&1 ) &
  BFF_PID=$!
  sleep 2
fi

# ── input-cancel steer hygiene (both modes; guards the bug #23 shape) ───────
# ctrl+c while composing must not duplicate the next sent prompt: type,
# ctrl+c, retype, send — the transcript must show the marker exactly once.
# In prod the send starts a REAL model turn, so it is cancelled (Esc)
# immediately after the count is read; the assertion is about the keystroke
# path, not the reply.
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 90
  wait_scr "ask a question" 30
  type_text "first draft message"
  curl -s -X POST "$KR/ctrlc" >/dev/null 2>&1 || true; sleep 1
  type_text "MARKER_STEER_ONCE"; enter
  sleep 5
  COUNT=$(scr_dump | grep -c "MARKER_STEER_ONCE" || true)
  # Exactly one: zero means the send never landed (plausible in prod where a
  # real turn starts), which would otherwise pass the duplicate check vacuously.
  if [ "${COUNT:-0}" -eq 1 ]; then pass "no duplicate send after ctrl+c"; else fail "no duplicate send after ctrl+c (count=${COUNT:-0})"; fi
  frame "steer-cancel"
  curl -s -X POST "$KR/escape" >/dev/null; sleep 1  # cancel the real turn in prod
fi
end_cloud_session

# ── /sessions listing renders cloud rows (both modes) ───────────────────────
# The concurrent-row/switch matrix needs the mock's canned spaces (E2E +
# mock-only resume section); what prod CAN answer cheaply is that /sessions
# opens against the real BFF listing without a raw error.
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 90
  wait_scr "ask a question" 30
  type_text "/sessions"; enter
  sleep 8
  if grepscr "Internal error"; then fail "/sessions never raw-errors"; else pass "/sessions never raw-errors"; fi
  frame "sessions-list"
  curl -s -X POST "$KR/escape" >/dev/null
fi
end_cloud_session

# ── /autonomous verified mode switch (both modes) ───────────────────────────
# KAS relays session/set_mode for cloud sessions; the success lines prove the
# VERIFIED switch — the CLI prints them only after the read-back confirms the
# sandbox applied the mode. Mock: the mock BFF plays the sandbox core. Prod:
# the REAL sandbox answers — this is exactly the set_mode-durability question
# that used to be deferred, so run it (longer waits: real relay latency).
# Non-destructive: the section always switches back off.
AUTONOMOUS_WAIT=15; [ "$MODE" = "prod" ] && AUTONOMOUS_WAIT=45
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 90
  wait_scr "ask a question" 30
  type_text "/autonomous on"; enter
  if wait_scr "Autonomous mode on|not supported on this session yet" "$AUTONOMOUS_WAIT"; then
    if grepscr "not supported on this session yet"; then
      # An honest not-supported message is a FAIL in mock (relay is modeled).
      # In prod it is a BACKEND fact, not a CLI regression — record it
      # loudly as a WARN so the run's exit code stays about the CLI.
      if [ "$MODE" = "mock" ]; then fail "autonomous on (verified)"; else say "  WARN  PROD sandbox refused set_mode (relay not enabled server-side?) — CLI surfaced the honest message"; fi
    else
      pass "autonomous on (verified)"
      type_text "/autonomous off"; enter
      if wait_scr "Autonomous mode off" "$AUTONOMOUS_WAIT"; then pass "autonomous off (verified)"; else fail "autonomous off (verified)"; fi
    fi
  else
    fail "autonomous on (no response)"
  fi
  frame "autonomous"
fi
end_cloud_session

# ── /hooks sandbox fetch (both modes) ────────────────────────────────────────
# Cloud /hooks bypasses the local cache and forwards _kiro/hooks/list to the
# sandbox. Mock: the default responder answers {} → the panel's friendly
# empty state. Prod: Hooks v2 EP enabled in beta/gamma 08/03 — the real
# sandbox must answer with a panel or the friendly message, never the raw
# "Internal error" the 08/04 parity sweep caught (KIRONEXT-4 shape). The
# positive listing round-trip is the E2E (cloud-hooks.test.ts); smoke pins
# the never-raw-error bar against the real backend.
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 90
  wait_scr "ask a question" 30
  type_text "/hooks"; enter
  sleep 8
  if grepscr "Internal error"; then fail "/hooks never raw-errors"; else pass "/hooks never raw-errors"; fi
  if grepscr "\.kiro/hooks"; then fail "/hooks shows no local paths"; else pass "/hooks shows no local paths"; fi
  frame "hooks"
fi
end_cloud_session

# ── /mcp + /tools + /agent sandbox surfaces (both modes) ────────────────────
# Panels must open and read sandbox-side (provenance/awaiting notice or real
# sandbox data), never raw-error and never leak this machine's config. These
# already pass against the mock in the E2E suite; running them here in prod
# mode is the cheap live check for the #3690 surface.
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 90
  wait_scr "ask a question" 30
  type_text "/mcp"; enter
  sleep 6
  if grepscr "Internal error"; then fail "/mcp never raw-errors"; else pass "/mcp never raw-errors"; fi
  frame "mcp"; curl -s -X POST "$KR/escape" >/dev/null; sleep 1
  type_text "/tools"; enter
  sleep 6
  if grepscr "Internal error"; then fail "/tools never raw-errors"; else pass "/tools never raw-errors"; fi
  frame "tools"; curl -s -X POST "$KR/escape" >/dev/null; sleep 1
  type_text "/agent"; enter
  if wait_scr "Select agent|Waiting for the sandbox" 20; then pass "/agent picker sandbox-owned"; else fail "/agent picker sandbox-owned"; fi
  frame "agent"; curl -s -X POST "$KR/escape" >/dev/null
fi
end_cloud_session

# ── /spec cloud gate with seeded local specs ────────────────────────────────
# /spec reads the LOCAL .kiro/specs tree, not the sandbox clone, so it must
# refuse in a cloud session — and the seeded local spec must never leak onto
# the screen. Runs in BOTH modes: against prod the gate contract is the same.
SPEC_WS=$(mktemp -d "${TMPDIR:-/tmp}/kr-smoke-spec-ws.XXXXXX")
mkdir -p "$SPEC_WS/.kiro/specs/checkout-flow"
printf '# Tasks\n\n- [ ] 1. Wire the cart API\n' > "$SPEC_WS/.kiro/specs/checkout-flow/tasks.md"
if start_kr "--cloud" "$SPEC_WS"; then
  wait_scr "Cloud session created" 60
  wait_scr "ask a question" 20
  type_text "/spec"; enter
  if wait_scr "is not available for a cloud session" 15; then pass "spec gate refuses in cloud"; else fail "spec gate refuses in cloud"; fi
  if scr_dump | grep -Ev "/spec( |$)" | grep -q "checkout-flow"; then fail "seeded spec never leaks"; else pass "seeded spec never leaks"; fi
  frame "spec-gate"
fi
end_cloud_session
rm -rf "$SPEC_WS"

# ── subagent view on cloud replay (mock only) ───────────────────────────────
# Replays a canned transcript containing a completed invoke_sub_agent
# delegation; the cloud-only adapter must render the role, not a flat row
# (bug #12 shape). Mock-only: prod replay content isn't deterministic.
if [ "$MODE" = "mock" ]; then
  kill "$BFF_PID" 2>/dev/null; wait "$BFF_PID" 2>/dev/null
  kill_stale_listener "$BFF_PORT"
  ( cd "$TUI_DIR" && MOCK_BFF_PORT=$BFF_PORT MOCK_BFF_HISTORY=1 MOCK_BFF_SUBAGENT=1 exec bun e2e_tests/cloud/mock-bff.mjs >>/tmp/mock-bff-smoke.log 2>&1 ) &
  BFF_PID=$!
  sleep 2
  if start_kr "--cloud --resume-id $BANANA_ID"; then
    if wait_scr "Audit complete" 60; then pass "subagent turn replays"; else fail "subagent turn replays"; fi
    if grepscr "api-auditor"; then pass "subagent role renders"; else fail "subagent role renders"; fi
    if grepscr "Cancelled"; then fail "subagent stays completed"; else pass "subagent stays completed"; fi
    frame "subagent-replay"
  fi
  stop_kr
fi

# ── unrouted BFF → version-skew guidance (mock only; 07/31 outage) ──────────
# MOCK_BFF_UNROUTED=1 replays the outage: every op answered by the router with
# a bodyless no-code error, flattened by KAS to "<op>: UnknownError". The CLI
# must classify it version_skew (#3797) and render #3720's guidance — never a
# bare UnknownError, never a phantom successful session. Prod-mode runs skip
# this: it requires breaking the backend.
if [ "$MODE" = "mock" ]; then
  kill "$BFF_PID" 2>/dev/null; wait "$BFF_PID" 2>/dev/null
  kill_stale_listener "$BFF_PORT"
  ( cd "$TUI_DIR" && MOCK_BFF_PORT=$BFF_PORT MOCK_BFF_UNROUTED=1 exec bun e2e_tests/cloud/mock-bff.mjs >>/tmp/mock-bff-smoke.log 2>&1 ) &
  BFF_PID=$!
  sleep 2
  if start_kr "--cloud"; then
    if wait_scr "out of sync" 60; then pass "version-skew guidance renders"; else fail "version-skew guidance renders"; fi
    if grepscr "Update kiro"; then pass "guidance names recovery action"; else fail "guidance names recovery action"; fi
    if grepscr "Cloud session created"; then fail "no phantom created line"; else pass "no phantom created line"; fi
    frame "version-skew"
  fi
  stop_kr
  # Restore the plain BFF for the remaining sections.
  kill "$BFF_PID" 2>/dev/null; wait "$BFF_PID" 2>/dev/null
  kill_stale_listener "$BFF_PORT"
  ( cd "$TUI_DIR" && MOCK_BFF_PORT=$BFF_PORT exec bun e2e_tests/cloud/mock-bff.mjs >>/tmp/mock-bff-smoke.log 2>&1 ) &
  BFF_PID=$!
  sleep 2
fi

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

# ── released-rollout listing shape (release binary only) ────────────────────
# Only sound on a RELEASE binary: a debug build force-enables every rollout
# feature (cfg!(debug_assertions)), so the released shape is unobservable —
# this mirrors the Rust suite's #[cfg_attr(debug_assertions, ignore)] guard.
# CONTRACT (post-ramp, #3913): remote_sandbox is rolled out to ALL users at
# 100%, so a released build MUST list an injected cloud row with its cloud
# tag — a missing row now means the rollout regressed (or the kill-switch,
# rollout.json treatment_percent, was dialed down; if that ever happens
# deliberately, this section flips back to asserting the row is hidden).
# The row is INJECTED via the test seam, every cloud env is dropped, and a
# throwaway HOME keeps the developer's real sessions out of the result.
if [ "$BIN_PROFILE" = "release" ]; then
  DARK_HOME="$(mktemp -d)"
  DARK_MOCK='[{"sessionId":"sess_local1111-2222-4333-8444-555555555555","cwd":"'"$PWD"'","title":"KR local session","updatedAt":"2026-01-01T00:00:00Z"},{"sessionId":"cloudaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee","cwd":"'"$PWD"'","title":"KR cloud row","updatedAt":"2026-01-02T00:00:00Z","executionTarget":"cloud-sandbox","status":"in_progress"}]'
  # KIRO_API_KEY satisfies the auth gate without any network or login state
  # (active_auth_source keys off env presence alone) — without it a fresh
  # HOME sends the release binary into interactive login, which on a captured
  # stdout waits out the full 10-minute OAuth timeout and then errors,
  # leaving the greps below to run against an error message. It does not
  # force-enable any rollout feature (KIRO_TEST_MODE stays dropped).
  DARK_OUT=$(env -u KIRO_TEST_MODE -u KIRO_REMOTE_SESSIONS_ENDPOINT -u KIRO_TEST_TUI_JS_PATH -u KIRO_KAS_SERVER_PATH -u KIRO_KAS_NODE_PATH \
    HOME="$DARK_HOME" KIRO_TEST_MOCK_KAS_SESSIONS="$DARK_MOCK" KIRO_API_KEY="rollout-shape-check-key" \
    "$BIN" chat --list-sessions 2>&1 | sed 's/\x1b\[[0-9;]*m//g')
  rm -rf "$DARK_HOME"
  # Positive control FIRST: the injected LOCAL row must list. If it doesn't,
  # the listing never ran (auth gate, crash, flag rename) and any cloud-row
  # claim below would prove nothing.
  if ! echo "$DARK_OUT" | grep -q "KR local session"; then
    fail "released-rollout control (injected local row not listed — listing never ran)"
  elif ! echo "$DARK_OUT" | grep -q "KR cloud row"; then
    fail "released-rollout regression (cloud row hidden on released build — remote_sandbox ramp is 100%)"
  elif ! echo "$DARK_OUT" | grep -qE "\| cloud \|"; then
    fail "released-rollout regression (cloud row listed without its cloud environment tag)"
  else
    pass "released-rollout listing shape (injected cloud row listed with cloud tag)"
  fi
else
  say "  SKIP  released-rollout listing check (needs a release binary; debug force-enables the rollout)"
fi

# ── summary (process cleanup runs via the EXIT trap) ────────────────────────
stop_kr
say ""
if [ "$FAILS" -eq 0 ]; then say "ALL PASS"; exit 0; else say "$FAILS FAILURE(S)"; exit 1; fi

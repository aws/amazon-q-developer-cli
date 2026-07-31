#!/usr/bin/env bash
# Knight-rider review run: drives EVERY covered cloud-sandbox scenario in a
# live PTY, captures a labeled screenshot at each assertion point, and emits
# a machine-readable manifest (manifest.tsv) that gen-review-report.py folds
# into one self-contained review HTML.
#
# Usage (from packages/tui):
#   KIRO_CHAT_CLI_BIN=... KIRO_KAS_NODE_PATH=... bash e2e_tests/cloud/kr-review-run.sh [outdir]
#
# Mock-mode only (hermetic): every scenario runs against the local mock BFF.
set -uo pipefail

TUI_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$TUI_DIR/e2e_tests/test-outputs/cloud-review-$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$OUT"
MANIFEST="$OUT/manifest.tsv"
: > "$MANIFEST"

PORT="${KR_PORT:-3121}"
BFF_PORT="${KR_BFF_PORT:-8797}"
KR="http://localhost:$PORT/api"

# Binary resolution (same contract as knight-rider-smoke.sh).
if [ -n "${KIRO_CHAT_CLI_BIN:-}" ] && [ -x "$KIRO_CHAT_CLI_BIN" ]; then
  BIN="$KIRO_CHAT_CLI_BIN"
else
  BIN="$TUI_DIR/../../target/debug/chat_cli"
fi
[ -x "$BIN" ] || { echo "no chat_cli binary (set KIRO_CHAT_CLI_BIN)"; exit 1; }

FAILS=0
KR_PID=""
BFF_PID=""
SCENARIO=""   # current scenario id, used to prefix frame labels

say()  { printf '%s\n' "$*"; }
pass() { say "  PASS  $1"; printf '%s\tcheck\tPASS\t%s\n' "$SCENARIO" "$1" >> "$MANIFEST"; }
fail() { say "  FAIL  $1"; printf '%s\tcheck\tFAIL\t%s\n' "$SCENARIO" "$1" >> "$MANIFEST"; FAILS=$((FAILS+1)); }
note() { printf '%s\tnote\t-\t%s\n' "$SCENARIO" "$1" >> "$MANIFEST"; }

kill_stale_listener() { local pids; pids=$(lsof -ti tcp:"$1" 2>/dev/null || true); [ -n "$pids" ] && kill $pids 2>/dev/null; sleep 0.5; }

cleanup() {
  stop_kr
  [ -n "$BFF_PID" ] && { kill "$BFF_PID" 2>/dev/null; wait "$BFF_PID" 2>/dev/null; }
}
trap cleanup EXIT INT TERM

# All JSON payloads are built with `jq -n --arg` so a label or typed char that
# contains a quote/backslash can't break the request (review of PR #3710).
kjson() { jq -cn --arg k "$1" '{keys:$k}'; }
type_text() { local m="$1" i c; for (( i=0; i<${#m}; i++ )); do c="${m:$i:1}"; curl -s -X POST "$KR/keys" -d "$(kjson "$c")" >/dev/null; sleep 0.03; done; sleep 0.3; }
enter() { curl -s -X POST "$KR/enter" >/dev/null; }
tab()   { curl -s -X POST "$KR/keys" -d '{"keys":"\t"}' >/dev/null; sleep 0.3; }
space() { curl -s -X POST "$KR/keys" -d '{"keys":" "}' >/dev/null; sleep 0.3; }
esc()   { curl -s -X POST "$KR/escape" >/dev/null; sleep 0.5; }
ctrlc() { curl -s -X POST "$KR/ctrlc" >/dev/null; sleep 0.5; }
scr_dump() { curl -s "$KR/screen" | python3 -c "import sys,json;print('\n'.join(json.load(sys.stdin)['lines']))" 2>/dev/null; }
grepscr() { scr_dump | grep -qiE "$1"; }
wait_scr() { local p="$1" t="${2:-60}" i; for i in $(seq 1 "$t"); do grepscr "$p" && return 0; sleep 1; done; return 1; }

# Global invariant, run at the end of EVERY scenario: no error may be visible
# anywhere EXCEPT a deliberate gate/refusal. Deliberate = the humanized
# "… is not available for a cloud session yet." messages and the provider-gate
# screen ("Source provider not found" + setup handoff). Everything else —
# raw session-uuid errors, sandbox rejections, wire errors, generic
# Internal error / Failed-to banners — is a bug surface and fails the check.
no_error_check() {
  local screen; screen="$(scr_dump)"
  local errs
  errs=$(printf '%s\n' "$screen" \
    | grep -iE "Internal error|rejected by sandbox|Remote session source|Session '?[0-9a-f-]{36}'? not found|Failed to|error occurred" \
    | grep -vE "is not available for a cloud session" \
    | grep -vE "Source provider not found" || true)
  if [ -n "$errs" ]; then
    fail "no unexpected error on screen"
    printf '%s\n' "$errs" | head -3 | while IFS= read -r l; do note "error row: $l"; done
  else
    pass "no unexpected error on screen"
  fi
}
frame() { # $1 = frame label (auto-prefixed with scenario id)
  curl -s -X POST "$KR/frame" -d "$(jq -cn --arg l "${SCENARIO}--$1" '{label:$l}')" >/dev/null
  printf '%s\tframe\t-\t%s\n' "$SCENARIO" "${SCENARIO}--$1" >> "$MANIFEST"
}

start_bff() { # $@ = extra env KEY=VAL...
  [ -n "$BFF_PID" ] && { kill "$BFF_PID" 2>/dev/null; wait "$BFF_PID" 2>/dev/null; BFF_PID=""; }
  kill_stale_listener "$BFF_PORT"
  ( cd "$TUI_DIR" && env MOCK_BFF_PORT=$BFF_PORT "$@" bun e2e_tests/cloud/mock-bff.mjs >>"$OUT/mock-bff.log" 2>&1 ) &
  BFF_PID=$!
  sleep 2
}

start_kr() { # $1 = chat args
  stop_kr
  kill_stale_listener "$PORT"
  sleep 1
  cd "$TUI_DIR" || return 1
  KIRO_TEST_MODE=1 KIRO_REMOTE_SESSIONS_ENDPOINT="http://127.0.0.1:$BFF_PORT" KIRO_API_KEY="review-mock-key" \
    bun run knight-rider --port "$PORT" --out "$OUT/frames" --cmd "$BIN chat $1" >>"$OUT/kr.log" 2>&1 &
  KR_PID=$!
  cd - >/dev/null || true
  sleep 20
  curl -s "$KR/status" | grep -q '"ready": *true' || { fail "knight-rider boot ($1)"; return 1; }
}

stop_kr() {
  [ -n "$KR_PID" ] && { pkill -P "$KR_PID" 2>/dev/null; kill "$KR_PID" 2>/dev/null; wait "$KR_PID" 2>/dev/null; }
  KR_PID=""
}

scenario() { # $1 = id, $2 = title, $3 = reference (bug/story)
  SCENARIO="$1"
  say ""
  say "── $1: $2"
  printf '%s\ttitle\t-\t%s\n' "$1" "$2" >> "$MANIFEST"
  printf '%s\tref\t-\t%s\n'   "$1" "$3" >> "$MANIFEST"
}

BANANA_ID="aaaaaaa1-0001-4001-8001-000000000001"
EMPTY_ID="aaaaaaa2-0002-4002-8002-000000000002"

# ════════════════════════════════════════════════════════════════════════════
say "output: $OUT"
start_bff

# ── S01 cloud boot ──────────────────────────────────────────────────────────
scenario "S01" "Cloud session boot: checklist + repo count + ☁ footer" "Story 1; batch-1 t1"
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 60 && pass "checklist: session created" || fail "checklist: session created"
  wait_scr "repositories found" 30 && pass "checklist: repo count row" || fail "checklist: repo count row"
  wait_scr "ask a question" 20 && pass "prompt usable" || fail "prompt usable"
  grepscr "Cloud" && pass "cloud footer chip" || fail "cloud footer chip"
  if grepscr "Cancelled"; then fail "no Cancelled startup rows (bug #30)"; else pass "no Cancelled startup rows (bug #30)"; fi
  no_error_check
  frame "boot"
fi
stop_kr

# ── S02 provider gate ───────────────────────────────────────────────────────
scenario "S02" "No source provider → gate with Kiro Web handoff" "Story 2; batch-1 t2"
start_bff MOCK_BFF_NO_PROVIDER=1
if start_kr "--cloud"; then
  wait_scr "source provider" 60 && pass "gate text" || fail "gate text"
  if grepscr "kiro.dev|browser"; then pass "setup handoff offered"; else fail "setup handoff offered"; fi
  if grepscr "ask a question"; then fail "no chat prompt while gated"; else pass "no chat prompt while gated"; fi
  no_error_check
  frame "provider-gate"
fi
stop_kr
start_bff

# ── S03 repo picker interactions ────────────────────────────────────────────
scenario "S03" "/repo picker: list, select, Tab->Selected, space-uncheck" "Story 3, bug #28, checkmark nit; #3656"
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 60
  wait_scr "ask a question" 20
  type_text "/repo"; sleep 0.5; enter
  wait_scr "banana-service" 30 && pass "picker lists provider repos" || fail "picker lists provider repos"
  grepscr "tab to switch panels" && pass "tab hint present" || fail "tab hint present"
  frame "picker-open"
  space
  grepscr "Selected\(1\)" && pass "space selects (Selected(1))" || fail "space selects (Selected(1))"
  frame "picker-selected"
  tab; space
  grepscr "Selected\(0\)" && pass "Tab->Selected + space unchecks (bug #28)" || fail "Tab->Selected + space unchecks (bug #28)"
  no_error_check
  frame "picker-unchecked"
  esc
fi
stop_kr

# ── S04 --repo binds footer, defers clone ───────────────────────────────────
scenario "S04" "--repo <name> binds footer, skips picker, defers clone" "Story 3b; batch-1 t4"
if start_kr "--cloud --repo kiro-team/banana-service"; then
  wait_scr "Cloud session created" 60
  wait_scr "banana-service" 20 && pass "footer binds repo" || fail "footer binds repo"
  if grepscr "space to toggle"; then fail "picker did not open"; else pass "picker did not open"; fi
  if grepscr "cloning"; then fail "clone deferred"; else pass "clone deferred"; fi
  no_error_check
  frame "repo-flag"
fi
stop_kr

# ── S05 command gates ───────────────────────────────────────────────────────
scenario "S05" "Cloud command gates: /chat save|load, /context add, ! shell" "Bugs #19, #8/#22, local-bash blocker; #3656"
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 60
  wait_scr "ask a question" 20
  type_text "/chat save /tmp/x.json"; sleep 0.5; enter
  wait_scr "not available for a cloud session" 15 && pass "/chat save gated" || fail "/chat save gated"
  if grepscr "\.kiro/sessions"; then fail "no local path leak"; else pass "no local path leak"; fi
  frame "gate-chatsave"
  type_text "/context add /tmp/nope.txt"; sleep 0.5; enter
  wait_scr "context add is not available|not available for a cloud session" 15 && pass "/context add gated" || fail "/context add gated"
  frame "gate-context"
  type_text "!echo LEAKED_LOCAL_EXEC"; sleep 0.5; enter
  wait_scr "not available for a cloud session" 15 && pass "! shell escape refused" || fail "! shell escape refused"
  TOTAL=$(scr_dump | grep -c "LEAKED_LOCAL_EXEC" || true)
  ECHOED=$(scr_dump | grep -c "!echo LEAKED_LOCAL_EXEC" || true)
  if [ "${TOTAL:-0}" -eq "${ECHOED:-0}" ]; then pass "no local exec output"; else fail "no local exec output (total=$TOTAL echoed=$ECHOED)"; fi
  frame "gate-shell"
  no_error_check
fi
stop_kr


# ── S06 resume with history ─────────────────────────────────────────────────
scenario "S06" "Resume replays full trajectory + resume wording + completed tools" "Bugs #2, #29; story 5; #3656/#3664"
start_bff MOCK_BFF_HISTORY=1
if start_kr "--cloud --resume-id $BANANA_ID"; then
  wait_scr "Repo cloned: 120 files at HEAD" 60 && pass "history replays" || fail "history replays"
  grepscr "clone the repo and list the files" && pass "user rows replay" || fail "user rows replay"
  grepscr "Shell git clone banana-service" && pass "tool row replays" || fail "tool row replays"
  if grepscr "Cancelled|interrupted"; then fail "completed tools not Cancelled (bug #2)"; else pass "completed tools not Cancelled (bug #2)"; fi
  frame "resume-history"
  # prompt-after-resume: no duplicate replay (bug #32, layered-dedup pin)
  type_text "hello again"; enter; sleep 8
  COUNT=$(scr_dump | grep -c "clone the repo and list the files" || true)
  if [ "${COUNT:-0}" -eq 1 ]; then pass "no duplicate replay after prompt (bug #32)"; else fail "no duplicate replay after prompt (count=$COUNT)"; fi
  no_error_check
  frame "resume-prompt"
fi
stop_kr

# ── S07 resume isolation (empty session) ────────────────────────────────────
scenario "S07" "Resume EMPTY session: no other session's history leaks" "Resume isolation; batch-2 t2"
if start_kr "--cloud --resume-id $EMPTY_ID"; then
  wait_scr "ask a question" 60 && pass "empty resume boots" || fail "empty resume boots"
  if grepscr "clone the repo and list the files"; then fail "no history leak"; else pass "no history leak"; fi
  no_error_check
  frame "resume-empty"
fi
stop_kr

# ── S08 /clear residue ──────────────────────────────────────────────────────
scenario "S08" "/clear wipes pre-clear conversation from viewport" "#3651 re-wipe; /clear = new session"
if start_kr "--cloud --resume-id $BANANA_ID"; then
  wait_scr "Repo cloned: 120 files at HEAD" 60
  frame "before-clear"
  type_text "/clear"; sleep 0.5; enter
  wait_scr "ask a question" 30
  sleep 10  # past the 8s max re-wipe delay
  if grepscr "clone the repo and list the files"; then fail "viewport residue wiped"; else pass "viewport residue wiped"; fi
  no_error_check
  frame "after-clear"
fi
stop_kr
start_bff MOCK_BFF_HISTORY=1 MOCK_BFF_CONCURRENT=1

# ── S09 concurrent sessions + switch ────────────────────────────────────────
scenario "S09" "/sessions lists concurrent cloud rows; picker switch replays target" "Story 4/7; batch-2 t3/t4"
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 60
  wait_scr "ask a question" 20
  type_text "/sessions"; sleep 0.5; enter
  wait_scr "refactor payments" 30 && pass "concurrent rows listed" || fail "concurrent rows listed"
  grepscr "migrate database" && pass "second concurrent row" || fail "second concurrent row"
  if scr_dump | grep -qE "refactor payments.*cloud"; then pass "row-scoped cloud tag"; else fail "row-scoped cloud tag"; fi
  frame "sessions-list"
  type_text "refactor"; sleep 0.5; enter
  wait_scr "refactor the payments retry logic" 30 && pass "switch replays TARGET transcript" || fail "switch replays TARGET transcript"
  if grepscr "clone the repo and list the files"; then fail "no wrong-session replay"; else pass "no wrong-session replay"; fi
  no_error_check
  frame "switched"
fi
stop_kr
start_bff

# ── S10 disconnect / quit ───────────────────────────────────────────────────
scenario "S10" "/disconnect detaches (work continues); /quit keep-running prompt" "Story 6; batch-1 t5/t6"
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 60
  wait_scr "ask a question" 20
  type_text "/disconnect"; sleep 0.5; enter
  wait_scr "work continues while" 30 && pass "/disconnect detaches" || fail "/disconnect detaches"
  grepscr "resume-id" && pass "reattach hint present" || fail "reattach hint present"
  no_error_check
  frame "disconnect"
fi
stop_kr
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 60
  type_text "/quit"; sleep 0.5; enter
  wait_scr "continue working" 30 && pass "/quit keep-running prompt" || fail "/quit keep-running prompt"
  frame "quit-prompt"
  enter
  wait_scr "work continues while" 20 && pass "keep-running detaches" || fail "keep-running detaches"
  no_error_check
  frame "quit-detached"
fi
stop_kr

# ── S11 input-cancel steer hygiene ──────────────────────────────────────────
scenario "S11" "ctrl+c while composing: next send appears exactly once" "Bug #23 shape"
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 60
  wait_scr "ask a question" 20
  type_text "first draft message"
  ctrlc; sleep 1
  type_text "MARKER_STEER_ONCE"; enter; sleep 5
  COUNT=$(scr_dump | grep -c "MARKER_STEER_ONCE" || true)
  if [ "${COUNT:-0}" -eq 1 ]; then pass "no duplicate send after ctrl+c"; else fail "no duplicate send after ctrl+c (count=$COUNT)"; fi
  no_error_check
  frame "steer-cancel"
fi
stop_kr

# ── S12 file attach ships bytes ─────────────────────────────────────────────
scenario "S12" "Local file path in prompt ships bytes in-band to BFF" "Bug #34; #3652"
ATTACH_FILE="$(mktemp /tmp/kr-attach-XXXXXX).txt"
echo "SECRET-MARKER-XYLOPHONE-42" > "$ATTACH_FILE"
IMG_FILE="$(mktemp /tmp/kr-attach-XXXXXX).png"
# 1x1 red PNG (68 bytes) — enough to prove the IMAGE path ships base64 blocks.
python3 - "$IMG_FILE" <<'PYEOF'
import base64, sys, pathlib
pathlib.Path(sys.argv[1]).write_bytes(base64.b64decode(
 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='))
PYEOF
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 60
  wait_scr "ask a question" 20
  type_text "read $ATTACH_FILE"; enter; sleep 8
  if grep -q "SECRET-MARKER-XYLOPHONE-42" "$OUT/mock-bff.log"; then pass "text-file bytes reached BFF wire"; else fail "text-file bytes reached BFF wire"; fi
  no_error_check
  frame "attach-text"
fi
stop_kr
# Image variant (the visible half of bug #34) in a FRESH boot: the mock never
# ends the first turn on the downlink, so a second prompt in the same session
# queues as steer and never reaches the wire — each probe must be prompt #1.
# An image block carries mimeType image/png in the decoded op log.
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 60
  wait_scr "ask a question" 20
  type_text "describe $IMG_FILE"; enter; sleep 8
  if grep -q "image/png" "$OUT/mock-bff.log"; then pass "image bytes reached BFF wire (image/png block)"; else fail "image bytes reached BFF wire (image/png block)"; fi
  no_error_check
  frame "attach-image"
fi
stop_kr
rm -f "$ATTACH_FILE" "$IMG_FILE"

# ── S13 opt-in: no --cloud stays local ──────────────────────────────────────
scenario "S13" "Same env WITHOUT --cloud boots a plain local session" "Opt-in semantics; batch-1 t7"
if start_kr ""; then
  wait_scr "ask a question" 60 && pass "local session boots" || fail "local session boots"
  if grepscr "Cloud session created"; then fail "no cloud checklist"; else pass "no cloud checklist"; fi
  # Local control: allow the sandboxed HOME's known "N MCP failure" row (test
  # env artifact, not a cloud surface); everything else must still be clean.
  ERRS=$(scr_dump | grep -iE "Internal error|rejected by sandbox|Remote session source|Session '?[0-9a-f-]{36}'? not found|error occurred" || true)
  if [ -n "$ERRS" ]; then fail "no unexpected error on screen"; else pass "no unexpected error on screen"; fi
  frame "local-optin"
fi
stop_kr

# ── S14 headless listing (no PTY) ───────────────────────────────────────────
scenario "S14" "--list-sessions shows cloud rows with env+status columns" "Story 4; rust-integ mirror"
SCENARIO="S14"
LIST_OUT=$(env -u KIRO_TEST_TUI_JS_PATH \
  KIRO_TEST_MODE=1 KIRO_REMOTE_SESSIONS_ENDPOINT="http://127.0.0.1:$BFF_PORT" KIRO_API_KEY="review-mock-key" \
  "$BIN" chat --list-sessions 2>&1 | sed 's/\x1b\[[0-9;]*m//g')
echo "$LIST_OUT" > "$OUT/frames-listing.txt"
echo "$LIST_OUT" | grep -q "Chat SessionId" && pass "listing runs" || fail "listing runs"
echo "$LIST_OUT" | grep -q "| cloud |" && pass "cloud rows tagged" || fail "cloud rows tagged"
note "headless output saved as frames-listing.txt (no PTY frame)"

# ── S15 /autonomous on|off (verified mode switch over the relay) ─────────────
# KAS 0.27.8 relays session/set_mode + the read-back set_config_option to the
# sandbox (SendAcpMessage); the mock BFF applies the mode and answers the
# read-back, so the success lines here prove the VERIFIED switch — the CLI
# only prints them after the read-back confirms the sandbox applied the mode.
scenario "S15" "/autonomous picker + verified on/off switch" "#3653 CLI; KAS 0.27.8 relay; mock sandbox applies set_mode"
start_bff
# The BFF log is appended across scenarios; mark where S15 begins so the wire
# grep below can't be satisfied by an earlier scenario's traffic.
echo "=== S15-WIRE-MARK ===" >> "$OUT/mock-bff.log"
if start_kr "--cloud"; then
  wait_scr "Cloud session created" 60
  wait_scr "ask a question" 20
  # Bare /autonomous: picker with the current state tagged. Fresh session is
  # off, so [current] must sit on the off row.
  type_text "/autonomous"; enter
  if wait_scr "\[current\]" 15; then pass "picker opens with [current] tag"; else fail "picker opens with [current] tag"; fi
  if scr_dump | grep -qE "off +\[current\]"; then pass "[current] tags the off row on a fresh session"; else fail "[current] tags the off row on a fresh session"; fi
  frame "picker"
  esc
  # ON: printed only after the read-back verification confirms the sandbox
  # applied the mode. The not-supported fallback would be a relay regression.
  type_text "/autonomous on"; enter
  if wait_scr "Autonomous mode on" 15; then pass "on: verified switch confirmed"; else fail "on: verified switch confirmed"; fi
  if grepscr "not supported on this session yet"; then fail "no not-supported fallback"; else pass "no not-supported fallback"; fi
  if awk '/=== S15-WIRE-MARK ===/{f=1} f' "$OUT/mock-bff.log" | grep -q '"modeId":"autonomous"'; then pass "set_mode crossed the BFF wire"; else fail "set_mode crossed the BFF wire"; fi
  frame "on"
  # Idempotent ON.
  type_text "/autonomous on"; enter
  if wait_scr "Autonomous mode is already on" 15; then pass "on-when-on says already on"; else fail "on-when-on says already on"; fi
  # OFF: same verified path back to the default agent.
  type_text "/autonomous off"; enter
  if wait_scr "Autonomous mode off" 15; then pass "off: verified switch back"; else fail "off: verified switch back"; fi
  no_error_check
  frame "off"
fi
stop_kr

# ════════════════════════════════════════════════════════════════════════════
say ""
if [ "$FAILS" -eq 0 ]; then say "ALL SCENARIOS PASS"; else say "$FAILS CHECK(S) FAILED"; fi
say "frames:   $OUT/frames/"
say "manifest: $MANIFEST"
say "next:     python3 e2e_tests/cloud/gen-review-report.py $OUT"
exit "$FAILS"

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

start_kr() { # $1 = chat args, $2 = optional --workspace dir for the PTY
  stop_kr
  kill_stale_listener "$PORT"
  sleep 1
  cd "$TUI_DIR" || return 1
  # bash 3.2 (macOS default) + `set -u` treats an empty array expansion as
  # unbound; the `+` parameter-expansion guard keeps it legal on both.
  local ws_args=()
  [ -n "${2:-}" ] && ws_args=(--workspace "$2")
  # KR_ENGINE=kas forces the KAS engine for LOCAL (no --cloud) boots — cloud
  # boots auto-select it, but a plain local control run would otherwise get
  # the default engine, where KAS-only commands like /spec go to chat. The
  # assignment goes through `env` because a parameter expansion is not
  # parsed as a lexical VAR=val command prefix.
  env KIRO_TEST_MODE=1 KIRO_REMOTE_SESSIONS_ENDPOINT="http://127.0.0.1:$BFF_PORT" KIRO_API_KEY="review-mock-key" \
    ${KR_ENGINE:+KIRO_AGENT_ENGINE="$KR_ENGINE"} \
    bun run knight-rider --port "$PORT" --out "$OUT/frames" ${ws_args[@]+"${ws_args[@]}"} --cmd "$BIN chat $1" >>"$OUT/kr.log" 2>&1 &
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

# ── S16 unrouted BFF → version-skew guidance (07/31 outage signature) ───────
# The one scenario whose SUBJECT is an error screen, so it does not run
# no_error_check; it asserts the classified guidance instead. The unrouted
# BFF answers every op with the router's no-code error, which the KAS Smithy
# client flattens to "createSession: UnknownError" (#3797).
scenario "S16" "Unrouted BFF: version-skew guidance, no silent fallback" "#3797; #3720 guidance; 07/31 outage"
start_bff MOCK_BFF_UNROUTED=1
if start_kr "--cloud"; then
  if wait_scr "Cloud session failed" 60; then pass "boot checklist marks the failure"; else fail "boot checklist marks the failure"; fi
  if wait_scr "out of sync" 20; then pass "version-skew guidance renders"; else fail "version-skew guidance renders"; fi
  if grepscr "Update kiro"; then pass "guidance names the recovery action"; else fail "guidance names the recovery action"; fi
  if grepscr "Cloud session created"; then fail "no phantom created line"; else pass "no phantom created line"; fi
  frame "guidance"
fi
stop_kr

# ── S17 /spec gate: pre-fed local specs never leak into a cloud session ─────
# /spec reads the LOCAL .kiro/specs tree (not the sandbox clone), so every
# form refuses in cloud. The workspace is seeded with a real spec first —
# the refusal is only meaningful with actual local state to leak — and the
# local control (same seeded workspace, no --cloud) proves the gate is
# cloud-scoped rather than /spec being broken.
scenario "S17" "/spec refuses in cloud; seeded local specs never leak; local control works" "scope-mismatch class of #3690/bug 19; cloud-spec.test.ts mirror"
SPEC_WS=$(mktemp -d "${TMPDIR:-/tmp}/kr-spec-ws.XXXXXX")
mkdir -p "$SPEC_WS/.kiro/specs/checkout-flow"
printf '# Requirements\n\nWHEN the cart changes THE checkout page SHALL recompute totals.\n' > "$SPEC_WS/.kiro/specs/checkout-flow/requirements.md"
printf '# Tasks\n\n- [ ] 1. Wire the cart API\n- [ ] 2. Render order totals\n' > "$SPEC_WS/.kiro/specs/checkout-flow/tasks.md"
start_bff
if start_kr "--cloud" "$SPEC_WS"; then
  wait_scr "Cloud session created" 60
  wait_scr "ask a question" 20
  type_text "/spec"; enter
  if wait_scr "is not available for a cloud session" 15; then pass "bare /spec refuses"; else fail "bare /spec refuses"; fi
  # The seeded feature name may appear only in the typed command echo, never
  # from a .kiro/specs read (picker row / document list). The echo filter
  # matches `/spec` as a command token — a bare substring would also drop
  # real leak rows like `.kiro/specs/checkout-flow/`.
  if scr_dump | grep -Ev "/spec( |$)" | grep -q "checkout-flow"; then fail "seeded spec never leaks"; else pass "seeded spec never leaks"; fi
  frame "spec-gate"
  type_text "/spec run checkout-flow"; enter
  if wait_scr "is not available for a cloud session" 15; then pass "/spec run refuses"; else fail "/spec run refuses"; fi
  no_error_check
  frame "spec-run-gate"
fi
stop_kr
# Local control: same seeded workspace, no --cloud — the picker must list
# the seeded feature and the gate must not fire. KAS engine forced: /spec is
# KAS-only, and a local boot doesn't auto-select KAS the way --cloud does.
if KR_ENGINE=kas start_kr "" "$SPEC_WS"; then
  wait_scr "ask a question" 60
  type_text "/spec"; enter
  if wait_scr "checkout-flow" 15; then pass "local control: picker lists seeded spec"; else fail "local control: picker lists seeded spec"; fi
  if grepscr "is not available for a cloud session"; then fail "local control: no cloud gate"; else pass "local control: no cloud gate"; fi
  frame "spec-local-control"
fi
stop_kr
rm -rf "$SPEC_WS"

# ── S18 subagent view on cloud replay (invoke_sub_agent, bug #12 shape) ─────
# The sandbox KAS delegates via invoke_sub_agent; on replay the fold strips
# _meta, leaving the "Sub-agent: <role>" title shape. The cloud-only
# invoke-subagent adapter (#3657 port) must claim it and render the role —
# a bare flat tool row is the flattened view bug #12 reported.
scenario "S18" "Replayed invoke_sub_agent renders with role, completed, turns intact" "bug #12 seam; #3657 port; mirrors cloud-subagent-view.test.ts"
start_bff MOCK_BFF_HISTORY=1 MOCK_BFF_SUBAGENT=1
if start_kr "--cloud --resume-id $BANANA_ID"; then
  wait_scr "audit the API layer for gaps" 60
  wait_scr "Audit complete" 20
  if grepscr "api-auditor"; then pass "delegation renders with its role"; else fail "delegation renders with its role"; fi
  if grepscr "Cancelled|interrupted"; then fail "completed delegation stays completed"; else pass "completed delegation stays completed"; fi
  if grepscr "now add a health check endpoint"; then pass "surrounding turns replay intact"; else fail "surrounding turns replay intact"; fi
  no_error_check
  frame "subagent-replay"
fi
stop_kr

# ── S19 /config cloud-config panel (config-as-cloud-replica surface) ────────
# With the BFF relaying cloud config, /config must open the category table
# and the steering page must list the relayed documents, each carrying a
# cloud Source on its own row — never a raw error.
scenario "S19" "/config renders relayed cloud config with cloud Source" "cloud config launch"
start_bff MOCK_BFF_CLOUD_CONFIG=1
# The mock serves cloud-config frames on the LoadSession downlink only, so
# resume the canned empty space instead of creating a new session.
if start_kr "--cloud --resume-id $EMPTY_ID"; then
  wait_scr "ask a question" 60
  type_text "/config"; enter
  if wait_scr "Category" 20; then pass "/config category table opens"; else fail "/config category table opens"; fi
  # In a cloud session every config row must carry a cloud Source; pin it on
  # the steering row itself — the ☁ footer chip also reads "Cloud", so a
  # screen-wide match would pass vacuously.
  if scr_dump | grep -i "steering" | grep -qi "cloud"; then pass "category table steering row carries cloud source"; else fail "category table steering row carries cloud source"; fi
  frame "config-table"
  esc
  type_text "/config steering"; enter
  if wait_scr "team-conventions" 20; then pass "steering lists first relayed cloud doc"; else fail "steering lists first relayed cloud doc"; fi
  if wait_scr "api-guidelines" 10; then pass "steering lists second relayed cloud doc"; else fail "steering lists second relayed cloud doc"; fi
  if scr_dump | grep -i "team-conventions" | grep -qi "cloud"; then pass "steering doc row carries cloud source"; else fail "steering doc row carries cloud source"; fi
  no_error_check
  frame "config-steering"
fi
stop_kr
# Restore the plain BFF so a scenario appended later never silently inherits
# the cloud-config payloads from the ambient server.
start_bff

# ════════════════════════════════════════════════════════════════════════════
say ""
if [ "$FAILS" -eq 0 ]; then say "ALL SCENARIOS PASS"; else say "$FAILS CHECK(S) FAILED"; fi
say "frames:   $OUT/frames/"
say "manifest: $MANIFEST"
say "next:     python3 e2e_tests/cloud/gen-review-report.py $OUT"
exit "$FAILS"

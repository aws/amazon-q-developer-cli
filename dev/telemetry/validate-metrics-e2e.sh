#!/usr/bin/env bash
# End-to-end metric validation against the LOCAL dev telemetry stack (NOT prod
# KUTS). Proves that metrics from BOTH emitters land in the local Prometheus
# when KIRO_TELEMETRY_OTLP_ENDPOINT points at the dev collector:
#
#   1. thin Rust launcher metrics (via the real OTel SDK code path,
#      crates/kiro-telemetry catalog_smoke example):
#        - kiro_cli_session_started_total
#        - kiro_cli.session.completed   (-> kiro_cli_session_completed_total)
#        - kiro_cli_user_logged_in_total
#        - kiro_cli_daily_heartbeat
#        - kiro_cli_auth_credential_failure_total
#
#   2. TUI metrics (via the real OTel JS SDK / tui-telemetry-observer code path,
#      dev/telemetry/emit-tui-metrics.fixture.ts) — §C1 + §C4 product metrics +
#      §E perf metrics, emitted for both engines (v2/v3 label split):
#        §C1: kiro_cli_chat_session_started_total, kiro_cli_user_turns,
#             kiro_cli_user_turn_duration_seconds (hist), kiro_cli_tool_call_total
#        §C4: kiro_cli_tokens_consumed, kiro_cli_model_invocations_total,
#             kiro_cli_turn_outcome_total, kiro_cli_tool_execution_duration_ms (hist),
#             kiro_cli_context_usage_percentage (gauge),
#             kiro_cli_mode_active_total, kiro_cli_subagent_delegations_total
#        §E:  kiro_cli.process.memory.{rss,peak_rss,heap_used} (gauges),
#             kiro_cli.process.cpu.utilization (hist),
#             kiro_cli.tui.{event_loop.delay,input.latency,render.duration} (hist)
#      The SDK batches, so the fixture force-flushes before exit.
#
# These are emitted through the actual product code paths (no curl-faked
# payloads), then asserted via the Prometheus HTTP API.
#
# The script is idempotent: it brings the stack up (optionally wiping prior
# state with -v), emits, asserts each metric NAME is present with at least one
# sample, prints the JSON result vectors as evidence, and (unless KEEP_STACK=1)
# tears the stack down.
#
# Runtime: finch compose (per dev/telemetry/README.md). Override with
# COMPOSE_CMD="docker compose" if needed.
#
# Usage:
#   bash dev/telemetry/validate-metrics-e2e.sh
#   KEEP_STACK=1 bash dev/telemetry/validate-metrics-e2e.sh   # leave stack up
#   FRESH=0      bash dev/telemetry/validate-metrics-e2e.sh   # reuse running stack, don't wipe
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"

compose_cmd="${COMPOSE_CMD:-finch compose}"
compose_file="${script_dir}/compose.yaml"
otlp_endpoint="${KIRO_TELEMETRY_OTLP_ENDPOINT:-http://localhost:4318}"
prometheus_endpoint="${PROMETHEUS_ENDPOINT:-http://localhost:9090}"
fresh="${FRESH:-1}"            # 1 => `down -v` then `up` for a clean slate
keep_stack="${KEEP_STACK:-0}"  # 1 => skip teardown at the end

export KIRO_TELEMETRY_OTLP_ENDPOINT="${otlp_endpoint}"
export KIRO_TELEMETRY_ENABLED="${KIRO_TELEMETRY_ENABLED:-1}"
export KIRO_STATE_DIR="${KIRO_STATE_DIR:-${TMPDIR:-/tmp}/kiro-metrics-e2e}"
# Must NOT be 'true' or the TUI v3 observer self-suppresses real emits.
unset KIRO_TEST_MODE || true

log() { printf '\n=== %s ===\n' "$*"; }

compose() { ${compose_cmd} -f "${compose_file}" "$@"; }

teardown() {
  if [ "${keep_stack}" = "1" ]; then
    log "KEEP_STACK=1 — leaving stack up"
    return
  fi
  log "Tearing the stack down"
  compose down >/dev/null 2>&1 || true
}

# ---------------------------------------------------------------------------
# 1. Bring up the stack.
# ---------------------------------------------------------------------------
if ! ${compose_cmd} version >/dev/null 2>&1; then
  echo "container runtime '${compose_cmd}' not available" >&2
  exit 3
fi

if [ "${fresh}" = "1" ]; then
  log "Bringing up the local telemetry stack (fresh: down -v then up -d)"
  compose down -v >/dev/null 2>&1 || true
else
  log "Bringing up the local telemetry stack (reuse: up -d)"
fi
compose up -d

trap teardown EXIT

log "Waiting for Prometheus + collector to be reachable"
for attempt in {1..30}; do
  if curl -fsS -o /dev/null "${prometheus_endpoint}/api/v1/query?query=up" \
     && curl -fsS -o /dev/null "http://localhost:9464/metrics"; then
    echo "stack reachable"
    break
  fi
  [ "${attempt}" -eq 30 ] && { echo "stack did not come up in time" >&2; exit 4; }
  sleep 2
done

# ---------------------------------------------------------------------------
# 2. Emit from BOTH emitters through the real code paths.
# ---------------------------------------------------------------------------
log "Emitting Rust launcher catalog metrics via the OTel SDK (catalog_smoke)"
cargo run -q -p kiro-telemetry --features test-support --example catalog_smoke \
  --manifest-path "${repo_root}/Cargo.toml"

log "Emitting TUI chat_cli_v3 metrics via the real emitOtlpMetric path (bun)"
( cd "${repo_root}/packages/tui" \
  && bun run "${script_dir}/emit-tui-metrics.fixture.ts" )

# ---------------------------------------------------------------------------
# 3. Assert each metric NAME appears in Prometheus with >=1 sample.
# ---------------------------------------------------------------------------
# Prometheus naming after the OTLP prometheus exporter:
#   - a counter whose OTLP name already ends in `_total` is kept verbatim
#     (kiro_cli.session.completed -> kiro_cli_session_completed_total,
#      kiro_cli_user_logged_in_total, kiro_cli_auth_credential_failure_total,
#      kiro_cli_chat_session_started_total, kiro_cli_tool_call_total).
#   - a counter NOT ending in `_total` gets `_total` appended
#     (kiro_cli_session_started_total -> kiro_cli_session_started_total_total,
#      kiro_cli_daily_heartbeat -> kiro_cli_daily_heartbeat_total,
#      kiro_cli_user_turns -> kiro_cli_user_turns_total).
#   - a histogram exposes `_bucket` / `_sum` / `_count`.
# Each assertion query is anchored so a base name cannot false-match a longer
# series.
declare -a RUST_METRICS=(
  "kiro_cli_session_started_total"
  "kiro_cli_session_completed_total"
  "kiro_cli_user_logged_in_total"
  "kiro_cli_daily_heartbeat"
  "kiro_cli_auth_credential_failure_total"
)
declare -a TUI_METRICS=(
  # §C1 existing 4
  "kiro_cli_chat_session_started_total"   # counter, already _total -> verbatim
  "kiro_cli_user_turns"                   # counter -> _total appended
  "kiro_cli_user_turn_duration_seconds"   # histogram -> _bucket/_sum/_count
  "kiro_cli_tool_call_total"              # counter, already _total -> verbatim
  # §C4 product metrics backfilled on the V3 path
  "kiro_cli_tokens_consumed"              # counter -> kiro_cli_tokens_consumed_total
  "kiro_cli_model_invocations_total"      # counter, already _total -> verbatim
  "kiro_cli_turn_outcome_total"           # counter, already _total -> verbatim
  "kiro_cli_tool_execution_duration_ms"   # histogram -> _bucket/_sum/_count
  "kiro_cli_context_usage_percentage"     # GAUGE -> verbatim
  "kiro_cli_mode_active_total"            # counter, already _total -> verbatim
  "kiro_cli_subagent_delegations_total"   # counter, already _total -> verbatim
  # §E perf metrics promoted from the process-health log (dots -> underscores)
  "kiro_cli_process_memory_rss"           # GAUGE
  "kiro_cli_process_memory_peak_rss"      # GAUGE
  "kiro_cli_process_memory_heap_used"     # GAUGE
  "kiro_cli_process_cpu_utilization"      # histogram -> _bucket/_sum/_count
  "kiro_cli_tui_event_loop_delay"         # histogram -> _bucket/_sum/_count
  "kiro_cli_tui_input_latency"            # histogram -> _bucket/_sum/_count
  "kiro_cli_tui_render_duration"          # histogram -> _bucket/_sum/_count
)

assert_metric() {
  # Returns 0 if at least one series matches the base name (+ known suffixes).
  local base="$1"
  local query="count({__name__=~\"^${base}(_total|_total_total|_bucket|_sum|_count)?\$\"})"
  local response
  response="$(curl -fsS --get "${prometheus_endpoint}/api/v1/query" \
    --data-urlencode "query=${query}" || true)"
  [ "$(printf '%s' "${response}" | jq -r '.data.result | length')" != "0" ]
}

print_evidence() {
  local base="$1"
  local query="{__name__=~\"^${base}(_total|_total_total|_bucket|_sum|_count)?\$\"}"
  curl -fsS --get "${prometheus_endpoint}/api/v1/query" \
    --data-urlencode "query=${query}" \
    | jq -c '.data.result[] | {name: .metric.__name__, labels: (.metric | del(.__name__,.instance,.job,.exported_job,.otel_scope_schema_url,.otel_scope_version)), value: .value[1]}'
}

missing=0
miss_list=""

assert_group() {
  local label="$1"; shift
  log "Asserting ${label} metrics in Prometheus (${prometheus_endpoint})"
  for base in "$@"; do
    # Scrape interval is 5s; retry for up to ~36s to absorb export+scrape lag.
    local ok=1
    for attempt in {1..12}; do
      if assert_metric "${base}"; then ok=0; break; fi
      sleep 3
    done
    if [ "${ok}" -eq 0 ]; then
      echo "OBSERVED  ${base}"
      print_evidence "${base}"
    else
      echo "MISSING   ${base}"
      missing=$((missing + 1))
      miss_list="${miss_list} ${base}"
    fi
  done
}

assert_group "Rust launcher" "${RUST_METRICS[@]}"
assert_group "TUI (chat_cli_v3)" "${TUI_METRICS[@]}"

# ---------------------------------------------------------------------------
# 3b. Assert the engine label splits the SAME series into v2 + v3 (§H.6).
#
# The TUI now emits its own client-experience view of BOTH engines on the same
# Prometheus series, distinguished only by the `engine` label. Prove that:
#   (a) the client-experience set carries engine="v2" AND engine="v3" (both
#       >=1 sample) on a shared series — kiro_cli_user_turns_total is the
#       canonical witness;
#   (b) the economics metrics (tokens/cost/context) carry engine="v3" ONLY and
#       NEVER engine="v2" — they are host-authoritative on v2 and must not be
#       double-counted by the client.
# ---------------------------------------------------------------------------
# Count samples for a fully-qualified PromQL selector (incl. an engine matcher).
# Echoes the integer sample count (0 on any error / empty vector).
sample_count() {
  local selector="$1"
  local response
  response="$(curl -fsS --get "${prometheus_endpoint}/api/v1/query" \
    --data-urlencode "query=count(${selector})" || true)"
  printf '%s' "${response}" | jq -r '.data.result[0].value[1] // "0"'
}

# Retry wrapper: PromQL selector must yield >=1 sample within ~36s.
expect_present() {
  local label="$1" selector="$2"
  for attempt in {1..12}; do
    if [ "$(sample_count "${selector}")" != "0" ]; then
      echo "OBSERVED  ${label}  ->  ${selector}"
      return 0
    fi
    sleep 3
  done
  echo "MISSING   ${label}  ->  ${selector}"
  missing=$((missing + 1))
  miss_list="${miss_list} ${label}"
  return 1
}

# Must be ABSENT: selector must yield 0 samples across the full retry window.
# (We poll the whole window so a late export can't sneak a false-absent past us.)
expect_absent() {
  local label="$1" selector="$2"
  for attempt in {1..12}; do
    local n
    n="$(sample_count "${selector}")"
    if [ "${n}" != "0" ]; then
      echo "UNEXPECTED ${label}  ->  ${selector}  (count=${n})"
      missing=$((missing + 1))
      miss_list="${miss_list} ${label}"
      return 1
    fi
    sleep 3
  done
  echo "ABSENT    ${label}  ->  ${selector}  (correct: never appeared)"
  return 0
}

log "Asserting the engine label splits client-experience series into v2 + v3"
expect_present 'kiro_cli_user_turns_total{engine="v2"}' \
  'kiro_cli_user_turns_total{engine="v2"}'
expect_present 'kiro_cli_user_turns_total{engine="v3"}' \
  'kiro_cli_user_turns_total{engine="v3"}'
# A second witness from the client-experience set, also split on engine.
expect_present 'kiro_cli_tool_call_total{engine="v2"}' 'kiro_cli_tool_call_total{engine="v2"}'
expect_present 'kiro_cli_tool_call_total{engine="v3"}' 'kiro_cli_tool_call_total{engine="v3"}'

log "Asserting economics metrics are engine=v3 ONLY (no engine=v2 double-count)"
# Present on v3...
expect_present 'kiro_cli_tokens_consumed_total{engine="v3"}' \
  'kiro_cli_tokens_consumed_total{engine="v3"}'
expect_present 'kiro_cli_context_usage_percentage{engine="v3"}' \
  'kiro_cli_context_usage_percentage{engine="v3"}'
# ...and ABSENT on v2.
expect_absent 'kiro_cli_tokens_consumed_total{engine="v2"}' \
  'kiro_cli_tokens_consumed_total{engine="v2"}'
expect_absent 'kiro_cli_context_usage_percentage{engine="v2"}' \
  'kiro_cli_context_usage_percentage{engine="v2"}'

# ---------------------------------------------------------------------------
# 4. Verdict (teardown runs via the EXIT trap).
# ---------------------------------------------------------------------------
if [ "${missing}" -eq 0 ]; then
  log "PASS — all $(( ${#RUST_METRICS[@]} + ${#TUI_METRICS[@]} )) metric names observed, engine={v2,v3} both present on client-experience series, and economics stayed engine=v3-only"
  exit 0
fi

log "FAIL — ${missing} check(s) failed in Prometheus:${miss_list}"
echo "Inspect collector logs: ${compose_cmd} -f ${compose_file} logs otel-collector" >&2
exit 1

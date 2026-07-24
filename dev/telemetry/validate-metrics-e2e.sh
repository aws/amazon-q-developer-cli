#!/usr/bin/env bash
# End-to-end metric validation against the LOCAL dev telemetry stack (NOT prod
# KUTS). Proves that metrics from all three production paths land in Prometheus
# when KIRO_TELEMETRY_OTLP_ENDPOINT points at the dev collector:
#
#   1. V1 metrics from a real non-interactive chat_cli invocation in dual-write
#      mode, with both OTLP and legacy Toolkit requests recorded:
#        - CLI + chat lifecycle
#        - chat message + user turn
#        - process RSS, peak RSS, and CPU utilization
#        - cached user identity and service.version attribution
#
#   2. Rust metric-constructor/exporter coverage (via
#      crates/kiro-telemetry catalog_smoke):
#        - kiro_cli_session_started_total
#        - kiro_cli.session.completed   (-> kiro_cli_session_completed_total)
#        - kiro_cli_user_logged_in_total
#        - kiro_cli_daily_heartbeat
#        - kiro_cli_auth_credential_failure_total
#
#   3. TUI recorder/exporter coverage
#      (dev/telemetry/emit-tui-metrics.fixture.ts) — §C1 + §C4 product metrics +
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
# The V1 metrics run through a real product session. The Rust catalog and TUI
# fixture cover constructors and exporters independently. OTLP metrics are
# asserted via Prometheus and legacy Toolkit requests via the local recorder.
#
# The script is idempotent: it wipes prior stack state, emits, asserts each
# metric NAME is present with at least one sample, prints the JSON result
# vectors as evidence, and (unless KEEP_STACK=1) tears the stack down.
#
# Runtime: finch compose (per dev/telemetry/README.md). Override with
# COMPOSE_CMD="docker compose" if needed.
#
# Usage:
#   bash dev/telemetry/validate-metrics-e2e.sh
#   KEEP_STACK=1 bash dev/telemetry/validate-metrics-e2e.sh   # leave stack up
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"

compose_cmd="${COMPOSE_CMD:-finch compose}"
compose_file="${script_dir}/compose.yaml"
otlp_endpoint="${KIRO_TELEMETRY_OTLP_ENDPOINT:-http://127.0.0.1:4318}"
prometheus_endpoint="${PROMETHEUS_ENDPOINT:-http://localhost:9090}"
keep_stack="${KEEP_STACK:-0}"  # 1 => skip teardown at the end

export KIRO_TELEMETRY_OTLP_ENDPOINT="${otlp_endpoint}"
export KIRO_TELEMETRY_ENABLED="${KIRO_TELEMETRY_ENABLED:-1}"
readonly run_dir="$(mktemp -d "${TMPDIR:-/tmp}/kiro-metrics-e2e.XXXXXX")"
export KIRO_STATE_DIR="${run_dir}/state"
readonly v1_state_dir="${run_dir}/v1"
v1_database="${v1_state_dir}/data.sqlite3"
v1_otlp_capture="${v1_state_dir}/otlp-requests.jsonl"
v1_toolkit_capture="${v1_state_dir}/toolkit-requests.jsonl"
v1_machine_id="5e813456-3b09-43f4-9b37-cb4c163d42c7"
run_id="$(date +%s)-$$"
v1_user_id="v1-e2e-user-${run_id}"
v1_version="0.0.0-v1-e2e-${run_id}"
v1_otlp_proxy_port="${KIRO_V1_OTLP_PROXY_PORT:-14318}"
v1_toolkit_proxy_port="${KIRO_V1_TOOLKIT_PROXY_PORT:-14319}"
capture_proxy_pid=""
stack_started=0
# Must NOT be 'true' or the TUI v3 observer self-suppresses real emits.
unset KIRO_TEST_MODE || true

log() { printf '\n=== %s ===\n' "$*"; }

compose() { ${compose_cmd} -f "${compose_file}" "$@"; }

teardown() {
  if [ -n "${capture_proxy_pid}" ]; then
    kill "${capture_proxy_pid}" >/dev/null 2>&1 || true
    wait "${capture_proxy_pid}" >/dev/null 2>&1 || true
  fi
  rm -rf "${run_dir}"
  if [ "${stack_started}" != "1" ]; then
    return
  fi
  if [ "${keep_stack}" = "1" ]; then
    log "KEEP_STACK=1 — leaving stack up"
    return
  fi
  log "Tearing the stack down"
  compose down >/dev/null 2>&1 || true
}

trap teardown EXIT

# ---------------------------------------------------------------------------
# 1. Bring up the stack.
# ---------------------------------------------------------------------------
if ! ${compose_cmd} version >/dev/null 2>&1; then
  echo "container runtime '${compose_cmd}' not available" >&2
  exit 3
fi

log "Bringing up the local telemetry stack (fresh: down -v then up -d)"
compose down -v >/dev/null 2>&1 || true
stack_started=1
compose up -d

log "Waiting for Prometheus, Grafana, and the collector to be reachable"
for attempt in {1..30}; do
  if curl -fsS -o /dev/null "${prometheus_endpoint}/api/v1/query?query=up" \
     && curl -fsS -o /dev/null "http://localhost:3000/api/health" \
     && curl -fsS -o /dev/null "http://localhost:9464/metrics"; then
    echo "stack reachable"
    break
  fi
  [ "${attempt}" -eq 30 ] && { echo "stack did not come up in time" >&2; exit 4; }
  sleep 2
done

# ---------------------------------------------------------------------------
# 2. Emit from all production paths through the real code paths.
# ---------------------------------------------------------------------------
log "Running a real V1 non-interactive chat with OTel and legacy Toolkit dual-write"
mkdir -p "${KIRO_STATE_DIR}" "${v1_state_dir}/kiro-home" "${v1_state_dir}/sessions"

# Initialize the isolated database without emitting telemetry, then seed the
# cached user identity and non-production credentials used by the local
# Toolkit request recorder.
KIRO_DISABLE_TELEMETRY=1 \
KIRO_TEST_MODE=1 \
KIRO_TEST_DB_PATH="${v1_database}" \
KIRO_HOME="${v1_state_dir}/kiro-home" \
cargo run -q -p chat_cli --bin chat_cli \
  --no-default-features --features legacy_codewhisperer_sink,legacy_toolkit_sink -- version >/dev/null
sqlite3 "${v1_database}" \
  "INSERT OR REPLACE INTO state (key, value) VALUES ('telemetryUserId', '\"${v1_user_id}\"');"
sqlite3 "${v1_database}" \
  "INSERT OR REPLACE INTO state (key, value) VALUES (
    'telemetry-cognito-credentials',
    '{\"access_key_id\":\"v1-e2e-access-key\",\"secret_key\":\"v1-e2e-secret-key\",\"session_token\":\"v1-e2e-session-token\",\"expiration\":\"2099-01-01T00:00:00Z\"}'
  );"
# Database initialization invokes `version`, which records today's heartbeat
# even with telemetry disabled. Clear it so the real chat owns the assertion.
sqlite3 "${v1_database}" \
  "DELETE FROM state WHERE key = 'telemetry.lastHeartbeatDate';"

KIRO_OTLP_CAPTURE_UPSTREAM="${otlp_endpoint}" \
KIRO_OTLP_CAPTURE_FILE="${v1_otlp_capture}" \
KIRO_OTLP_CAPTURE_PORT="${v1_otlp_proxy_port}" \
KIRO_TOOLKIT_CAPTURE_FILE="${v1_toolkit_capture}" \
KIRO_TOOLKIT_CAPTURE_PORT="${v1_toolkit_proxy_port}" \
bun run "${script_dir}/capture-v1-telemetry.ts" &
capture_proxy_pid=$!
for attempt in {1..20}; do
  if curl -fsS -o /dev/null "http://127.0.0.1:${v1_otlp_proxy_port}/health" \
     && curl -fsS -o /dev/null "http://127.0.0.1:${v1_toolkit_proxy_port}/health"; then
    break
  fi
  [ "${attempt}" -eq 20 ] && { echo "V1 telemetry capture proxy did not start" >&2; exit 5; }
  sleep 1
done

KIRO_TEST_MODE=1 \
KIRO_TEST_DB_PATH="${v1_database}" \
KIRO_TEST_SESSIONS_DIR="${v1_state_dir}/sessions" \
KIRO_HOME="${v1_state_dir}/kiro-home" \
KIRO_TELEMETRY_OTEL=1 \
KIRO_TELEMETRY_OTLP_ENDPOINT="http://127.0.0.1:${v1_otlp_proxy_port}" \
KIRO_TELEMETRY_TOOLKIT_ENDPOINT="http://127.0.0.1:${v1_toolkit_proxy_port}" \
KIRO_TELEMETRY_EXPORT_INTERVAL_MS=600000 \
KIRO_TELEMETRY_CLIENT_ID="${v1_machine_id}" \
KIRO_MOCK_CHAT_RESPONSE="${script_dir}/v1-chat-response.json" \
KIRO_VERSION_OVERRIDE="${v1_version}" \
cargo run -q -p chat_cli --bin chat_cli \
  --no-default-features --features legacy_codewhisperer_sink,legacy_toolkit_sink -- \
  chat --agent-engine=v1 --no-interactive "validate V1 telemetry"

jq -se --arg machine_id "${v1_machine_id}" \
  'length > 0 and all(.[]; .method == "POST" and .path == "/v1/metrics" and .machine_id == $machine_id)' \
  "${v1_otlp_capture}" >/dev/null
echo "OBSERVED  V1 metrics-only POST requests with x-kiro-machineid transport header"

jq -se --arg client_id "${v1_machine_id}" '
  def occurrences($name):
    [.[] | .metric_names[] | select(. == $name)] | length;
  length > 0
  and all(.[]; .method == "POST" and .path == "/metrics" and .client_id == $client_id)
  and occurrences("amazonqcli_dailyHeartbeat") == 1
  and occurrences("codewhispererterminal_cliSubcommandExecuted") == 1
  and occurrences("codewhispererterminal_agentConfigInit") == 1
  and occurrences("amazonq_startChat") == 1
  and occurrences("codewhispererterminal_addChatMessage") == 1
  and occurrences("codewhispererterminal_recordUserTurnCompletion") == 1
  and occurrences("amazonq_endChat") == 1
' "${v1_toolkit_capture}" >/dev/null
echo "OBSERVED  seven V1 Toolkit parity events exactly once"

log "Emitting Rust launcher catalog metrics via the OTel SDK (catalog_smoke)"
cargo run -q -p kiro-telemetry --features test-support --example catalog_smoke \
  --manifest-path "${repo_root}/Cargo.toml"

log "Emitting TUI chat_cli_v3 metrics via the real emitOtlpMetric path (bun)"
( cd "${repo_root}/packages/tui" \
  && bun run "${script_dir}/emit-tui-metrics.fixture.ts" )

# ---------------------------------------------------------------------------
# 3. Assert each metric NAME appears in Prometheus with >=1 sample.
# ---------------------------------------------------------------------------
# Prometheus naming after the pinned OTLP prometheus exporter:
#   - counter names ending in `_total` remain unchanged;
#   - other counters receive `_total`;
#   - gauges remain unchanged;
#   - histograms expose `_bucket`, `_sum`, and `_count`.
# Every declaration includes the metric kind so validation can require the
# exact exported series instead of accepting permissive name fallbacks.
declare -a RUST_METRICS=(
  "kiro_cli_session_started_total counter"
  "kiro_cli_session_completed_total counter"
  "kiro_cli_user_logged_in_total counter"
  "kiro_cli_daily_heartbeat counter"
  "kiro_cli_auth_credential_failure_total counter"
)
declare -a V1_METRICS=(
  "kiro_cli_session_started_total counter"
  "kiro_cli_daily_heartbeat counter"
  "kiro_cli_feature_used_total counter"
  "kiro_cli_agent_config_init_total counter"
  "kiro_cli_chat_session_started_total counter"
  "kiro_cli_chat_messages_total counter"
  "kiro_cli_user_turns counter"
  "kiro_cli_conversation_completed_total counter"
  "kiro_cli_process_memory_rss gauge"
  "kiro_cli_process_memory_peak_rss gauge"
  "kiro_cli_process_cpu_utilization histogram"
  "kiro_cli_session_completed_total counter"
)
declare -a TUI_METRICS=(
  # §C1 existing 4
  "kiro_cli_chat_session_started_total counter"
  "kiro_cli_user_turns counter"
  "kiro_cli_user_turn_duration_seconds histogram"
  "kiro_cli_tool_call_total counter"
  # §C4 product metrics backfilled on the V3 path
  "kiro_cli_tokens_consumed counter"
  "kiro_cli_model_invocations_total counter"
  "kiro_cli_turn_outcome_total counter"
  "kiro_cli_tool_execution_duration_ms histogram"
  "kiro_cli_context_usage_percentage gauge"
  "kiro_cli_mode_active_total counter"
  "kiro_cli_subagent_delegations_total counter"
  # §E perf metrics promoted from the process-health log (dots -> underscores)
  "kiro_cli_process_memory_rss gauge"
  "kiro_cli_process_memory_peak_rss gauge"
  "kiro_cli_process_memory_heap_used gauge"
  "kiro_cli_process_cpu_utilization histogram"
  "kiro_cli_tui_event_loop_delay histogram"
  "kiro_cli_tui_input_latency histogram"
  "kiro_cli_tui_render_duration histogram"
)

metric_series_names() {
  local base="$1" kind="$2"
  case "${kind}" in
    counter)
      if [[ "${base}" == *_total ]]; then
        printf '%s\n' "${base}"
      else
        printf '%s\n' "${base}_total"
      fi
      ;;
    gauge)
      printf '%s\n' "${base}"
      ;;
    histogram)
      printf '%s\n' "${base}_bucket" "${base}_sum" "${base}_count"
      ;;
    *)
      echo "unknown metric kind '${kind}' for ${base}" >&2
      return 2
      ;;
  esac
}

assert_metric() {
  local base="$1" kind="$2" labels="${3:-}" name matcher query response count
  while IFS= read -r name; do
    matcher="__name__=\"${name}\""
    if [ -n "${labels}" ]; then
      matcher="${matcher},${labels}"
    fi
    query="count({${matcher}})"
    if ! response="$(prometheus_query "${query}")"; then
      return 2
    fi
    if ! count="$(printf '%s' "${response}" | jq -er \
      'if length == 0 then "0"
       elif length == 1 then .[0].value[1]
       else error("expected one Prometheus count result")
       end')"; then
      return 2
    fi
    if [ "${count}" = "0" ]; then
      return 1
    fi
  done < <(metric_series_names "${base}" "${kind}")
}

prometheus_query() {
  local query="$1" response
  if ! response="$(curl -fsS --get "${prometheus_endpoint}/api/v1/query" \
    --data-urlencode "query=${query}")"; then
    echo "Prometheus query failed: ${query}" >&2
    return 2
  fi
  if ! printf '%s' "${response}" | jq -ce \
    'if .status == "success" and (.data.result | type == "array")
     then .data.result
     else error(.error // "invalid Prometheus response")
     end'; then
    echo "Invalid Prometheus response for: ${query}" >&2
    return 2
  fi
}

print_evidence() {
  local base="$1" kind="$2" labels="${3:-}" name matcher query response
  while IFS= read -r name; do
    matcher="__name__=\"${name}\""
    if [ -n "${labels}" ]; then
      matcher="${matcher},${labels}"
    fi
    query="{${matcher}}"
    response="$(prometheus_query "${query}")"
    printf '%s' "${response}" \
      | jq -ec '.[] | {name: .metric.__name__, labels: (.metric | del(.__name__,.instance,.job,.exported_job,.otel_scope_schema_url,.otel_scope_version)), value: .value[1]}'
  done < <(metric_series_names "${base}" "${kind}")
}

missing=0
miss_list=""

assert_group() {
  local label="$1" labels="$2"; shift 2
  log "Asserting ${label} metrics in Prometheus (${prometheus_endpoint})"
  local spec base kind status
  for spec in "$@"; do
    read -r base kind <<< "${spec}"
    if assert_metric "${base}" "${kind}" "${labels}"; then
      echo "OBSERVED  ${base}"
      print_evidence "${base}" "${kind}" "${labels}"
    else
      status=$?
      if [ "${status}" -ne 1 ]; then
        exit "${status}"
      fi
      echo "MISSING   ${base}"
      missing=$((missing + 1))
      miss_list="${miss_list} ${base}"
    fi
  done
}

wait_for_metric_ingestion() {
  local deadline=$((SECONDS + 45)) status
  log "Waiting for one completed scrape from each metric emitter"
  while [ "${SECONDS}" -lt "${deadline}" ]; do
    status=0
    assert_metric "kiro_cli_conversation_completed_total" counter \
      "otel_scope_name=\"kiro-telemetry\",engine=\"v1\",service_version=\"${v1_version}\"" || status=$?
    if [ "${status}" -eq 0 ]; then
      assert_metric "kiro_cli_auth_credential_failure_total" counter \
        'otel_scope_name="kiro-telemetry-catalog-smoke"' || status=$?
    fi
    if [ "${status}" -eq 0 ]; then
      assert_metric "kiro_cli_tui_render_duration" histogram \
        'otel_scope_name="kiro.tui",engine="v3"' || status=$?
    fi
    if [ "${status}" -eq 0 ]; then
      echo "all emitter sentinels observed"
      return
    fi
    if [ "${status}" -ne 1 ]; then
      exit "${status}"
    fi
    sleep 3
  done
  echo "metric ingestion did not complete within 45 seconds" >&2
  return 1
}

wait_for_metric_ingestion
assert_group "V1 binary" \
  "otel_scope_name=\"kiro-telemetry\",engine=\"v1\",service_version=\"${v1_version}\"" \
  "${V1_METRICS[@]}"
assert_group "Rust launcher catalog" \
  'otel_scope_name="kiro-telemetry-catalog-smoke"' \
  "${RUST_METRICS[@]}"
assert_group "TUI (chat_cli_v3)" \
  'otel_scope_name="kiro.tui",engine=~"v2|v3"' \
  "${TUI_METRICS[@]}"

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
# Echoes the integer sample count.
sample_count() {
  local selector="$1"
  local response
  response="$(prometheus_query "count(${selector})")" || return 2
  printf '%s' "${response}" | jq -er \
    'if length == 0 then "0"
     elif length == 1 then .[0].value[1]
     else error("expected one Prometheus count result")
     end'
}

sample_value() {
  local query="$1" response count
  response="$(prometheus_query "${query}")" || return 2
  count="$(printf '%s' "${response}" | jq -er 'length')" || return 2
  if [ "${count}" = "0" ]; then
    return 1
  fi
  if [ "${count}" != "1" ]; then
    echo "Expected exactly one Prometheus result for: ${query}" >&2
    return 2
  fi
  printf '%s' "${response}" | jq -er '.[0].value[1]'
}

expect_present() {
  local label="$1" selector="$2" n
  if ! n="$(sample_count "${selector}")"; then
    exit 2
  fi
  if [ "${n}" != "0" ]; then
    echo "OBSERVED  ${label}  ->  ${selector}"
    return 0
  fi
  echo "MISSING   ${label}  ->  ${selector}"
  missing=$((missing + 1))
  miss_list="${miss_list} ${label}"
  return 0
}

expect_absent() {
  local label="$1" selector="$2" n
  if ! n="$(sample_count "${selector}")"; then
    exit 2
  fi
  if [ "${n}" != "0" ]; then
    echo "UNEXPECTED ${label}  ->  ${selector}  (count=${n})"
    missing=$((missing + 1))
    miss_list="${miss_list} ${label}"
    return 0
  fi
  echo "ABSENT    ${label}  ->  ${selector}  (correct: never appeared)"
  return 0
}

expect_exact() {
  local label="$1" query="$2" expected="$3" actual="" status
  if actual="$(sample_value "${query}")"; then
    if [ "${actual}" = "${expected}" ]; then
      echo "OBSERVED  ${label}=${expected}  ->  ${query}"
      return 0
    fi
  else
    status=$?
    if [ "${status}" -ne 1 ]; then
      exit "${status}"
    fi
  fi
  echo "MISMATCH  ${label}: expected=${expected} actual=${actual:-missing}  ->  ${query}"
  missing=$((missing + 1))
  miss_list="${miss_list} ${label}"
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
# 3c. Assert the real V1 binary carries the dimensions used by adoption and
# process dashboards. The transport-level x-kiro-machineid header is checked by
# the capture proxy because Prometheus intentionally does not turn request
# headers into metric dimensions.
# ---------------------------------------------------------------------------
log "Asserting V1 lifecycle, identity, version, and process dimensions"
v1_scope_labels="otel_scope_name=\"kiro-telemetry\""
v1_identity_labels="engine=\"v1\",user_id=\"${v1_user_id}\",service_version=\"${v1_version}\""
v1_process_labels="${v1_identity_labels},agent_kind=\"v1\",process_role=\"host\""
expect_absent 'V1 series with missing or incorrect user identity' \
  "{${v1_scope_labels},engine=\"v1\",service_version=\"${v1_version}\",user_id!=\"${v1_user_id}\"}"
expect_absent 'V1 series with missing or incorrect service version' \
  "{${v1_scope_labels},engine=\"v1\",user_id=\"${v1_user_id}\",service_version!=\"${v1_version}\"}"
expect_absent 'Engine-scoped V1 series with missing or incorrect engine' \
  "{${v1_scope_labels},user_id=\"${v1_user_id}\",service_version=\"${v1_version}\",__name__!=\"kiro_cli_consent_record_integrity_total\",engine!=\"v1\"}"
expect_exact 'V1 CLI session starts' \
  "sum(kiro_cli_session_started_total{${v1_identity_labels},client_application=\"chat_cli\"})" 1
expect_exact 'V1 daily heartbeats' \
  "sum(kiro_cli_daily_heartbeat_total{${v1_identity_labels}})" 1
expect_exact 'V1 CLI subcommands' \
  "sum(kiro_cli_feature_used_total{${v1_identity_labels},feature=\"chat\"})" 1
expect_exact 'V1 agent config initializations' \
  "sum(kiro_cli_agent_config_init_total{${v1_identity_labels}})" 1
expect_exact 'V1 newly-created chat sessions' \
  "sum(kiro_cli_chat_session_started_total{${v1_identity_labels},session_start_kind=\"new\"})" 1
expect_exact 'V1 chat messages' \
  "sum(kiro_cli_chat_messages_total{${v1_identity_labels}})" 1
expect_exact 'V1 user turns' \
  "sum(kiro_cli_user_turns_total{${v1_identity_labels}})" 1
expect_exact 'V1 conversation completions' \
  "sum(kiro_cli_conversation_completed_total{${v1_identity_labels}})" 1
expect_exact 'V1 RSS series' \
  "count(kiro_cli_process_memory_rss{${v1_process_labels}})" 1
expect_exact 'V1 peak RSS series' \
  "count(kiro_cli_process_memory_peak_rss{${v1_process_labels}})" 1
expect_exact 'V1 final process samples' \
  "sum(kiro_cli_process_cpu_utilization_count{${v1_process_labels}})" 1
expect_present 'V1 CPU histogram buckets' \
  "kiro_cli_process_cpu_utilization_bucket{${v1_process_labels}}"
expect_present 'V1 CPU histogram sum' \
  "kiro_cli_process_cpu_utilization_sum{${v1_process_labels}}"
expect_exact 'V1 clean CLI completions' \
  "sum(kiro_cli_session_completed_total{${v1_identity_labels},agent_kind=\"v1\",exit_reason=\"clean\"})" 1
expect_absent 'V1 TUI-only process metrics' \
  "{${v1_identity_labels},__name__=~\"kiro_cli_process_memory_heap_used|kiro_cli_tui_event_loop_delay(_bucket|_sum|_count)?|kiro_cli_tui_input_latency(_bucket|_sum|_count)?|kiro_cli_tui_render_duration(_bucket|_sum|_count)?\"}"

# ---------------------------------------------------------------------------
# 3d. Prove Grafana loaded the provisioned dashboard and its Prometheus
# datasource is healthy; this catches provisioning/query-path drift.
# ---------------------------------------------------------------------------
log "Asserting Grafana provisioning and Prometheus datasource health"
curl -fsS "http://localhost:3000/api/datasources/uid/prometheus/health" \
  | jq -e '.status == "OK"' >/dev/null
dashboard_response="$(curl -fsS "http://localhost:3000/api/dashboards/uid/kiro-telemetry-local")"
printf '%s' "${dashboard_response}" \
  | jq -e '.dashboard.uid == "kiro-telemetry-local"' >/dev/null

assert_grafana_panel_query() {
  local title="$1" expected="${2:-}" targets ref_id datasource_uid query payload response target=0
  targets="$(printf '%s' "${dashboard_response}" | jq -cer --arg title "${title}" \
    '[.dashboard.panels[] | select(.title == $title) | .targets[]
      | [.refId, .datasource.uid, .expr]]
     | select(length > 0)')"
  while IFS=$'\t' read -r ref_id datasource_uid query; do
    target=$((target + 1))
    query="${query//\$instance/.+}"
    query="${query//\$__range/5m}"
    payload="$(jq -cn \
      --arg ref_id "${ref_id}" \
      --arg datasource_uid "${datasource_uid}" \
      --arg query "${query}" \
      '{
        from: "now-5m",
        to: "now",
        queries: [{
          refId: $ref_id,
          datasource: {uid: $datasource_uid, type: "prometheus"},
          expr: $query,
          format: "time_series",
          instant: true,
          range: false,
          intervalMs: 5000,
          maxDataPoints: 1000
        }]
      }')"
    response="$(curl -fsS \
      -H "Content-Type: application/json" \
      --data "${payload}" \
      "http://localhost:3000/api/ds/query")"
    if ! printf '%s' "${response}" | jq -e --arg ref_id "${ref_id}" --arg expected "${expected}" '
      .results[$ref_id] as $result
      | ($result.error? == null)
        and (($result.frames // []) | length > 0)
        and ([($result.frames // [])[].data.values[1][]? | select(. != null)] as $values
          | ($values | length > 0)
            and all($values[];
              (type == "number")
              and ($expected == "" or . == ($expected | tonumber))))
    ' >/dev/null; then
      echo "Grafana query returned no finite data: ${title} target ${target}" >&2
      printf '%s\n' "${response}" >&2
      exit 2
    fi
    echo "OBSERVED  Grafana panel: ${title} target ${target}"
  done < <(printf '%s' "${targets}" | jq -r '.[] | @tsv')
}

assert_grafana_panel_query "V1 Authenticated Users Seen" 1
assert_grafana_panel_query "V1 New Chat Sessions" 1
for title in "V1 Host RSS" "V1 Host CPU Mean"; do
  assert_grafana_panel_query "${title}"
done
echo "OBSERVED  Grafana dashboard, datasource, and V1 panel queries"

# ---------------------------------------------------------------------------
# 4. Verdict (teardown runs via the EXIT trap).
# ---------------------------------------------------------------------------
if [ "${missing}" -eq 0 ]; then
  log "PASS — all $(( ${#V1_METRICS[@]} + ${#RUST_METRICS[@]} + ${#TUI_METRICS[@]} )) metric names observed, seven real V1 Toolkit/OTel event pairs verified exactly once, identity/lifecycle/process dimensions verified, engine={v2,v3} split preserved, and Grafana V1 queries validated"
  exit 0
fi

log "FAIL — ${missing} check(s) failed in Prometheus:${miss_list}"
echo "Inspect collector logs: ${compose_cmd} -f ${compose_file} logs otel-collector" >&2
exit 1

#!/usr/bin/env bash
# Validates the reviewed metric catalog and production emitters against the
# local collector, Prometheus, and Grafana stack. The real V1 binary runs in
# dual-write mode so its OTLP records and legacy Toolkit parity are both
# exercised.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"

compose_cmd="${COMPOSE_CMD:-finch compose}"
compose_file="${script_dir}/compose.yaml"
otlp_endpoint="${KIRO_TELEMETRY_OTLP_ENDPOINT:-http://127.0.0.1:4318}"
prometheus_endpoint="${PROMETHEUS_ENDPOINT:-http://localhost:9090}"
grafana_endpoint="${GRAFANA_ENDPOINT:-http://localhost:3000}"
fresh="${FRESH:-1}"
keep_stack="${KEEP_STACK:-0}"

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
tui_version="0.0.0-e2e"
v1_user_id="v1-e2e-user-${run_id}"
v1_version="0.0.0-v1-e2e-${run_id}"
v1_otlp_proxy_port="${KIRO_V1_OTLP_PROXY_PORT:-14318}"
v1_toolkit_proxy_port="${KIRO_V1_TOOLKIT_PROXY_PORT:-14319}"
capture_proxy_pid=""
stack_started=0
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
    log "KEEP_STACK=1: leaving stack up"
    return
  fi
  log "Tearing the stack down"
  compose down >/dev/null 2>&1 || true
}

if ! ${compose_cmd} version >/dev/null 2>&1; then
  echo "container runtime '${compose_cmd}' not available" >&2
  exit 3
fi

log "Checking Grafana query-generation invariants"
bun test "${script_dir}/grafana/tests/generator.test.mjs"

log "Checking generated Grafana dashboards"
bun "${script_dir}/grafana/generate-dashboards.mjs" --check

if [ "${fresh}" = "1" ]; then
  log "Bringing up the local telemetry stack with fresh storage"
  compose down -v >/dev/null 2>&1 || true
else
  log "Bringing up the local telemetry stack with existing storage"
fi
stack_started=1
compose up -d
trap teardown EXIT

log "Waiting for Prometheus, Grafana, and the collector"
for attempt in {1..30}; do
  if curl -fsS -o /dev/null "${prometheus_endpoint}/api/v1/query?query=up" \
     && curl -fsS -o /dev/null "${grafana_endpoint}/api/health" \
     && curl -fsS -o /dev/null "http://localhost:9464/metrics"; then
    echo "stack reachable"
    break
  fi
  [ "${attempt}" -eq 30 ] && {
    echo "stack did not come up in time" >&2
    exit 4
  }
  sleep 2
done

log "Running a real V1 one-shot chat with OTel and legacy Toolkit dual-write"
mkdir -p "${KIRO_STATE_DIR}" "${v1_state_dir}/kiro-home" "${v1_state_dir}/sessions"

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
sqlite3 "${v1_database}" \
  "DELETE FROM state WHERE key LIKE 'telemetry.lastHeartbeatDate.%';"

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
  [ "${attempt}" -eq 20 ] && {
    echo "V1 telemetry capture proxy did not start" >&2
    exit 5
  }
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

if ! jq -se --arg client_id "${v1_machine_id}" '
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
' "${v1_toolkit_capture}" >/dev/null; then
  echo "MISMATCH  V1 Toolkit parity events" >&2
  jq -s '[.[].metric_names[]] | group_by(.) | map({name: .[0], count: length})' \
    "${v1_toolkit_capture}" >&2
  exit 1
fi
echo "OBSERVED  seven V1 Toolkit parity events exactly once"

log "Emitting all typed catalog records through the Rust OTLP SDK"
cargo run -q -p kiro-telemetry --features test-support --example catalog_smoke \
  --manifest-path "${repo_root}/Cargo.toml"

log "Emitting production TUI observer records through the JS OTLP SDK"
(
  cd "${repo_root}/packages/tui"
  bun run "${script_dir}/emit-tui-metrics.fixture.ts"
)

log "Emitting coherent synthetic installations for dashboard filter coverage"
KIRO_DASHBOARD_DEMO_WAVES=3 \
KIRO_DASHBOARD_DEMO_INTERVAL_MS=5500 \
KIRO_DASHBOARD_DEMO_EMIT_HEARTBEATS=1 \
cargo run -q -p kiro-telemetry --features test-support --example dashboard_demo \
  --manifest-path "${repo_root}/Cargo.toml"

catalog_metrics="$(
  awk '
    /^  - name:/ { name=$3 }
    /^    kind:/ {
      if ($2=="counter" || $2=="histogram" || $2=="observable_gauge") print name
    }
  ' "${repo_root}/crates/kiro-telemetry-schema/schema/metrics.yaml"
)"

tui_metrics="
kiro_cli_chat_session_started_total
kiro_cli_ui_mode_session_started_total
kiro_cli_slash_command_invoked_total
kiro_cli_cloud_session_lifecycle_total
kiro_cli_cloud_session_ready_seconds
kiro_cli_config_panel_total
kiro_cli_cloud_config_diagnostic_total
kiro_cli_cloud_config_source_total
kiro_cli_time_to_first_visible_response_ms
kiro_cli_user_turns
kiro_cli_user_turn_duration_seconds
kiro_cli_turn_failure_total
kiro_cli_turn_cancelled_total
kiro_cli_tokens_consumed
kiro_cli_model_invocations_total
kiro_cli_credits_consumed
kiro_cli_tool_call_total
kiro_cli_tool_execution_duration_ms
kiro_cli_process_memory_rss_bytes
kiro_cli_process_peak_rss_bytes
kiro_cli_tui_heap_used_bytes
kiro_cli_process_open_file_descriptor_count
kiro_cli_process_handle_count
kiro_cli_process_thread_count
kiro_cli_process_cpu_utilization_ratio
kiro_cli_tui_event_loop_delay_p99_seconds
kiro_cli_tui_input_to_render_p95_seconds
kiro_cli_tui_render_duration_seconds
kiro_cli_workflow_run_total
kiro_cli_workflow_run_duration_seconds
kiro_cli_workflow_node_total
kiro_cli_workflow_node_duration_seconds
kiro_cli_workflow_control_total
kiro_cli_workflow_restore_total
kiro_cli_workflow_concurrent_runs
"

metric_selector() {
  local base="${1//./_}"
  local scope="$2"
  printf '{__name__=~"^%s(_total|_total_total|_bucket|_sum|_count)?$",otel_scope_name="%s"}' \
    "${base}" "${scope}"
}

sample_count() {
  local selector="$1"
  local response
  response="$(curl -fsS --get "${prometheus_endpoint}/api/v1/query" \
    --data-urlencode "query=count(${selector})" || true)"
  printf '%s' "${response}" | jq -r '.data.result[0].value[1] // "0"'
}

missing=0
miss_list=""

expect_present() {
  local label="$1"
  local selector="$2"
  for attempt in {1..12}; do
    if [ "$(sample_count "${selector}")" != "0" ]; then
      echo "OBSERVED  ${label}"
      return 0
    fi
    [ "${attempt}" -lt 12 ] && sleep 3
  done
  echo "MISSING   ${label} -> ${selector}"
  missing=$((missing + 1))
  miss_list="${miss_list} ${label}"
  return 0
}

expect_absent() {
  local label="$1"
  local selector="$2"
  local count
  count="$(sample_count "${selector}")"
  if [ "${count}" != "0" ]; then
    echo "UNEXPECTED ${label} -> ${selector} (count=${count})"
    missing=$((missing + 1))
    miss_list="${miss_list} ${label}"
    return 0
  fi
  echo "ABSENT    ${label}"
}

sample_value() {
  local query="$1"
  local response
  response="$(curl -fsS --get "${prometheus_endpoint}/api/v1/query" \
    --data-urlencode "query=${query}")"
  printf '%s' "${response}" | jq -r '.data.result[0].value[1] // "<absent>"'
}


wait_for_exact() {
  local label="$1"
  local query="$2"
  local expected="$3"
  local actual
  for attempt in {1..12}; do
    actual="$(sample_value "${query}")"
    if [ "${actual}" = "${expected}" ]; then
      echo "OBSERVED  ${label}=${expected}"
      return
    fi
    [ "${attempt}" -lt 12 ] && sleep 3
  done
  echo "READINESS TIMEOUT  ${label}: expected=${expected} actual=${actual}" >&2
  exit 2
}

expect_exact() {
  local label="$1"
  local query="$2"
  local expected="$3"
  local actual
  actual="$(sample_value "${query}")"
  if [ "${actual}" = "${expected}" ]; then
    echo "OBSERVED  ${label}=${expected}"
    return
  fi
  echo "MISMATCH  ${label}: expected=${expected} actual=${actual}"
  missing=$((missing + 1))
  miss_list="${miss_list} ${label}"
}

demo_scope='otel_scope_name="kiro-telemetry-dashboard-demo"'
demo_readiness_query="sum(kiro_cli_chat_session_started_total{${demo_scope},version_full=\"2.7.0\",agent_engine=\"v2\",session_interface=\"interactive_cli\",agent_mode=\"plan\"})"
wait_for_exact "dashboard demo third wave" "${demo_readiness_query}" 18

log "Asserting all catalog records came from the Rust catalog emitter"
while IFS= read -r metric; do
  [ -z "${metric}" ] && continue
  expect_present "${metric}" \
    "$(metric_selector "${metric}" "kiro-telemetry-catalog-smoke")"
done <<< "${catalog_metrics}"

log "Asserting the production TUI observer emitted its owned metrics"
while IFS= read -r metric; do
  [ -z "${metric}" ] && continue
  expect_present "${metric}{otel_scope_name=kiro.tui}" \
    "$(metric_selector "${metric}" "kiro.tui")"
done <<< "${tui_metrics}"

log "Asserting shared TUI metrics split cleanly by agent engine"
expect_present 'user turns for agent_engine=v2' \
  'kiro_cli_user_turns_total{otel_scope_name="kiro.tui",agent_engine="v2"}'
expect_present 'user turns for agent_engine=v3' \
  'kiro_cli_user_turns_total{otel_scope_name="kiro.tui",agent_engine="v3"}'
expect_present 'first visible response for agent_engine=v2' \
  'kiro_cli_time_to_first_visible_response_ms_count{otel_scope_name="kiro.tui",agent_engine="v2"}'
expect_present 'first visible response for agent_engine=v3' \
  'kiro_cli_time_to_first_visible_response_ms_count{otel_scope_name="kiro.tui",agent_engine="v3"}'
expect_exact 'V2 /settings invocations' \
  "sum(kiro_cli_slash_command_invoked_total{otel_scope_name=\"kiro.tui\",version_full=\"${tui_version}\",agent_engine=\"v2\",slash_command=\"/settings\"})" 1
expect_exact 'V3 /settings invocations' \
  "sum(kiro_cli_slash_command_invoked_total{otel_scope_name=\"kiro.tui\",version_full=\"${tui_version}\",agent_engine=\"v3\",slash_command=\"/settings\"})" 1
expect_exact 'V3 /upgrade-agent invocations' \
  "sum(kiro_cli_slash_command_invoked_total{otel_scope_name=\"kiro.tui\",version_full=\"${tui_version}\",agent_engine=\"v3\",slash_command=\"/upgrade-agent\"})" 1
expect_exact 'V3 /workflow-run invocations' \
  "sum(kiro_cli_slash_command_invoked_total{otel_scope_name=\"kiro.tui\",version_full=\"${tui_version}\",agent_engine=\"v3\",slash_command=\"/workflow-run\"})" 1
expect_exact 'V3 /goal invocations' \
  "sum(kiro_cli_slash_command_invoked_total{otel_scope_name=\"kiro.tui\",version_full=\"${tui_version}\",agent_engine=\"v3\",slash_command=\"/goal\"})" 1

log "Asserting KAS-authoritative economics are not duplicated by the v2 TUI"
expect_present 'tokens for agent_engine=v3' \
  'kiro_cli_tokens_consumed_total{otel_scope_name="kiro.tui",agent_engine="v3"}'
expect_absent 'tokens for agent_engine=v2' \
  'kiro_cli_tokens_consumed_total{otel_scope_name="kiro.tui",agent_engine="v2"}'
expect_present 'model invocations for agent_engine=v3' \
  'kiro_cli_model_invocations_total{otel_scope_name="kiro.tui",agent_engine="v3"}'
expect_absent 'model invocations for agent_engine=v2' \
  'kiro_cli_model_invocations_total{otel_scope_name="kiro.tui",agent_engine="v2"}'

case "$(uname -s)" in
  Darwin) v1_os_type="macos" ;;
  Linux) v1_os_type="linux" ;;
  MINGW* | MSYS* | CYGWIN*) v1_os_type="windows" ;;
  *) v1_os_type="unknown" ;;
esac
# The telemetry pipeline pseudonymizes the raw service user id before it reaches
# any wire format (kiro-telemetry `TelemetryConfig::with_user_id`): the DB is
# seeded with the raw id above, but every emitted datapoint carries the stable
# pseudonym "v1:" + base64url(SHA256("kiro-tui-telemetry-user-id:v1\0" || raw)).
# Mirror that transform so the V1 identity assertions match what is emitted.
pseudonymous_user_id() {
  local pseudo
  pseudo="$({ printf 'kiro-tui-telemetry-user-id:v1'; printf '\0'; printf '%s' "$1"; } \
    | openssl dgst -sha256 -binary | openssl base64 -A | tr '+/' '-_' | tr -d '=')"
  printf 'v1:%s' "${pseudo}"
}
v1_user_id_pseudonym="$(pseudonymous_user_id "${v1_user_id}")"
v1_scope="otel_scope_name=\"kiro-telemetry\",version_full=\"${v1_version}\",user_id=\"${v1_user_id_pseudonym}\""
v1_run="${v1_scope},agent_engine=\"v1\",session_interface=\"noninteractive_cli\",os_type=\"${v1_os_type}\""
v1_process="${v1_scope},agent_engine=\"v1\",os_type=\"${v1_os_type}\",process_role=\"host\""

log "Asserting the real V1 producer uses the reviewed metric contract"
expect_exact "V1 run starts" "sum(kiro_cli_run_started_total{${v1_run}})" 1
expect_exact "V1 daily heartbeats" \
  "sum(kiro_cli_daily_heartbeat_total{${v1_scope},os_type=\"${v1_os_type}\"})" 1
expect_exact "V1 top-level chat commands" \
  "sum(kiro_cli_top_level_command_invoked_total{${v1_scope},top_level_command=\"chat\"})" 1
expect_exact "V1 chat sessions" \
  "sum(kiro_cli_chat_session_started_total{${v1_scope},agent_engine=\"v1\",session_interface=\"noninteractive_cli\",agent_mode=\"custom\"})" 1
expect_exact "V1 model invocations" \
  "sum(kiro_cli_model_invocations_total{${v1_scope},agent_engine=\"v1\"})" 1
expect_exact "V1 user turns" \
  "sum(kiro_cli_user_turns_total{${v1_scope},agent_engine=\"v1\",session_interface=\"noninteractive_cli\",agent_mode=\"custom\"})" 1
expect_exact "V1 successful startups" \
  "sum(kiro_cli_startup_duration_seconds_count{${v1_run}})" 1
expect_exact "V1 RSS series" \
  "count(kiro_cli_process_memory_rss_bytes{${v1_process}})" 1
expect_exact "V1 peak RSS observations" \
  "sum(kiro_cli_process_peak_rss_bytes_count{${v1_process}})" 1
expect_exact "V1 CPU observations" \
  "sum(kiro_cli_process_cpu_utilization_ratio_count{${v1_process}})" 1
expect_exact "V1 thread-count series" \
  "count(kiro_cli_process_thread_count{${v1_process}})" 1
if [ "${v1_os_type}" != "windows" ]; then
  expect_exact "V1 file-descriptor series" \
    "count(kiro_cli_process_open_file_descriptor_count{${v1_process}})" 1
fi
expect_exact "V1 successful run outcomes" \
  "sum(kiro_cli_run_outcome_total{${v1_run},run_outcome=\"success\"})" 1
expect_absent "V1 metrics with the wrong user identity" \
  "{otel_scope_name=\"kiro-telemetry\",version_full=\"${v1_version}\",user_id!=\"${v1_user_id_pseudonym}\"}"
expect_absent "retired V1 metric families" \
  "{otel_scope_name=\"kiro-telemetry\",version_full=\"${v1_version}\",__name__=~\"kiro_cli_session_started_total|kiro_cli_session_completed_total|kiro_cli_feature_used_total|kiro_cli_conversation_completed_total|kiro_cli_process_memory_rss|kiro_cli_process_memory_peak_rss|kiro_cli_process_cpu_utilization_(bucket|sum|count)\"}"

log "Asserting representative dashboard filter combinations"
expect_exact "dashboard demo daily active installations" \
  "sum(kiro_cli_daily_heartbeat_total{${demo_scope}})" 7
expect_exact "stable 2.7.0 daily active installations" \
  "sum(kiro_cli_daily_heartbeat_total{${demo_scope},version_full=\"2.7.0\"})" 2
expect_exact "nightly 2.8.0 daily active installations" \
  "sum(kiro_cli_daily_heartbeat_total{${demo_scope},version_full=\"2.8.0-nightly.14\"})" 2
expect_exact "beta 2.8.0 daily active installations" \
  "sum(kiro_cli_daily_heartbeat_total{${demo_scope},version_full=\"2.8.0-beta.2\"})" 1
expect_exact "stable 2.6.3 daily active installations" \
  "sum(kiro_cli_daily_heartbeat_total{${demo_scope},version_full=\"2.6.3\"})" 1
expect_exact "development daily active installations" \
  "sum(kiro_cli_daily_heartbeat_total{${demo_scope},version_full=\"0.0.0-dev\"})" 1
expect_exact "stable Windows heartbeats with unknown install source" \
  "sum(kiro_cli_daily_heartbeat_total{${demo_scope},version_full=\"2.7.0\",release_channel=\"stable\",os_type=\"windows\",install_method=\"unknown\"})" 1
expect_exact "V2 interactive plan sessions" \
  "sum(kiro_cli_chat_session_started_total{${demo_scope},version_full=\"2.7.0\",agent_engine=\"v2\",session_interface=\"interactive_cli\",agent_mode=\"plan\"})" 18
expect_exact "V3 nightly Opus requests" \
  "sum(kiro_cli_model_invocations_total{${demo_scope},version_full=\"2.8.0-nightly.14\",agent_engine=\"v3\",model=\"claude-opus-4.1\"})" 15
expect_exact "subagent-context tool calls" \
  "sum(kiro_cli_tool_call_total{${demo_scope},agent_engine=\"v3\",tool_origin=\"builtin\",execution_context=\"subagent\"})" 27
expect_exact "ACP-injected MCP failures" \
  "sum(kiro_cli_mcp_server_init_total{${demo_scope},agent_engine=\"v3\",mcp_server_source=\"acp_injected\",mcp_init_outcome=\"failure\"})" 15
expect_exact "KAS subprocess memory series" \
  "count(kiro_cli_process_memory_rss_bytes{${demo_scope},agent_engine=\"v3\",process_role=\"kas_subprocess\"})" 1

log "Asserting Grafana provisioning and reviewed dashboard queries"
curl -fsS "${grafana_endpoint}/api/datasources/uid/prometheus/health" \
  | jq -e '.status == "OK"' >/dev/null
detailed_dashboard_response="$(curl -fsS "${grafana_endpoint}/api/dashboards/uid/kiro-telemetry-local")"
health_dashboard_response="$(curl -fsS "${grafana_endpoint}/api/dashboards/uid/kiro-telemetry-health-local")"
printf '%s' "${detailed_dashboard_response}" \
  | jq -e '
      .dashboard.uid == "kiro-telemetry-local"
      and ([.dashboard.panels[] | select(.type != "row")] | length == 50)
      and ([.dashboard.templating.list[].name] == [
        "version_full",
        "agent_engine",
        "slash_command",
        "session_interface",
        "agent_mode",
        "os_type",
        "model",
        "tool_origin",
        "execution_context",
        "mcp_server_source",
        "process_role",
        "instance"
      ])
      and any(.dashboard.panels[];
        .title == "Slash commands invoked"
        and any(.targets[];
          (.expr | contains("$version_full"))
          and (.expr | contains("$agent_engine"))
          and (.expr | contains("$slash_command"))
        )
      )
    ' >/dev/null
printf '%s' "${health_dashboard_response}" \
  | jq -e '
      .dashboard.uid == "kiro-telemetry-health-local"
      and ([.dashboard.panels[] | select(.type != "row")] | length == 33)
      and ([.dashboard.templating.list[].name] == [
        "version_full",
        "release_channel",
        "agent_engine",
        "session_interface",
        "agent_mode",
        "os_type",
        "install_method",
        "model",
        "mcp_server_source",
        "process_role",
        "auth_method",
        "instance"
      ])
    ' >/dev/null

source "${script_dir}/dashboard-validation.sh"

assert_dashboard_queries_populated "detailed dashboard" "${detailed_dashboard_response}"
assert_dashboard_queries_populated "health dashboard" "${health_dashboard_response}"
assert_dashboard_semantics "${detailed_dashboard_response}" "${health_dashboard_response}"

assert_grafana_panel_query "${detailed_dashboard_response}" "Runs started by engine"
assert_grafana_panel_query "${detailed_dashboard_response}" "Chat sessions by engine"
assert_grafana_panel_query "${detailed_dashboard_response}" "Process RSS by engine and role"
assert_grafana_panel_query "${detailed_dashboard_response}" "CPU utilization mean by engine and role"
assert_grafana_panel_query "${health_dashboard_response}" "Daily active installations for selected version"
assert_grafana_panel_query "${health_dashboard_response}" "Client-turn availability by engine (%)"
assert_grafana_panel_query "${health_dashboard_response}" "Startup availability by engine (%)"
assert_grafana_panel_query "${health_dashboard_response}" "Startup duration mean by engine"

log "Asserting slash-command panel filter"
assert_grafana_panel_query \
  "${detailed_dashboard_response}" "Slash commands invoked" "" slash "/workflow-run"

grafana_prometheus_proxy="${grafana_endpoint}/api/datasources/proxy/uid/prometheus"
grafana_slash_response="$(
  curl -fsS --get "${grafana_prometheus_proxy}/api/v1/query" \
    --data-urlencode "query=sum(kiro_cli_slash_command_invoked_total{version_full=\"${tui_version}\",agent_engine=\"v3\",slash_command=\"/workflow-run\"})"
)"
printf '%s' "${grafana_slash_response}" \
  | jq -e '.status == "success" and .data.result[0].value[1] == "1"' >/dev/null
grafana_command_values="$(
  curl -fsS --get "${grafana_prometheus_proxy}/api/v1/label/slash_command/values" \
    --data-urlencode "match[]=kiro_cli_slash_command_invoked_total{version_full=\"${tui_version}\",agent_engine=\"v3\"}"
)"
printf '%s' "${grafana_command_values}" \
  | jq -e '.status == "success" and (.data | contains(["/settings", "/upgrade-agent", "/workflow-run", "/goal"]))' >/dev/null
echo "OBSERVED  Grafana slash-command panel, filters, and V3 series through the datasource proxy"

log "Asserting selected-window queries survive a collector reset"
compose restart otel-collector >/dev/null
for attempt in {1..30}; do
  if curl -fsS -o /dev/null "http://localhost:9464/metrics"; then
    break
  fi
  [ "${attempt}" -eq 30 ] && {
    echo "collector did not recover after restart" >&2
    exit 4
  }
  sleep 2
done
KIRO_DASHBOARD_DEMO_WAVES=4 \
KIRO_DASHBOARD_DEMO_INTERVAL_MS=5500 \
KIRO_DASHBOARD_DEMO_EMIT_HEARTBEATS=0 \
cargo run -q -p kiro-telemetry --features test-support --example dashboard_demo \
  --manifest-path "${repo_root}/Cargo.toml"
wait_for_exact "post-reset dashboard demo fourth wave" "${demo_readiness_query}" 24
wait_for_exact "post-reset heartbeat counter remains at baseline" \
  "sum(kiro_cli_daily_heartbeat_total{${demo_scope}})" 0
heartbeat_utc_day_response="$(grafana_query \
  HEARTBEAT_DAY prometheus \
  "sum(increase(kiro_cli_daily_heartbeat_total{${demo_scope}}[\$__range]))" \
  true false now/d)"
printf '%s' "${heartbeat_utc_day_response}" | jq -e '
  [.results.HEARTBEAT_DAY.frames[].data.values[1][]? | select(type == "number")] as $values
  | ($values | length == 1) and (($values[0] | round) == 7)
' >/dev/null
echo "OBSERVED  UTC-day heartbeat total remains 7 after collector reset"
assert_grafana_panel_query "${detailed_dashboard_response}" "Runs started by engine"
assert_grafana_panel_query "${detailed_dashboard_response}" "Startup duration mean by engine"
assert_grafana_panel_query "${health_dashboard_response}" "Client-turn availability by engine (%)"
echo "OBSERVED  selected-window counters, histograms, and ratios across collector reset"

catalog_count="$(printf '%s\n' "${catalog_metrics}" | grep -c . || true)"
tui_count="$(printf '%s\n' "${tui_metrics}" | grep -c . || true)"
if [ "${missing}" -eq 0 ]; then
  log "PASS: ${catalog_count} catalog metrics, ${tui_count} TUI metrics, real V1 dual-write, dashboard scenarios, and populated Grafana queries validated"
  exit 0
fi

log "FAIL: ${missing} check(s) failed:${miss_list}"
echo "Inspect collector logs: ${compose_cmd} -f ${compose_file} logs otel-collector" >&2
exit 1

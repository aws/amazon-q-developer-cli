#!/usr/bin/env bash
# Starts the local telemetry stack, emits coherent synthetic installations, and
# verifies dashboard coverage plus representative filter combinations.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
compose_cmd="${COMPOSE_CMD:-finch compose}"
compose_file="${script_dir}/compose.yaml"
prometheus_endpoint="${PROMETHEUS_ENDPOINT:-http://localhost:9090}"
grafana_endpoint="${GRAFANA_ENDPOINT:-http://localhost:3000}"
otlp_endpoint="${KIRO_TELEMETRY_OTLP_ENDPOINT:-http://127.0.0.1:4318}"
fresh="${FRESH:-1}"
run_dir="$(mktemp -d "${TMPDIR:-/tmp}/kiro-dashboard-demo.XXXXXX")"

export KIRO_TELEMETRY_OTLP_ENDPOINT="${otlp_endpoint}"
export KIRO_TELEMETRY_ENABLED=1
export KIRO_STATE_DIR="${run_dir}/state"

log() { printf '\n=== %s ===\n' "$*"; }
compose() { ${compose_cmd} -f "${compose_file}" "$@"; }
cleanup() { rm -rf "${run_dir}"; }
trap cleanup EXIT

case "${fresh}" in
  0 | 1) ;;
  *)
    echo "FRESH must be 0 or 1; got '${fresh}'" >&2
    exit 2
    ;;
esac

if ! ${compose_cmd} version >/dev/null 2>&1; then
  echo "container runtime '${compose_cmd}' not available" >&2
  exit 3
fi

log "Checking generated dashboards"
bun "${script_dir}/grafana/generate-dashboards.mjs" --check

if [ "${fresh}" = "1" ]; then
  log "Resetting local telemetry storage"
  compose down -v >/dev/null 2>&1 || true
fi

log "Starting Prometheus, Grafana, and the collector"
compose up -d
for attempt in {1..30}; do
  if curl -fsS -o /dev/null "${prometheus_endpoint}/api/v1/query?query=up" \
     && curl -fsS -o /dev/null "${grafana_endpoint}/api/health" \
     && curl -fsS -o /dev/null "http://localhost:9464/metrics"; then
    break
  fi
  [ "${attempt}" -eq 30 ] && {
    echo "local telemetry stack did not become ready" >&2
    exit 4
  }
  sleep 2
done

log "Emitting the dashboard scenario matrix"
KIRO_DASHBOARD_DEMO_EMIT_HEARTBEATS="${fresh}" \
cargo run -q -p kiro-telemetry --features test-support --example dashboard_demo \
  --manifest-path "${repo_root}/Cargo.toml"

log "Waiting for Prometheus to scrape the generated series"
for attempt in {1..12}; do
  observed="$(curl -fsS --get "${prometheus_endpoint}/api/v1/query" \
    --data-urlencode 'query=count({otel_scope_name="kiro-telemetry-dashboard-demo"})' \
    | jq -r '.data.result[0].value[1] // "0"')"
  [ "${observed}" != "0" ] && break
  [ "${attempt}" -eq 12 ] && {
    echo "dashboard demo series did not reach Prometheus" >&2
    exit 5
  }
  sleep 2
done

expect_label_values() {
  local metric="$1"
  local label="$2"
  local expected="$3"
  local query response actual expected_sorted
  query="count by (${label}) (${metric}{otel_scope_name=\"kiro-telemetry-dashboard-demo\"})"
  response="$(curl -fsS --get "${prometheus_endpoint}/api/v1/query" \
    --data-urlencode "query=${query}")"
  actual="$(printf '%s' "${response}" | jq -r --arg label "${label}" \
    '.data.result[].metric[$label]' | LC_ALL=C sort | tr '\n' ' ' | sed 's/ $//')"
  expected_sorted="$(tr ' ' '\n' <<< "${expected}" | LC_ALL=C sort | tr '\n' ' ' | sed 's/ $//')"
  if [ "${actual}" != "${expected_sorted}" ]; then
    echo "Unexpected ${label} values on ${metric}" >&2
    echo "expected: ${expected_sorted}" >&2
    echo "actual:   ${actual}" >&2
    exit 6
  fi
  echo "OBSERVED  ${label}: ${actual}"
}

expect_series() {
  local label="$1"
  local query="$2"
  local value
  value="$(curl -fsS --get "${prometheus_endpoint}/api/v1/query" \
    --data-urlencode "query=${query}" \
    | jq -r '.data.result[0].value[1] // "0"')"
  if [ "${value}" = "0" ]; then
    echo "Missing filtered scenario: ${label}" >&2
    echo "${query}" >&2
    exit 7
  fi
  echo "OBSERVED  ${label}"
}

log "Checking every dashboard selector vocabulary"
expect_label_values kiro_cli_daily_heartbeat_total version_full \
  "0.0.0-dev 2.6.3 2.7.0 2.8.0-beta.2 2.8.0-nightly.14"
expect_label_values kiro_cli_daily_heartbeat_total release_channel \
  "beta nightly stable unknown"
expect_label_values kiro_cli_run_started_total agent_engine \
  "unknown v1 v2 v3"
expect_label_values kiro_cli_slash_command_invoked_total command \
  "/custom /goal /help /model /settings /upgrade-agent /workflow-run"
expect_label_values kiro_cli_run_started_total session_interface \
  "external_acp interactive_cli noninteractive_cli"
expect_label_values kiro_cli_chat_session_started_total agent_mode \
  "autonomous custom default plan spec"
expect_label_values kiro_cli_daily_heartbeat_total os_type \
  "linux macos unknown windows"
expect_label_values kiro_cli_daily_heartbeat_total install_method \
  "brew internal_toolbox unknown"
expect_label_values kiro_cli_model_invocations_total model \
  "auto claude-haiku-4 claude-opus-4.1 claude-sonnet-3.7 claude-sonnet-4 claude-sonnet-4.5 unknown"
expect_label_values kiro_cli_tool_call_total tool_origin \
  "builtin mcp unknown"
expect_label_values kiro_cli_tool_call_total execution_context \
  "main subagent"
expect_label_values kiro_cli_mcp_server_init_total mcp_server_source \
  "acp_injected agent global registry unknown workspace"
expect_label_values kiro_cli_process_memory_rss_bytes process_role \
  "host kas_subprocess tui"
expect_label_values kiro_cli_login_success_total auth_method \
  "builder_id external_idp identity_center social unknown"

log "Checking coherent cross-dimension filter combinations"
expect_series "stable Windows adoption with unknown install source" \
  'sum(kiro_cli_daily_heartbeat_total{version_full="2.7.0",release_channel="stable",os_type="windows",install_method="unknown"})'
expect_series "V2 interactive plan sessions" \
  'sum(kiro_cli_chat_session_started_total{version_full="2.7.0",agent_engine="v2",session_interface="interactive_cli",agent_mode="plan"})'
expect_series "V3 nightly Opus requests" \
  'sum(kiro_cli_model_invocations_total{version_full="2.8.0-nightly.14",agent_engine="v3",model="claude-opus-4.1"})'
expect_series "subagent-context tool calls" \
  'sum(kiro_cli_tool_call_total{agent_engine="v3",tool_origin="builtin",execution_context="subagent"})'
expect_series "ACP-injected MCP failures" \
  'sum(kiro_cli_mcp_server_init_total{agent_engine="v3",mcp_server_source="acp_injected",mcp_init_outcome="failure"})'
expect_series "KAS subprocess memory" \
  'count(kiro_cli_process_memory_rss_bytes{agent_engine="v3",process_role="kas_subprocess"})'

source "${script_dir}/dashboard-validation.sh"

detailed_dashboard_response="$(curl -fsS "${grafana_endpoint}/api/dashboards/uid/kiro-telemetry-local")"
health_dashboard_response="$(curl -fsS "${grafana_endpoint}/api/dashboards/uid/kiro-telemetry-health-local")"

log "Checking every Grafana panel target with all selectors enabled"
assert_dashboard_queries_populated "detailed dashboard" "${detailed_dashboard_response}"
assert_dashboard_queries_populated "health dashboard" "${health_dashboard_response}"
assert_dashboard_semantics "${detailed_dashboard_response}" "${health_dashboard_response}"

log "Dashboard demo is ready"
echo "Detailed: ${grafana_endpoint}/d/kiro-telemetry-local/kiro-cli-local-telemetry"
echo "Health:   ${grafana_endpoint}/d/kiro-telemetry-health-local/kiro-cli-local-health"

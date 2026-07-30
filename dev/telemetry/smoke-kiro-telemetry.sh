#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"

otlp_endpoint="${KIRO_TELEMETRY_OTLP_ENDPOINT:-http://localhost:4318}"
prometheus_endpoint="${PROMETHEUS_ENDPOINT:-http://localhost:9090}"

export KIRO_TELEMETRY_ENABLED="${KIRO_TELEMETRY_ENABLED:-1}"
export KIRO_TELEMETRY_OTEL="${KIRO_TELEMETRY_OTEL:-2}"
export KIRO_TELEMETRY_OTLP_ENDPOINT="${otlp_endpoint}"
export KIRO_TELEMETRY_EXPORT_INTERVAL_MS="${KIRO_TELEMETRY_EXPORT_INTERVAL_MS:-1000}"
export KIRO_STATE_DIR="${KIRO_STATE_DIR:-${TMPDIR:-/tmp}/kiro-telemetry-local-smoke}"

cargo run -p kiro-telemetry --example local_smoke \
  --manifest-path "${repo_root}/Cargo.toml"

metrics=(
  'kiro_cli_run_started_total'
  'kiro_cli_chat_session_started_total'
  'kiro_cli_model_invocations_total'
  'kiro_cli_user_turns'
  'kiro_cli_user_turn_duration_seconds'
  'kiro_cli_run_outcome_total'
)

missing=0
for metric in "${metrics[@]}"; do
  observed=0
  for attempt in {1..24}; do
    base="${metric//./_}"
    query="count({__name__=~\"^${base}(_total|_total_total|_bucket|_sum|_count)?\$\",otel_scope_name=\"kiro-telemetry-local-smoke\"})"
    response="$(curl -fsS --get "${prometheus_endpoint}/api/v1/query" \
      --data-urlencode "query=${query}")"
    if [ "$(printf '%s' "${response}" | jq -r '.data.result[0].value[1] // "0"')" != "0" ]; then
      echo "Prometheus observed ${metric} from ${prometheus_endpoint}"
      observed=1
      break
    fi
    sleep 2
  done
  if [ "${observed}" -eq 0 ]; then
    echo "Missing ${metric}" >&2
    missing=$((missing + 1))
  fi
done

if [ "${missing}" -ne 0 ]; then
  echo "Prometheus did not observe ${missing} kiro-telemetry smoke metric(s)." >&2
  echo "Check collector logs with: finch compose logs -f otel-collector" >&2
  exit 1
fi

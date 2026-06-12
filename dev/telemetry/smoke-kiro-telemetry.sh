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

cargo run -p kiro-telemetry --example local_smoke --manifest-path "${repo_root}/Cargo.toml"

queries=(
  'feature_used_total{feature="local_smoke"}'
  'feature_used_total_total{feature="local_smoke"}'
  'chat_session_started_total{client_application="chat_cli_v3",mode="plan"}'
  'chat_session_started_total_total{client_application="chat_cli_v3",mode="plan"}'
  'chat_cli_session_completed_total{agent_kind="kas",exit_reason="clean"}'
)

for attempt in {1..24}; do
  for query in "${queries[@]}"; do
    response="$(curl -fsS --get "${prometheus_endpoint}/api/v1/query" --data-urlencode "query=${query}")"
    if printf "%s" "${response}" | grep -q '"result":\[{'; then
      echo "Prometheus observed ${query} from ${prometheus_endpoint}"
      exit 0
    fi
  done

  if [[ "${attempt}" -lt 24 ]]; then
    sleep 2
  fi
done

echo "Prometheus did not observe kiro-telemetry smoke metrics within 48 seconds." >&2
echo "Check collector logs with: finch compose logs -f otel-collector" >&2
exit 1

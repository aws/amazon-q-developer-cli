#!/usr/bin/env bash
# Emits one record for every metric/log in the §5 catalog via the catalog_smoke
# example, then asserts every metric instrument is observed in the local Prometheus.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"

otlp_endpoint="${KIRO_TELEMETRY_OTLP_ENDPOINT:-http://localhost:4318}"
prometheus_endpoint="${PROMETHEUS_ENDPOINT:-http://localhost:9090}"

export KIRO_TELEMETRY_OTLP_ENDPOINT="${otlp_endpoint}"
export KIRO_STATE_DIR="${KIRO_STATE_DIR:-${TMPDIR:-/tmp}/kiro-telemetry-catalog-verify}"

echo "Emitting full catalog to ${otlp_endpoint} ..."
cargo run -q -p kiro-telemetry --features test-support --example catalog_smoke \
  --manifest-path "${repo_root}/Cargo.toml"

# Every metric-instrument name in the catalog (counters/histograms/gauges).
# log_event and derived rows are excluded: KUTS is metrics-only (no logs), and
# derived rows are computed downstream.
metrics="$(
  awk '
    /^  - name:/ { name=$3 }
    /^    kind:/ {
      kind=$2
      if (kind=="counter" || kind=="histogram" || kind=="observable_gauge") print name
    }
  ' "${repo_root}/crates/kiro-telemetry-schema/schema/metrics.yaml"
)"

missing=0
for attempt in {1..12}; do
  missing=0
  miss_list=""
  while IFS= read -r metric; do
    [ -z "${metric}" ] && continue
    base="${metric//./_}"
    # Anchor to the exact base plus the known OTLP->Prometheus suffixes so a base
    # name cannot false-match a longer series (e.g. foo_bar vs foo_bar_baz).
    query="count({__name__=~\"^${base}(_total|_total_total|_bucket|_sum|_count)?\$\"})"
    response="$(curl -fsS --get "${prometheus_endpoint}/api/v1/query" --data-urlencode "query=${query}" || true)"
    # jq: non-empty result vector means at least one matching series exists.
    if [ "$(printf "%s" "${response}" | jq -r '.data.result | length')" = "0" ]; then
      missing=$((missing + 1))
      miss_list="${miss_list} ${metric}"
    fi
  done <<< "${metrics}"

  if [ "${missing}" -eq 0 ]; then
    total="$(printf "%s\n" "${metrics}" | grep -c . || true)"
    echo "All ${total} catalog metric instruments observed in Prometheus."
    exit 0
  fi
  [ "${attempt}" -lt 12 ] && sleep 3
done

echo "Missing ${missing} metric instrument(s) in Prometheus:${miss_list}" >&2
echo "Check collector logs: finch compose -f dev/telemetry/compose.yaml logs -f otel-collector" >&2
exit 1

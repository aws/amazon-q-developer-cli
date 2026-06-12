#!/usr/bin/env bash
set -euo pipefail

otlp_endpoint="${KIRO_TELEMETRY_OTLP_ENDPOINT:-http://localhost:4318}"
prometheus_endpoint="${PROMETHEUS_ENDPOINT:-http://localhost:9090}"
metric_name="${1:-kiro_cli_local_smoke}"
run_id="${KIRO_TELEMETRY_SMOKE_RUN_ID:-$(date +%s)-$$}"

if [[ ! "${metric_name}" =~ ^[a-zA-Z_:][a-zA-Z0-9_:]*$ ]]; then
  echo "metric name must be a valid Prometheus identifier: ${metric_name}" >&2
  exit 2
fi

now_seconds="$(date +%s)"
now_nanos="$((now_seconds * 1000000000))"

curl -fsS \
  -X POST "${otlp_endpoint}/v1/metrics" \
  -H "Content-Type: application/json" \
  --data-binary @- >/dev/null <<JSON
{
  "resourceMetrics": [
    {
      "resource": {
        "attributes": [
          {
            "key": "service.name",
            "value": {
              "stringValue": "kiro-cli-local-smoke"
            }
          }
        ]
      },
      "scopeMetrics": [
        {
          "scope": {
            "name": "dev.telemetry.smoke"
          },
          "metrics": [
            {
              "name": "${metric_name}",
              "sum": {
                "aggregationTemporality": 2,
                "isMonotonic": true,
                "dataPoints": [
                  {
                    "attributes": [
                      {
                        "key": "source",
                        "value": {
                          "stringValue": "dev.telemetry.smoke"
                        }
                      },
                      {
                        "key": "run_id",
                        "value": {
                          "stringValue": "${run_id}"
                        }
                      }
                    ],
                    "startTimeUnixNano": "${now_nanos}",
                    "timeUnixNano": "${now_nanos}",
                    "asInt": "1"
                  }
                ]
              }
            }
          ]
        }
      ]
    }
  ]
}
JSON

echo "sent OTLP metric ${metric_name} with run_id=${run_id} to ${otlp_endpoint}"

queries=(
  "${metric_name}{run_id=\"${run_id}\"}"
  "${metric_name}_total{run_id=\"${run_id}\"}"
  "${metric_name}_total_total{run_id=\"${run_id}\"}"
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

echo "Prometheus did not observe ${metric_name} within 48 seconds." >&2
echo "Check collector logs with: finch compose logs -f otel-collector" >&2
exit 1

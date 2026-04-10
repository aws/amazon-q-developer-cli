#!/usr/bin/env bash
# Usage: ./scripts/es-query.sh <index-pattern> <query-json>
# Example: ./scripts/es-query.sh 'metrics-2026-03-*' '{"size":0,"query":{"match_all":{}}}'
#
# Set ES_COOKIE env var or put cookies in scripts/.es-cookie file.

set -euo pipefail

INDEX="${1:?Usage: es-query.sh <index-pattern> <query-json>}"
QUERY="${2:?Usage: es-query.sh <index-pattern> <query-json>}"

BASE="https://telemetry-externalprod.ide-toolkits.dev-tools.aws.dev/_plugin/kibana/api/console/proxy"
ENCODED_INDEX=$(python3 -c "import urllib.parse; print(urllib.parse.quote('${INDEX}/_search', safe=''))")

COOKIE_FILE="$(dirname "$0")/.es-cookie"
if [[ -z "${ES_COOKIE:-}" && -f "$COOKIE_FILE" ]]; then
  ES_COOKIE=$(cat "$COOKIE_FILE")
fi
: "${ES_COOKIE:?Set ES_COOKIE env var or create $COOKIE_FILE}"

curl -s "${BASE}?path=${ENCODED_INDEX}&method=GET" \
  -H 'content-type: application/json' \
  -H 'kbn-xsrf: kibana' \
  -b "$ES_COOKIE" \
  -d "$QUERY" | python3 -m json.tool

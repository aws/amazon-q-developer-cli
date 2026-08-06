#!/usr/bin/env bash
# smoke-cut-ticket.sh — cut a SIM-T ticket via the Tickety API when the smoke
# judge verdict is "fail".
#
# Gated off by default: requires SMOKE_TICKETY_ENABLED=1 (the AWS account must
# be onboarded to Tickety via a Ticketing Application before enabling).
#
# Dedupe: a marker in the evidence S3 bucket records the fingerprint of the
# failing check set. A repeat failure with the same fingerprint within
# DEDUPE_DAYS comments on the existing ticket instead of cutting a new one.
#
# Env:
#   SMOKE_TICKETY_ENABLED  must be "1" to do anything; otherwise exits 0
#   VERDICT_FILE           path to judge-verdict.json (required)
#   GITHUB_RUN_ID          workflow run id, used for links
#   TICKETY_REGION         default us-west-2
#   DEDUPE_BUCKET          default kiro-reviewer-archive
#   DEDUPE_DAYS            default 7
set -euo pipefail

[ "${SMOKE_TICKETY_ENABLED:-0}" = "1" ] || { echo "tickety disabled; skipping"; exit 0; }
VERDICT_FILE="${VERDICT_FILE:?VERDICT_FILE is required}"
[ -f "$VERDICT_FILE" ] || { echo "no verdict file; skipping" >&2; exit 0; }

VERDICT=$(jq -r '.verdict // "unknown"' "$VERDICT_FILE")
[ "$VERDICT" = "fail" ] || { echo "verdict=$VERDICT; no ticket needed"; exit 0; }

REGION="${TICKETY_REGION:-us-west-2}"
BUCKET="${DEDUPE_BUCKET:-kiro-reviewer-archive}"
DEDUPE_DAYS="${DEDUPE_DAYS:-7}"
RUN_ID="${GITHUB_RUN_ID:-0}"
RUN_URL="https://github.com/kiro-team/kiro-cli/actions/runs/${RUN_ID}"
DASHBOARD_URL="https://kiro-bot-dashboard.beta.harmony.a2z.com/?tab=smoke&runId=smoke-${RUN_ID}"

SUMMARY=$(jq -r '.summary // "no summary"' "$VERDICT_FILE")
FAILING_LEGS=$(jq -r '
  [.legs[]? | select(.verdict == "fail")
    | "* \(.os)/\(.engine)/\(.ui_mode) — \(.summary // "no summary")"]
  | join("\n")' "$VERDICT_FILE")
CRITICAL=$(jq -r '
  [.critical_failures[]? | if type == "object" then (.reason // tojson) else tostring end
    | "* \(.)"]
  | join("\n")' "$VERDICT_FILE")

# Fingerprint of WHAT failed (legs + failing check names), not the run id, so
# the same breakage on consecutive nights maps to the same ticket.
FINGERPRINT=$(jq -r '
  [([.legs[]? | select(.verdict == "fail") | "\(.os)/\(.engine)/\(.ui_mode)"] | sort),
   ([.checks // {} | to_entries[] | select(.value.pass == false) | .key] | sort)]
  | tojson' "$VERDICT_FILE" | shasum -a 256 | cut -c1-16)
MARKER_KEY="smoke-tickets/open-ticket.json"
MARKER_LOCAL=$(mktemp)
trap 'rm -f "$MARKER_LOCAL"' EXIT

EXISTING_TICKET=""
if aws s3 cp "s3://${BUCKET}/${MARKER_KEY}" "$MARKER_LOCAL" >/dev/null 2>&1; then
  PRIOR_FP=$(jq -r '.fingerprint // empty' "$MARKER_LOCAL")
  PRIOR_TS=$(jq -r '.created_epoch // 0' "$MARKER_LOCAL")
  AGE_DAYS=$(( ($(date +%s) - PRIOR_TS) / 86400 ))
  if [ "$PRIOR_FP" = "$FINGERPRINT" ] && [ "$AGE_DAYS" -lt "$DEDUPE_DAYS" ]; then
    EXISTING_TICKET=$(jq -r '.ticket_id // empty' "$MARKER_LOCAL")
  fi
fi

DESCRIPTION="Smoke test judge verdict: FAIL

${SUMMARY}

Failing legs:
${FAILING_LEGS:-\* (no per-leg detail)}

Critical failures:
${CRITICAL:-\* (no critical failure detail)}

Links:
* Run: ${RUN_URL}
* Dashboard: ${DASHBOARD_URL}
* Evidence: s3://kiro-reviewer-archive/evidence/smoke-${RUN_ID}-<os>-<engine>-<ui_mode>/

Fingerprint: ${FINGERPRINT}

This ticket was cut automatically by the smoke-tests workflow."

# Tickety is the NAWS SIM-T API: SigV4-signed REST, service name "tickety".
# botocore ships on GitHub runners; bash+curl cannot SigV4-sign.
tickety_call() { # $1=METHOD $2=PATH $3=BODY-JSON
  python3 - "$1" "$2" "$3" "$REGION" <<'PYEOF'
import json, sys
from botocore.auth import SigV4Auth
from botocore.awsrequest import AWSRequest
from botocore.session import Session
import urllib.request

method, path, body, region = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
endpoint = f"https://{region}.api.tickety.amazon.dev{path}"
creds = Session().get_credentials()
if creds is None:
    print(json.dumps({"error": "no AWS credentials"})); sys.exit(1)
req = AWSRequest(method=method, url=endpoint, data=body.encode(),
                 headers={"Content-Type": "application/json"})
SigV4Auth(creds.get_frozen_credentials(), "tickety", region).add_auth(req)
try:
    resp = urllib.request.urlopen(urllib.request.Request(
        endpoint, data=body.encode(), method=method,
        headers=dict(req.headers)), timeout=30)
    print(resp.read().decode())
except urllib.error.HTTPError as e:
    print(json.dumps({"error": f"HTTP {e.code}", "body": e.read().decode()[:500]}))
    sys.exit(1)
PYEOF
}

if [ -n "$EXISTING_TICKET" ]; then
  echo "duplicate failure (fingerprint $FINGERPRINT); commenting on $EXISTING_TICKET"
  COMMENT_BODY=$(jq -cn --arg msg "Recurred in run ${RUN_URL} (verdict: fail). ${SUMMARY}" \
    '{message: $msg, contentType: "text/amz-markdown-sim"}')
  tickety_call POST "/Default/Default/tickets/${EXISTING_TICKET}/comments" "$COMMENT_BODY" \
    || echo "comment failed; ticket may be resolved" >&2
  exit 0
fi

# CTI values are case-sensitive and must match the resolver group settings.
TICKET_BODY=$(jq -cn \
  --arg title "[SmokeBot] Smoke tests FAILED: ${SUMMARY}" \
  --arg desc "$DESCRIPTION" \
  '{severity: "SEV_3",
    title: ($title | .[0:250]),
    description: $desc,
    descriptionContentType: "text/amz-markdown-sim",
    categorization: [
      {key: "category", value: "Kiro"},
      {key: "type", value: "CLI"},
      {key: "item", value: "Intake"}
    ]}')

RESPONSE=$(tickety_call POST "/Default/Default/tickets" "$TICKET_BODY")
TICKET_ID=$(printf '%s' "$RESPONSE" | jq -r '.ticketId // .id // empty')
if [ -z "$TICKET_ID" ]; then
  echo "ticket creation failed: $RESPONSE" >&2
  exit 1
fi
echo "created ticket $TICKET_ID (https://t.corp.amazon.com/$TICKET_ID)"

jq -cn --arg fp "$FINGERPRINT" --arg id "$TICKET_ID" --argjson ts "$(date +%s)" \
  '{fingerprint: $fp, ticket_id: $id, created_epoch: $ts}' > "$MARKER_LOCAL"
aws s3 cp "$MARKER_LOCAL" "s3://${BUCKET}/${MARKER_KEY}" >/dev/null \
  || echo "dedupe marker upload failed; next failure may double-ticket" >&2

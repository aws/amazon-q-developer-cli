#!/usr/bin/env bash
# smoke-slack-report.sh — post the smoke judge verdict to Slack.
#
# Always posts a one-line verdict summary to the channel. When the verdict is
# "fail", additionally posts a threaded reply with the failing legs and
# critical failures so the channel stays scannable.
#
# Env:
#   SLACK_BOT_TOKEN  bot token; when unset the script exits 0 without posting
#   VERDICT_FILE     path to judge-verdict.json (required)
#   GITHUB_RUN_ID    workflow run id, used to build run/dashboard links
#   RUN_REASON       free-text reason shown in the summary line
#   SLACK_CHANNEL    channel id override
set -euo pipefail

[ -n "${SLACK_BOT_TOKEN:-}" ] || exit 0
VERDICT_FILE="${VERDICT_FILE:?VERDICT_FILE is required}"
CHANNEL="${SLACK_CHANNEL:-C0ARG53D0NL}"
RUN_ID="${GITHUB_RUN_ID:-0}"

if [ -f "$VERDICT_FILE" ] && jq -e 'type == "object"' "$VERDICT_FILE" >/dev/null 2>&1; then
  VERDICT=$(jq -r '.verdict // "unknown"' "$VERDICT_FILE")
  SUMMARY=$(jq -r '.summary // "no summary"' "$VERDICT_FILE")
else
  VERDICT="unknown"
  SUMMARY="Judge did not produce a verdict file"
fi

case "$VERDICT" in
  pass) EMOJI=":white_check_mark:" ;;
  warn) EMOJI=":warning:" ;;
  fail) EMOJI=":rotating_light:" ;;
  *)    EMOJI=":question:" ;;
esac

RUN_URL="https://github.com/kiro-team/kiro-cli/actions/runs/${RUN_ID}"
DASHBOARD_URL="https://kiro-bot-dashboard.beta.harmony.a2z.com/?tab=smoke&runId=smoke-${RUN_ID}"
MESSAGE="${EMOJI} *${VERDICT^^}* — ${SUMMARY}"$'\n\n'"<${RUN_URL}|GitHub Run> | <${DASHBOARD_URL}|Dashboard> | Reason: ${RUN_REASON:-scheduled}"

PARENT_PAYLOAD=$(jq -cn --arg channel "$CHANNEL" --arg text "$MESSAGE" \
  '{channel: $channel, unfurl_links: false, blocks: [
    {type: "header", text: {type: "plain_text", text: "Smoke Test Judge Verdict"}},
    {type: "section", text: {type: "mrkdwn", text: $text}}
  ]}')
PARENT_TS=$(curl -s -X POST https://slack.com/api/chat.postMessage \
  -H "Authorization: Bearer $SLACK_BOT_TOKEN" \
  -H "Content-Type: application/json" \
  -d "$PARENT_PAYLOAD" | jq -r '.ts // empty')

[ "$VERDICT" = "fail" ] || exit 0
if [ -z "$PARENT_TS" ]; then
  echo "slack parent post failed; skipping failure detail thread" >&2
  exit 0
fi
if [ ! -f "$VERDICT_FILE" ]; then
  exit 0
fi

# Slack caps a section block at 3000 chars; truncation keeps the post valid.
FAILING_LEGS=$(jq -r '
  [.legs[]? | select(.verdict == "fail")
    | "• `\(.os)/\(.engine)/\(.ui_mode)` — \(.summary // "no summary")"]
  | join("\n") | .[0:2800]' "$VERDICT_FILE")
CRITICAL=$(jq -r '
  [.critical_failures[]? | if type == "object" then (.reason // tojson) else tostring end
    | "• \(.)"]
  | join("\n") | .[0:2800]' "$VERDICT_FILE")
[ -n "$FAILING_LEGS" ] || FAILING_LEGS="(no per-leg failure detail in verdict)"
[ -n "$CRITICAL" ] || CRITICAL="(no critical failure detail in verdict)"

DETAIL_PAYLOAD=$(jq -cn \
  --arg channel "$CHANNEL" \
  --arg ts "$PARENT_TS" \
  --arg legs "$FAILING_LEGS" \
  --arg critical "$CRITICAL" \
  --arg run_id "$RUN_ID" \
  '{channel: $channel, thread_ts: $ts, unfurl_links: false, blocks: [
    {type: "section", text: {type: "mrkdwn", text: ":x: *Failing legs*\n\($legs)"}},
    {type: "divider"},
    {type: "section", text: {type: "mrkdwn", text: ":mag: *Critical failures*\n\($critical)"}},
    {type: "context", elements: [{type: "mrkdwn",
      text: "Evidence: `s3://kiro-reviewer-archive/evidence/smoke-\($run_id)-<os>-<engine>-<ui_mode>/` — _AI Generated - SmokeBot_"}]}
  ]}')
RESP=$(curl -s -X POST https://slack.com/api/chat.postMessage \
  -H "Authorization: Bearer $SLACK_BOT_TOKEN" \
  -H "Content-Type: application/json" \
  -d "$DETAIL_PAYLOAD")
if [ "$(printf '%s' "$RESP" | jq -r '.ok')" != "true" ]; then
  echo "slack detail post failed: $(printf '%s' "$RESP" | jq -r '.error // "unknown"')" >&2
fi

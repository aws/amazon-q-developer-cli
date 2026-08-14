---
name: kuts-metrics-explorer
description: Investigate Kiro CLI KUTS metrics in CloudWatch Metrics and Logs Insights, including ADA read-only profile setup, missing-series diagnosis, dimensions, version and engine usage, availability, and raw EMF diagnostics. Use when asked to access, explore, troubleshoot, chart, query, or explain Kiro CLI metrics in the production KUTS account.
---

# KUTS Metrics Explorer

Investigate production Kiro CLI metrics without changing AWS resources.

## Production constants

- Account: `615299732016`
- Region: `us-east-1`
- Standard profile: `kuts_telemetry_prod_read-only`
- CloudWatch namespace: `KiroCLI`
- Direct KUTS EMF log group: `/kuts/kiro-cli/metrics`
- KiroTelemetry forwarded copy: `/kiro/metrics`

Read `docs/oncall/metrics_and_telemetry.md` for the maintained operator workflow and query
templates. Read `crates/kiro-telemetry-schema/schema/metrics.yaml` for the exact metric attributes and
CloudWatch dimensions. Use `docs/design/kuts-metric-dimension-review.md` only when the metric's product
question, count point, or derived formula needs explanation.

## Prepare read-only access

Use `KUTS_AWS_PROFILE` when it is set; otherwise use the standard profile. If the profile is missing,
create it:

```bash
PROFILE="${KUTS_AWS_PROFILE:-kuts_telemetry_prod_read-only}"

if ! aws configure list-profiles | grep -Fxq "$PROFILE"; then
  ada profile add \
    --account 615299732016 \
    --profile "$PROFILE" \
    --provider isengard \
    --region us-east-1 \
    --role ReadOnly
fi
```

Refresh credentials and verify the account before querying:

```bash
PROFILE="${KUTS_AWS_PROFILE:-kuts_telemetry_prod_read-only}"
ada credentials update --profile "$PROFILE" --once
aws sts get-caller-identity \
  --profile "$PROFILE" \
  --region us-east-1 \
  --no-cli-pager
```

Stop if the returned account is not `615299732016` or the ARN is not an assumed `ReadOnly` role. If
ADA reports a missing posture cookie or invalid posture, ask the user to restore AEA/Midway posture
and retry; do not recreate a profile that already exists.

## Investigation workflow

1. Restate the operational or product question and select a narrow UTC window. Start with 15 minutes
   for incidents or one hour for exploration.
2. Select the source:
   - Use CloudWatch namespace `KiroCLI` for bounded dimensions, graphs, alarms, and metric math.
   - Use `/kuts/kiro-cli/metrics` for direct metric records and non-dimension diagnostic fields.
   - Use `/kiro/metrics` with `` `kuts.forwarded` = "true" `` only when reproducing the adoption
     report or comparing the KiroTelemetry forwarding path.
3. Confirm the metric's `cloudwatch_dimensions` in `metrics.yaml` before constructing a query.
4. List recent CloudWatch series before graphing or retrieving data:

   ```bash
   PROFILE="${KUTS_AWS_PROFILE:-kuts_telemetry_prod_read-only}"
   aws cloudwatch list-metrics \
     --profile "$PROFILE" \
     --region us-east-1 \
     --namespace KiroCLI \
     --metric-name METRIC_NAME \
     --recently-active PT3H \
     --no-cli-pager
   ```

5. Run a Logs Insights query with `aws logs start-query`, then poll `aws logs get-query-results` until
   `status` is `Complete`. Prefer the copy-ready templates in the on-call guide.
6. Report the time window, source, metric names, dimensions, aggregation, result, and query scan
   statistics. State rollout or denominator caveats next to the result.

## Diagnose missing or unexpected series

Use the same UTC window to compare CloudWatch series with direct EMF:

1. Run `list-metrics` for the metric and inspect each returned series' complete dimension set.
   CloudWatch treats the full set as the series identity; a graph using a different set does not match
   it.
2. Query `/kuts/kiro-cli/metrics` for the same metric and cohort:

   ```text
   fields @timestamp, @message
   | filter ispresent(METRIC_NAME)
   | sort @timestamp desc
   | limit 20
   ```

3. Interpret the comparison:
   - CloudWatch series and EMF records both exist: correct the graph's region, time range, statistic,
     period, or exact dimensions.
   - EMF records exist but no matching CloudWatch series exists: inspect the record's
     `_aws.CloudWatchMetrics` declaration and compare it with `metrics.yaml`; investigate the KUTS
     declaration/export path.
   - Neither exists: check client-version rollout, whether the producer path ran, telemetry consent,
     and `kiro_cli_telemetry_export_dropped_total`.
   - Only old dimension sets exist: separate pre-migration and migrated client versions before
     concluding that emission stopped.

## Question map

| Question | Base metrics |
|---|---|
| Active user adoption by engine/interface | Run `.kiro/skills/telemetry/scripts/adoption-report.py`; installation adoption uses `kiro_cli_daily_heartbeat` |
| V1, V2, V3, interactive, one-shot, or external ACP usage | `kiro_cli_user_turns` |
| Chat sessions created | `kiro_cli_chat_session_started_total` |
| Client-turn availability | `kiro_cli_user_turns`, `kiro_cli_turn_failure_total`, `kiro_cli_turn_cancelled_total` |
| Startup readiness rate | `kiro_cli_run_started_total`, sample count of `kiro_cli_startup_duration_seconds` |
| Login availability | `kiro_cli_login_success_total`, `kiro_cli_auth_failure_total{auth_operation=login}` |
| Model request failures | `kiro_cli_model_invocations_total`, `kiro_cli_model_request_failure_total` |
| MCP availability, failed server names, and tool footprint | `kiro_cli_mcp_server_init_total`, `kiro_cli_mcp_tools_token_count_estimate`; use Logs Insights for `mcp_server_name` |
| Telemetry delivery failures | `kiro_cli_telemetry_export_dropped_total` |

Use the formulas in the dimension review for derived rates. Never combine model, retry, tool, auth,
startup, and turn failures into one numerator: one user-visible failure can emit several diagnostic
counters.

## Guardrails

- Keep Logs Insights windows narrow; the log groups are high volume.
- Treat CloudWatch dimensions and raw EMF fields separately. A field present in EMF is not
  necessarily selectable in CloudWatch Metrics.
- Use `Sum` for delta counters. Preserve the metric's declared dimensions when comparing numerator and
  denominator series.
- Expect old and new dimension sets during client rollout. Missing pre-migration fields may appear as
  `unknown` in reports.
- Do not infer unique people, WAU, or MAU from daily heartbeats. One heartbeat is one active
  installation-version day.
- Do not expose or group by user, request, session, conversation, or other unique identifiers.
- Do not call server-generated EMF records client logs. KUTS does not accept client OTLP logs.
- Keep all AWS activity read-only.

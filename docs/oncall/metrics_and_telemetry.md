---
name: metrics-and-telemetry
description: Kiro CLI KUTS metrics, CloudWatch diagnostics, and legacy Toolkit telemetry. Use when investigating errors or analyzing CLI telemetry.
---

# Metrics and Telemetry

## Observability surfaces

Kiro CLI currently has two client-telemetry pipelines:

- KUTS receives OTLP metrics directly from the CLI and TUI. Its server-side ADOT collector writes raw
  EMF to `/kuts/kiro-cli/metrics` and creates `KiroCLI` CloudWatch series from explicit metric
  declarations. KUTS also forwards a tagged copy to KiroTelemetry, which writes `/kiro/metrics`.
- Legacy Toolkit and CodeWhisperer telemetry continue to feed existing Kibana dashboards until their
  independent cutovers are approved.

The reconciled KUTS client catalog and local validation are complete, but its production declarations
must deploy before the client revision. During the transition, confirm which client version and
pipeline produced a signal before comparing counts.

## KUTS metrics and CloudWatch

### Production locations

| Setting | Value |
|---|---|
| AWS account | `615299732016` |
| Region | `us-east-1` |
| Read-only profile | `kuts_telemetry_prod_read-only` |
| CloudWatch namespace | `KiroCLI` |
| Direct KUTS EMF log group | `/kuts/kiro-cli/metrics` |
| KiroTelemetry forwarded copy | `/kiro/metrics`, filtered by `` `kuts.forwarded` = "true" `` |

The schema source of truth is
[`metrics.yaml`](../../crates/kiro-telemetry-schema/schema/metrics.yaml). The
[KUTS telemetry architecture and metric dimension review](../design/kuts-metric-dimension-review.md)
records the 54 emitted metrics, their product questions, dimensions, derived CloudWatch expressions,
and rollout order.

### Explore CloudWatch metrics

1. Open the [telemetry AWS account](https://iad.merlon.amazon.dev/account/aws/615299732016/aws) and
   log in with the `ReadOnly` role.
2. In the AWS console, navigate to **CloudWatch** and confirm the region is `us-east-1`.
3. Choose **Metrics**, **All metrics**, then the `KiroCLI` custom namespace.
4. Search for a metric such as `kiro_cli_user_turns`. Select the dimension set that matches the
   catalog, then select the series to graph.
5. Use `Sum` for counters. For latency and resource measurements, choose the statistic and period
   appropriate to the question.
6. Use the **Query** tab for Metrics Insights. For example, completed turns by version, agent engine, and
   interface:

```sql
SELECT SUM(kiro_cli_user_turns)
FROM SCHEMA("KiroCLI", version_full, session_interface, agent_mode, agent_engine)
GROUP BY version_full, agent_engine, session_interface
```

The exporter uses `NoDimensionRollup`, so CloudWatch contains the declared dimension set rather than
automatic copies with fewer or no dimensions. Released clients may temporarily expose older
dimension sets during the migration.

List recently active series from a terminal:

```bash
export AWS_PROFILE=kuts_telemetry_prod_read-only
export AWS_REGION=us-east-1

aws sts get-caller-identity
aws cloudwatch list-metrics \
  --namespace KiroCLI \
  --metric-name kiro_cli_user_turns \
  --recently-active PT3H
```

### Explore raw EMF with Logs Insights

Use `/kuts/kiro-cli/metrics` for direct KUTS records and non-dimension diagnostic attributes. Use
`/kiro/metrics` plus `` `kuts.forwarded` = "true" `` when reproducing the adoption report or comparing
KUTS with the KiroTelemetry pipeline. Keep the time range narrow because these are shared,
high-volume log groups. These EMF records are server-generated representations of metric datapoints,
not client-emitted OTLP logs.

Inspect recent records:

```text
fields @timestamp, @message
| filter @message like /"kiro_cli_/
| sort @timestamp desc
| limit 20
```

Compare completed turn usage:

```text
fields kiro_cli_user_turns, version_full, agent_engine, session_interface
| filter ispresent(kiro_cli_user_turns)
| stats sum(kiro_cli_user_turns) as turns
  by version_full, agent_engine, session_interface
| sort turns desc
```

Calculate client-turn availability by cohort:

```text
fields kiro_cli_user_turns,
       kiro_cli_turn_failure_total,
       kiro_cli_turn_cancelled_total,
       version_full,
       agent_engine,
       session_interface,
       agent_mode
| filter ispresent(kiro_cli_user_turns)
      or ispresent(kiro_cli_turn_failure_total)
      or ispresent(kiro_cli_turn_cancelled_total)
| stats sum(kiro_cli_user_turns) as turns,
        sum(kiro_cli_turn_failure_total) as failures,
        sum(kiro_cli_turn_cancelled_total) as cancellations
  by version_full, agent_engine, session_interface, agent_mode
| filter turns > cancellations
| fields version_full,
         agent_engine,
         session_interface,
         agent_mode,
         100 * (1 - failures / (turns - cancellations)) as availability_pct
| sort availability_pct asc
```

Find MCP servers with failed initialization:

```text
fields kiro_cli_mcp_server_init_total,
       mcp_server_name,
       mcp_server_source,
       version_full,
       agent_engine,
       mcp_init_outcome
| filter ispresent(kiro_cli_mcp_server_init_total) and mcp_init_outcome = "failure"
| stats sum(kiro_cli_mcp_server_init_total) as failures
  by mcp_server_name,
     mcp_server_source,
     version_full,
     agent_engine
| sort failures desc
```

Run a one-hour Logs Insights query from a terminal:

```bash
START_TIME=$(python3 -c 'import time; print(int(time.time()) - 3600)')
END_TIME=$(python3 -c 'import time; print(int(time.time()))')
QUERY_ID=$(
  aws logs start-query \
    --log-group-name /kuts/kiro-cli/metrics \
    --start-time "$START_TIME" \
    --end-time "$END_TIME" \
    --query-string 'fields version_full, agent_engine, session_interface
| filter ispresent(kiro_cli_user_turns)
| stats sum(kiro_cli_user_turns) as turns by version_full, agent_engine, session_interface
| sort turns desc' \
    --query queryId \
    --output text
)
aws logs get-query-results --query-id "$QUERY_ID"
```

Repeat `get-query-results` until `status` is `Complete`. For the standard user adoption and
agent-engine usage report, run:

```bash
python3 .kiro/skills/telemetry/scripts/adoption-report.py 7
```

CloudWatch can split a metric only by its declared `cloudwatch_dimensions`. Additional diagnostic
attributes remain fields on the raw EMF record and do not create more metric series. For example,
`kiro_cli_mcp_server_init_total` exposes aggregate source and terminal outcome dimensions while
retaining `mcp_server_name` for Logs Insights investigation. `OauthRequest` is intermediate and does
not create an initialization point. `kiro_cli_mcp_tools_token_count_estimate` records the raw advertised
tool-schema footprint for successful servers.

KUTS does not accept OTLP logs. Request IDs, free-form errors, exact custom tool names, and similar
high-cardinality facts must not be added to metric dimensions.

## Legacy Toolkit and Kibana

Legacy Toolkit telemetry is available in Kibana. You must be on the VPN and select the `global` tenant.

- [Chat Dashboard](https://telemetry-externalprod.ide-toolkits.dev-tools.aws.dev/_plugin/kibana/app/dashboards?security_tenant=global#/view/ba643300-3bdf-11f0-81a5-5b380cd28ab3?_g=(filters%3A!()%2CrefreshInterval%3A(pause%3A!t%2Cvalue%3A0)%2Ctime%3A(from%3Anow-1M%2Cto%3Anow)))
- [Autocomplete Dashboard (old)](https://telemetry-externalprod.ide-toolkits.dev-tools.aws.dev/_plugin/kibana/app/dashboards#/view/7d632ba0-8452-11ee-a5e4-47e65b6d55d5?_g=h@c823129&_a=h@7b5e93b)

### Quick Tips for Kibana

- `result` is `Failed` for exceptions. Generally, it is `Cancelled` for Ctrl-C interrupts.
- `reason` contains general reason codes and supports wildcard searches such as `*Exception`.
- `reasonDesc` contains loose descriptive data and may include request IDs.

See the [Client Telemetry runbook](https://w.amazon.com/bin/view/CodeWhisperer/Operations/Runbooks/Plugin/)
for a larger overview of the legacy Kibana workflow.

## CloudWatch Alarms

[CloudWatch Alarms](https://isengard.amazon.com/federate?account=678005972646&role=ReadOnly&destination=https%3A%2F%2Fus-east-1.console.aws.amazon.com%2Fcloudwatch%2Fdeeplink.js%3Fregion%3Dus-east-1%23alarmsV2%3Aalarm%2FConsolasRTS-prod-IAD-ChatAlarms-ChatAPIs-Availability%2BAlarm-GenerateAssistantComponentExecution-CLI-Critical%3F~(search~%27cli))

## Telemetry Definitions

KUTS metrics are defined in
[`metrics.yaml`](../../crates/kiro-telemetry-schema/schema/metrics.yaml), with attribute vocabularies in
[`types.yaml`](../../crates/kiro-telemetry-schema/schema/types.yaml). Legacy Toolkit events are defined
in the shared
[`telemetry_definitions.json`](../../crates/kiro-telemetry-legacy/telemetry_definitions.json) catalog.

### Main legacy events

| Event | Description |
|-------|-------------|
| `codewhispererterminal_addChatMessage` | Emitted on every new request |
| `amazonq_messageResponseError` | Emitted whenever the global error handler receives a new error |
| `codewhispererterminal_toolUseSuggested` | Emitted once for every single tool use emitted by the model (multiple tool uses = multiple events) |
| `codewhispererterminal_recordUserTurnCompletion` | Emitted at the end of every user turn |

### Important Call-outs

- When tracking legacy errors, `addChatMessage` is a subset of the errors emitted by
  `messageResponseError`.
- `messageResponseError` contains all application errors, not just API request errors such as Ctrl-C
  handling.

## Performance Guardrails

Telemetry emission runs on request, turn, tool-use, and process-health paths, so new metrics should be
cheap when disabled and bounded when enabled.

- Start with the exact product or operational question and include only dimensions needed by a
  dashboard, alarm, or investigation.
- Use schema-aware constructors in `kiro_telemetry::metric` rather than hand-built attribute strings.
  Constructors enforce canonical names and bounded vocabularies for fixed concepts such as outcomes,
  OS, agent engine, and agent mode.
- Treat intentionally open dimensions such as the service-provided model ID as explicit exceptions.
  Raw MCP server names, custom tool names, IDs, and error text are not CloudWatch dimensions.
- Add high-frequency measurements as accumulated turn or session summaries where possible. Avoid
  per-token, per-render, or per-stream-chunk emission unless the signal justifies the volume.
- Attributes without `allowed_values` are intentionally open. KUTS metric declarations, not client
  schema metadata, control which attributes become CloudWatch dimensions.
- KUTS rejects an OTLP metrics payload above its request limit. The Rust exporter records bounded
  permanent-drop receipts and removes them after a successful recovery flush.

## Legacy Kibana error fields

### Result Field Values

Legacy telemetry events use `result` to specify whether an event refers to an error:

| Value | Meaning |
|-------|---------|
| `Succeeded` | Operation completed successfully |
| `Failed` | An error occurred (see error codes below) |
| `Canceled` | User interrupted with Ctrl-C |

### Error Classification Fields

Legacy events that refer to errors define two fields:

- **reason** - Specific error code such as `Interrupted` or `QuotaBreachError`.
- **reasonDesc** - Loose descriptive data that may contain request IDs.

## Legacy error codes

### Application Error Codes

| Error Code | Description |
|------------|-------------|
| `Interrupted` | User pressed ctrl+c. Generally safe to ignore. |
| `ContextWindowOverflow` | User sent a request that caused the context window to overflow |
| `ModelOverloadedError` | The requested model has had too much traffic recently |
| `MonthlyLimitReached` | User has reached their monthly limit |
| `CompactHistoryFailure` | Conversation compaction failed. Indicates a bug - compaction should never fail. |
| `QuotaBreachError` | Request rate limit was breached (too many requests in short period) |
| `NonInteractiveToolApproval` | Emitted for non-interactive sessions where tool approval is required |
| `RecvErrorUnexpectedToolUseEos` | Unexpected end of stream while receiving a tool use. Not revealed as error in service metrics - bedrock sometimes returns invalid JSON for complicated tool uses. |
| `RecvErrorStreamTimeout` | Error waiting for next event in stream after long wait time |
| `RecvErrorApiClient` | Unknown errors while consuming a response stream |
| `RecvErrorJson` | JSON deserialization error during stream processing (should no longer be emitted) |
| `RecvErrorToolValidationError` | Invalid JSON from LLM for tool uses - could cause conversation corruption |

### API Client Error Codes

These errors come from the generated Q API client:

| Error Code | Description |
|------------|-------------|
| `dispatch failure` | Request failed during dispatch. No HTTP response received. Request MAY have been sent. Transient transport failures, including a peer resetting or closing a pooled connection, are retried automatically; reaching the user means retries were exhausted or the failure was not transient. Do not assume the user's local environment. |
| `service error` | Error response received from the service |
| `request has timed out` | Request failed due to timeout. Request MAY have been sent and received. |
| `InternalServerException` | Internal server error |
| `AccessDeniedException` | Access denied |
| `ValidationException` | Validation error |
| `ThrottlingException` | Request was throttled |
| `BedrockError` | Error from Bedrock |

## Legacy error classification

| Error | User Error | System Error | Notes |
|-------|:----------:|:------------:|-------|
| Interrupted | ✓ | | |
| ContextWindowOverflow | ✓ | | |
| ModelOverloadedError | | ✓ | Needs Investigation |
| MonthlyLimitReached | ✓ | | Quota |
| CompactHistoryFailure | | ✓ | |
| QuotaBreachError | ✓ | | Rate limiting |
| NonInteractiveToolApproval | ✓ | | |
| RecvErrorUnexpectedToolUseEos | | ✓ | |
| RecvErrorToolValidationError | | | |
| RecvErrorStreamTimeout | | ✓ | |
| RecvErrorApiClient | | ✓ | |
| RecvErrorJson | | ✓ | |
| dispatch error | ✓ | | |
| service error | | ✓ | |
| request has timed out | | ✓ | |
| InternalServerException | | ✓ | |
| AccessDeniedException | ✓ | | |
| ValidationException | | ✓ | Needs Investigation |
| ThrottlingException | ✓ | | |
| BedrockError | | ✓ | |

---
name: cloudwatch-alarms
description: Kiro CLI CloudWatch alarm definitions, metric pipeline, and investigation guide. Use when triaging alarm tickets, understanding alarm thresholds, or tracing metric data sources.
---

# CloudWatch Alarms

## Quick Reference

| Alarm Name | Metric | Stat | Threshold | Eval | Datapoints | Sev | Missing Data |
|---|---|---|---|---|---|---|---|
| `QCLI-FirstTokenLatency` | `QCLITimeToFirstTokenLatency` | p90 | > 15,000ms | 3 × 5min | 2 | SEV-2 | BREACHING |
| `QCLI-SuccessRateDown` | Success Rate % (math) | — | < 99% | 3 × 5min | 2 | SEV-2 | BREACHING |
| `QCLIRTSCallSuccessRate` | RTS Call Success Rate % (math) | — | ≤ 80% | 4 × 1hr | 3 | SEV-2 | NOT_BREACHING |
| `QCLIFaultCount` | `QCLIFaultCount` | SUM | ≥ 50 | 4 × 1hr | 3 | SEV-3 | NOT_BREACHING |
| `QCLIErrorCount` | `QCLIErrorCount` | SUM | ≥ 50 | 4 × 1hr | 3 | SEV-3 | NOT_BREACHING |

Composite alarm `QCLI-{stage}-ServiceHealthAlarm` fires if either `QCLI-FirstTokenLatency` or `QCLI-SuccessRateDown` breach.

All alarms auto-create SIM tickets (CTI: `AWS / QDev / CLI`).

## Account & Console Links

* **Account**: `421629052180` — **Namespace**: `Toolkit` — **Dimension**: `product: CodewhispererForTerminal` — **Region**: us-east-1
* [CloudWatch Alarms](https://isengard.amazon.com/federate?account=421629052180&role=ReadOnly&destination=https%3A%2F%2Fus-east-1.console.aws.amazon.com%2Fcloudwatch%2Fdeeplink.js%3Fregion%3Dus-east-1%23alarmsV2%3A) · [Dashboards](https://isengard.amazon.com/federate?account=421629052180&role=ReadOnly&destination=https%3A%2F%2Fus-east-1.console.aws.amazon.com%2Fcloudwatch%2Fdeeplink.js%3Fregion%3Dus-east-1%23dashboards%2F) · [Kiro CLI Dashboard](https://isengard.amazon.com/federate?account=421629052180&role=ReadOnly&destination=https%3A%2F%2Fus-east-1.console.aws.amazon.com%2Fcloudwatch%2Fdeeplink.js%3Fregion%3Dus-east-1%23dashboards%3Aname%3DKiro-CLI-Dashboard)
* Backend/RTS alarms are in account `678005972646` (Consolas RTS team) — see [metrics_and_telemetry.md](metrics_and_telemetry.md)

## Data Pipeline

```
CLI binary → Telemetry Service → ToolkitTelemetryLambda
  → EMF logs → CloudWatch Metrics → Alarms → SIM Ticket
```

The Lambda transforms telemetry events into [Embedded Metric Format](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/CloudWatch_Embedded_Metric_Format_Specification.html) logs. CloudWatch auto-parses these into metrics. All QCLI metrics require CLI version ≥ `1.12.1`.

## Alarm Details

### QCLI-FirstTokenLatency

Time from request to first response token (ms).

**Data source**: telemetry event `codewhispererterminal_addChatMessage`, field `codewhispererterminal_timeToFirstChunksMs`. Only emitted when value > 0.

**Investigation**:
1. Check the "Time To First Token Latency(MS)" widget on the [Kiro CLI Dashboard](https://isengard.amazon.com/federate?account=421629052180&role=ReadOnly&destination=https%3A%2F%2Fus-east-1.console.aws.amazon.com%2Fcloudwatch%2Fdeeplink.js%3Fregion%3Dus-east-1%23dashboards%3Aname%3DKiro-CLI-Dashboard) for p50/p90/p99 trends
2. Check if request volume changed — a drop with latency spike suggests backend issue
3. Cross-reference with `#dae-ops` — this is server-side latency
4. Root-cause by version/model in Kibana (see [Kibana Root-Cause](#kibana-root-cause) below)

### QCLI-SuccessRateDown

Percentage of requests succeeding, excluding user failures. Formula: `100 - (systemFailures / total) * 100`.

**Data source**:
* `total` = SUM of `QCLIMessageCount`
* `systemFailures` = all `QCLIMessageResponseError` minus these user failure reasons: `Interrupted`, `ContextWindowOverflow`, `MonthlyLimitReached`, `QuotaBreachError`, `NonInteractiveToolApproval`, `dispatch failure`, `AccessDeniedException`, `ThrottlingException`

**Investigation**:
1. Check "System Failure Rate %" and "System Failures by Type" dashboard widgets
2. Check Kibana for `amazonq_messageResponseError` with `result=Failed` — look at `reason` and `statusCode`
3. Cross-reference `#dae-ops` for backend outages

### QCLIRTSCallSuccessRate

RTS streaming call success rate. Formula: `(succeeded / (succeeded + failed)) * 100` using `QCLIRecordCompleteStreamReceivedCount` split by `result` dimension.

### QCLIFaultCount / QCLIErrorCount

Count of 5xx (fault) and 4xx (error) responses.

**Data source**: telemetry event `amazonq_messageResponseError`, field `statusCode` — first digit determines which metric (`5xx` → Fault, `4xx` → Error).

## Kibana Root-Cause

CloudWatch metrics only have `{product}` as a dimension for latency — you **cannot** break down by version or model in CloudWatch.

Use the [Chat Kibana Dashboard](https://telemetry-externalprod.ide-toolkits.dev-tools.aws.dev/_plugin/kibana/app/dashboards?security_tenant=global#/view/ba643300-3bdf-11f0-81a5-5b380cd28ab3) (VPN required) to root-cause:

1. Filter to `metricName: codewhispererterminal_addChatMessage`
2. Key fields available in raw telemetry:
   * `productVersion` — CLI version
   * `codewhispererterminal_model` — model ID
   * `codewhispererterminal_timeToFirstChunksMs` — latency (ms)
   * `codewhispererterminal_timeBetweenChunksMs` — inter-chunk latencies
   * `result`, `statusCode`
3. Group by `productVersion` or `codewhispererterminal_model` with percentile aggregation on the latency field

## CloudWatch Dimensions

Most metrics only have `{product}`. Richer dimensions exist on count-based metrics:

| Dimensions | Metrics |
|---|---|
| `{product}` | All latency metrics, `QCLIMessageCount`, `QCLIDailyHeartbeat`, `QCLIFaultCount`, `QCLIErrorCount` |
| `{product, result}` | `QCLIRecordCompleteStreamReceivedCount` |
| `{product, failureReason}` | `QCLIMessageResponseError` |
| `{product, toolName[, toolUseStatus]}` | `QCLIToolUsage` |
| `{product, productVersion}` | `QCLIRequestsByVersion` |


## All QCLI Metrics

| EMF Metric Name | Source Event | Key Fields | Dedup |
|---|---|---|---|
| `QCLITimeToFirstTokenLatency` | `codewhispererterminal_addChatMessage` | `codewhispererterminal_timeToFirstChunksMs` | No |
| `QCLIRTSStreamCallFullResponseLatency` | `codewhispererterminal_addChatMessage` | `timeToFirstChunksMs` + `timeBetweenChunksMs` (summed) | No |
| `QCLIRecordCompleteStreamReceivedCount` | `codewhispererterminal_addChatMessage` | `result` | Yes |
| `QCLIMessageCount` | `codewhispererterminal_addChatMessage` | — | Yes |
| `QCLIRequestsByVersion` | `codewhispererterminal_addChatMessage` | `productVersion` | Yes |
| `QCLIUserTurnDuration` | `codewhispererterminal_recordUserTurnCompletion` | `codewhispererterminal_userTurnDurationSeconds` | No |
| `QCLIToolUsage` | `codewhispererterminal_toolUseSuggested` | `toolName`, `toolUseIsSuccess` | Yes |
| `QCLIMessageResponseError` | `amazonq_messageResponseError` / `addChatMessage` (Failed) | `reason` | Yes |
| `QCLIFaultCount` | `amazonq_messageResponseError` | `statusCode` 5xx | Yes |
| `QCLIErrorCount` | `amazonq_messageResponseError` | `statusCode` 4xx | Yes |
| `QCLIDailyHeartbeat` | `amazonqcli_dailyHeartbeat` | — | Yes |

## Code Reference

Both packages live in the [ToolkitTelemetryLambda](https://code.amazon.com/packages/ToolkitTelemetryLambda/trees/mainline) repo, deployed via the [ToolkitTelemetryLambda pipeline](https://pipelines.amazon.dev/pipelines-wip/ToolkitTelemetryLambda).

* **ToolkitTelemetryLambda** (Kotlin) — The Lambda that receives raw telemetry events and transforms them into CloudWatch EMF metrics. This is where metric names, dimensions, and transformation logic are defined. Search for `QCLI` in `CloudWatchUtils.kt`.

* **ToolkitTelemetryInfrastructure** (TypeScript CDK) — Defines the CloudWatch alarms, dashboards, and metric math expressions that consume the EMF metrics. QCLI-specific alarms and dashboard are under `src/monitoring/`. Some alarms (`QCLIRTSCallSuccessRate`, `QCLIFaultCount`, `QCLIErrorCount`) are in the shared `src/cloudwatch/` framework.

To modify: change the Lambda for metric emission, change the CDK for alarm thresholds/dashboards, then deploy the pipeline.

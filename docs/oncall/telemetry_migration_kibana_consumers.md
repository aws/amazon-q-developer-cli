---
name: telemetry-migration-kibana-consumers
description: Consumer inventory and sign-off tracker for migrating Kiro CLI telemetry from Toolkit/Kibana to OTel-derived CloudWatch and facts logs.
---

# Telemetry Migration Consumer Inventory

This document is the Phase 2 exit-gate tracker for the OTel telemetry migration described in
`docs/design/telemetry-otel-cloudwatch-migration.md`. Every legacy CloudWatch, Kibana, Kinesis, and
schema consumer listed here must either migrate to the new OTel-derived data source or explicitly
accept deprecation before legacy telemetry channels are retired.

## Current Discovery Status

| Area | Status | Evidence | Gap |
|---|---|---|---|
| CloudWatch SEV alarms | Seeded | `cloudwatch_alarms_and_dashboard.md` lists the 5 QCLI alarms and source metrics | Reconcile `product=CodewhispererTerminal` in code/design with `CodewhispererForTerminal` in older runbook text |
| Kibana dashboards | Seeded | `metrics_and_telemetry.md` links the Chat dashboard used by on-call | Saved-search IDs and owners still need export from Kibana |
| ToolkitTelemetryInfrastructure | Blocked | Design references `src/monitoring/metrics/qcli-metrics.ts`; local observer comments link the file | Internal code search currently requires Midway re-auth |
| Kinesis-fed consumers | Open | Design identifies the parallel Kinesis to Kibana path | Need owner list from Toolkit telemetry infrastructure |
| Finance metering | Seeded | Design states `MeteringEvent` is for finance reconciliation | Need finance owner/sign-off |
| External schema consumers | Open | Design requires inventory of `aws-toolkit-telemetry-definitions/def.json` consumers | Need internal code search across Brazil/source index |

## Consumer Tracker

| Consumer | Legacy data source | New data source | Owner | Migration action | Sign-off |
|---|---|---|---|---|---|
| QCLI-FirstTokenLatency alarm | `Toolkit` namespace, `QCLITimeToFirstTokenLatency`, `codewhispererterminal_addChatMessage.codewhispererterminal_timeToFirstChunksMs` | `ChatCLI` namespace metric `kiro_cli_time_to_first_chunk_ms` and/or `kiro_cli.bedrock.stream.ttft` | chat-cli on-call | Create `-shadow` alarm and compare p90 for 30 days | Pending |
| QCLI-SuccessRateDown alarm | `QCLIMessageCount` and `QCLIMessageResponseError` metric math | Derived `kiro_cli.slo.success_rate{slo_target=turn}` from OTel counters/logs | chat-cli on-call | Implement nightly parity job and shadow alarm | Pending |
| QCLIRTSCallSuccessRate alarm | `QCLIRecordCompleteStreamReceivedCount` split by `result` | `kiro_cli.bedrock.request.duration` / derived RTS success-rate rule | chat-cli on-call / RTS | Define OTel source mapping and shadow alarm | Pending |
| QCLIFaultCount alarm | `QCLIFaultCount` from `amazonq_messageResponseError.statusCode` 5xx | `kiro_cli.bedrock.request.errors{status_class=5xx}` | chat-cli on-call | Create `-shadow` alarm and validate count parity | Pending |
| QCLIErrorCount alarm | `QCLIErrorCount` from `amazonq_messageResponseError.statusCode` 4xx | `kiro_cli.bedrock.request.errors{status_class=4xx}` | chat-cli on-call | Create `-shadow` alarm and validate count parity | Pending |
| Kiro CLI CloudWatch dashboard | `Toolkit` namespace widgets in account `421629052180` | `ChatCLI` namespace widgets | chat-cli on-call | Duplicate dashboard with OTel metrics, then cut links/runbooks | Pending |
| Chat Kibana dashboard | Raw Toolkit telemetry in `telemetry-externalprod` global tenant | `/aws/chat-cli/facts` via Athena/QuickSight or CloudWatch Logs Insights | TBD | Export saved objects, rewrite each search/query, identify widget owners | Pending |
| Release SOP health checks | Kibana crash, memory, and error-rate telemetry | OTel process, error, and health metrics | release owner / chat-cli on-call | Update `release_process_sop.md` once dashboard exists | Pending |
| ToolkitTelemetryInfrastructure QCLI metrics | `src/monitoring/metrics/qcli-metrics.ts` and related CDK | OTel shadow alarm and dashboard definitions | TBD | Audit code after Midway auth, list every metric expression | Blocked |
| ToolkitTelemetryLambda QCLI transforms | EMF transform logic for `QCLI*` metrics | ADOT EMF exporter and schema-backed OTel records | Toolkit telemetry owner | Confirm no unmapped legacy metric before Phase 3 | Pending |
| Finance metering reconciliation | Server-side metering path; client previously dropped `MeteringEvent` | `kiro_cli_metering_event` fact log | finance owner TBD | Validate usage/unit fields and reconciliation query | Pending |
| `aws-toolkit-telemetry-definitions` consumers | Generated `def.json` / Rust structs | `kiro-telemetry-schema` generated legacy exports | external owners TBD | Internal code search for imports; keep bridge until zero consumers | Open |

## Required Queries And Artifacts

| Artifact | Owner | Exit requirement |
|---|---|---|
| Kibana saved-object export for Chat dashboard | TBD | Every visualization mapped to Athena/CloudWatch replacement or marked deprecated |
| ToolkitTelemetryInfrastructure QCLI source audit | TBD | Every metric/alarm expression copied into this tracker with owner |
| Legacy-to-OTel parity report | chat-cli on-call | 14 consecutive daily runs within tolerance before Phase 2 exit |
| Finance metering reconciliation query | finance owner TBD | Client `kiro_cli_metering_event` agrees with server metering within the Phase 3 threshold |
| Schema consumer search results | telemetry migration owner | All `aws-toolkit-telemetry-definitions` consumers listed with migration status |

## Known Migration Notes

- Legacy alarms run in account `421629052180`, region `us-east-1`, namespace `Toolkit`.
- The design treats `product=CodewhispererTerminal` as authoritative from code, while
  `cloudwatch_alarms_and_dashboard.md` still says `CodewhispererForTerminal`; resolve this before
  writing parity queries.
- CloudWatch latency widgets cannot break down by version or model today; existing on-call guidance
  uses Kibana for that root cause path.
- The new facts path is `/aws/chat-cli/facts`; anything requiring user, request, message, model,
  version, or cohort joins must move there rather than becoming metric dimensions.
- If the future KUTS swap changes the downstream EMF log group to `/kiro/metrics`, this inventory
  must be re-run for the Phase 3.75 re-pointing step.

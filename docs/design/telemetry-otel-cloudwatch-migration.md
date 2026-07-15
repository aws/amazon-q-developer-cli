# Chat-CLI Telemetry: Migration to OpenTelemetry + CloudWatch

| | |
|---|---|
| **Author** | chat-cli platform team |
| **Date** | 2026-06-03 |
| **Status** | Draft for review (accept-with-fixes from 6 reviewers integrated) |
| **Contributors** | PM (adoption/funnels), SRE/on-call (SLOs, telemetry-on-telemetry), Data Engineering (split rule, fact tables), Security/Compliance (paired posture counters), Engineering (wire protocol, schema crate) |
| **Prior research** | `docs/oncall/metrics_and_telemetry.md`, `docs/oncall/cloudwatch_alarms_and_dashboard.md` (cited as authoritative for current-state inventory) |

---

## TL;DR

Chat-CLI today emits telemetry through two hand-rolled channels (`PostMetrics` SigV4/Cognito and `SendTelemetryEvent` Bearer) routed asymmetrically across 20 (V1) and 23 (V2) `EventType` variants. The schema lives in three drifting JSON copies. We have **zero observability of the telemetry pipeline itself** (no drop rate, no queue depth, no exporter health), no PII redaction, no opt-out end-to-end test, no per-version active-user gauge, and no funnel/retention metrics computable without a Kibana ticket.

This design replaces both channels with an OpenTelemetry Rust SDK (pinned `~0.32.0`) emitting OTLP/HTTP to a team-owned ADOT collector that exports EMF to CloudWatch — with a single canonical schema crate, a hard cardinality budget enforced at the SDK boundary, paired success/violation counters that prove privacy and partition isolation, telemetry-on-telemetry as P0, and a 4-phase rollout that holds dual-write parity gates before retiring legacy. We name the downstream Kibana/dashboard consumers up front and require their migration before cutover. Cost envelope: ~$2-4K/month standing CW metrics + EMF logs at proposed cardinality (vs unbounded today). The migration is *transport + schema + posture*, not a collection expansion — new metrics from the four perspective catalogs land in Phase 2+.

---

## 1. Background — what telemetry exists today

Detailed inventory in `docs/oncall/metrics_and_telemetry.md` and the V1/V2 schema research surface. One-page TL;DR.

**Two emission channels, asymmetric routing.**

- **Channel A — `PostMetrics`** (Toolkit Telemetry, SigV4 via Cognito identity pool `us-east-1:820fd6d1-95c0-4ca4-bffb-3f01d32da842`, hardcoded `EXTERNAL_PROD` endpoint at `crates/chat-cli/src/telemetry/mod.rs:137-141` and `cognito.rs:180`). Routes **all** EventTypes via `into_metric_datum()` (`mod.rs:778-808`).
- **Channel B — `SendTelemetryEvent`** (CodeWhisperer `ApiClient`, Bearer from `auth.builder_id`). Per `mod.rs:680-775`, **only** `ChatAddedMessage` and `AgentContribution` match; the other 18+ variants fall through to a no-op arm at `mod.rs:772`.
- GovCloud (`US_GOV_EAST` / `US_GOV_WEST`) hard-skips Channel A at `mod.rs:213-234` — partition-aware, but unproven by metric.

**Schema, three copies, drifting.**

| File | Lines | Notes |
|---|---|---|
| `crates/chat-cli/telemetry_definitions.json` | 666 | V1; 20 EventType variants enumerated at `crates/chat-cli/src/telemetry/core.rs:705-838` (`UserLoggedIn`, `AuthFailed`, `RefreshCredentials`, `CliSubcommandExecuted`, `ChatSlashCommandExecuted`, `ChatStart`, `ChatEnd`, `ChatAddedMessage`, `RecordUserTurnCompletion`, `TangentModeSession`, `ToolUseSuggested`, `AgentContribution`, `McpServerInit`, `AgentConfigInit`, `DidSelectProfile`, `ProfileState`, `MessageResponseError`, `DailyHeartbeat`, `SubagentInvocation`, `VoiceInput`). |
| `crates/chat-cli-v2/telemetry_definitions.json` | 908 | V2; 23 variants (V1 + `ProcessHealthMetric`, `ModeChanged`, `GoalCompleted`); V2 `Event` struct adds `app_type`, `acp_client_name`, `acp_client_version` at `core.rs:60-87`. |
| `crates/aws-toolkit-telemetry-definitions/def.json` | 543 | Build-time codegen via `build.rs:42-279` using `quote!`+`syn`; consumed by `amzn-toolkit-telemetry-client` and unidentified external consumers. |

**CloudWatch consumers.** Account `421629052180`, namespace `Toolkit`, dimension `product=CodewhispererTerminal` (NOT `CodewhispererForTerminal` — verified at `mod.rs:789`). 5 critical alarms in `docs/oncall/cloudwatch_alarms_and_dashboard.md:10-26`: `QCLI-FirstTokenLatency` (p90 > 15s), `QCLI-SuccessRateDown` (< 99%), `QCLIRTSCallSuccessRate` (≤ 80%), `QCLIFaultCount` (≥ 50 5xx), `QCLIErrorCount` (≥ 50 4xx). All static, none multi-burn-rate, none monitor pipeline health.

**Downstream consumer landscape (required for cutover).** The five SEV alarms above feed CloudWatch — but `ToolkitTelemetryInfrastructure/qcli-metrics.ts` runs a parallel Kinesis → Kibana pipe whose dashboards on-call references for triage. We do not have a complete inventory; Phase 2 cannot exit until `docs/oncall/telemetry_migration_kibana_consumers.md` is written and every owner signs off.

**Operational gaps observed in code.**

- `time_to_first_chunk` is logged at `crates/chat-cli/src/agent/rts/mod.rs` (≈line 542) but never emitted as a metric.
- `MetadataEvent` (input/output/cache tokens) is captured at the streaming layer but never emitted.
- `MeteringEvent` from the streaming client is dropped on the floor (`api_client/model.rs`).
- No PII redaction is performed today on telemetry payloads; outbound free-text fields like `reason_desc`, `error_type`, `init_failure_reason`, `all_tool_names` are sent verbatim.
- No telemetry-on-telemetry: no exporter drop counter, no queue depth, no batch flush latency, no opt-out-respected counter.
- No `kiro_cli_pii_redaction_runs_total` (the redactor does not run).
- No per-version active-user gauge → rollback decisions are reactive (forum-post-driven), not metric-driven.
- No `install_date` / `first_message_date` propagation → cohort retention is not computable.

---

## 2. Goals & Non-goals

### Goals (priority order)

**P0 — must ship in Phase 1.**

- **G1. Single schema source of truth.** One canonical `kiro-telemetry-schema` crate replaces V1 + V2 `telemetry_definitions.json`; `aws-toolkit-telemetry-definitions/def.json` is bridged via one-way generator (see §7a) until external consumers are inventoried and migrated.
- **G2. OTel SDK at the boundary.** All emit sites in chat-cli/chat-cli-v2 use OpenTelemetry Metrics + Logs APIs through a single `kiro-telemetry-facade`. No direct calls to Toolkit Telemetry or `SendTelemetryEvent` from product code.
- **G3. CloudWatch alarm parity.** Every existing alarm keeps firing on equivalent OTel-derived metrics during dual-write; cut over only after ≥30 days of side-by-side parity within 0.5%.
- **G4. Privacy proof points.** `kiro_cli_telemetry_opt_out_respected_total`, `kiro_cli_telemetry_opt_out_violation_total` (must stay 0), `kiro_cli_pii_redaction_coverage_ratio` (0 → ≥0.999), `kiro_cli_consent_record_integrity_total`, `kiro_cli_govcloud_channel_disabled_total` / `kiro_cli_govcloud_channel_leak_total` paired counters wired before any new outbound channel turns on. End-to-end opt-out test in CI (see §11).
- **G5. Cardinality budget enforced downstream, schema shape enforced at the SDK boundary.** Closed enums only for genuinely fixed sets (outcomes, os_type, engine, …); free-form dimensions (`version_full`, `mode`, `mcp_server_name`, `subagent_name_class`, `builtin_tool_name`) are emitted raw client-side with known-alias normalization, bounded by schema `max_distinct` budgets in the KUTS ingestion layer. Client-side validation still rejects unregistered metrics/attributes and out-of-enum values. Hard caps: ≤10 dims/metric.
- **G6. Telemetry-on-telemetry.** P0 self-observability set ships day one (§5.10).

**P1 — required for first PM dashboards (Phase 2).**

- **G7.** Adoption KPIs live: DAU/WAU/MAU, `version_adoption_pct`, `client_version_seen` (the only metric carrying full semver, capped LRU 200), `dau_mau_ratio`, `new_users_daily`.
- **G8.** Funnel/cohort fact tables: `kiro_cli_client_identity`, `kiro_cli_first_message_sent`, `kiro_cli_user_turn_completed`, `kiro_cli_conversation_completed`, `kiro_cli_tool_invoked`, `kiro_cli_feature_first_use`.
- **G9.** Conversation outcomes + token economics: per-turn token counts on the fact row, `kiro_cli_estimated_cost_usd` counter (with sidecar `kiro_cli_pricing_table_active` gauge — `price_table_version` is NOT a metric dim), reconciliation against server-side `MeteringEvent`.

**P2 — stretch (Phase 3+).**

- A/B exposure + assignment, voice/tangent/ACP-external parity, subagent rollups with depth and cost.

### Non-goals

- New metrics backend. We use ADOT Collector → CloudWatch EMF and inherit KUTS when it ships.
- Tracing on day one. Metrics + structured logs cover the surface; tracing is Phase 5+.
- Per-`user_id` metrics. User-level data lives in fact tables (logs), never metric dimensions.
- New collection on day one of Phase 1. Phase 1 is migration; new metrics from the four catalogs land Phase 2+.
- Removing Toolkit Telemetry until parity is proven. Dual-write through Phase 3.
- Preserving the `Toolkit/CodewhispererTerminal` namespace path. Consumers MUST migrate to the new EMF log group within Phase 2 (explicit non-goal — see §7a).

---

## 3. Future-state architecture

```
+----------------------------------------------------------------------+
|                         chat-cli / chat-cli-v2                       |
|                                                                      |
|  product code (~30 unchanged call-sites: agent/subagent.rs:116,      |
|     auth/builder_id.rs:318/581, cli/chat/tool_manager.rs:1616/2160,  |
|     os/mod.rs:45...)                                                 |
|     |                                                                |
|     v                                                                |
|  +-----------------------------------------------------------------+ |
|  |   kiro-telemetry-facade  (single send_event API)                | |
|  |   - opt-out gate         (zero exports if disabled, fuzzed)     | |
|  |   - PII redactor         (fail-closed = drop event)             | |
|  |   - cardinality limiter  (LRU, _other_ overflow bucket)         | |
|  |   - partition guard      (compile-flag + runtime double-check)  | |
|  |   - sink fan-out         (per-sink bounded mpsc + timeout)      | |
|  +-----------------------------------------------------------------+ |
|     |             |             |                                   |
|     v             v             v                                   |
|  Legacy        Legacy         OtelSink (primary; eventually sole)   |
|  ToolkitSink   CWhispererSink                                       |
|  (Channel A)   (Channel B)                                          |
|  retired P3    retired P3                                           |
|                                                                     |
|  OTel SDK opentelemetry ~0.32.0:                                    |
|    Counter, Histogram (Delta);  UpDownCounter, ObservableGauge      |
|       (Cumulative); Logs SDK for fact rows                          |
|    PeriodicReader: 60s interval, 30s timeout, ±10% jitter           |
|    On-disk WAL (~64 MB cap, 7d TTL) at ${state_dir}/telemetry/wal/  |
|    Crash hook (panic + SIGTERM + clean) → flush w/ deadline         |
+-----------------------------|----------------------------------------+
                              | OTLP/HTTP :4318 (/v1/metrics, /v1/logs)
                              | http-proto, delta temporality on counters
                              v
+----------------------------------------------------------------------+
|              ADOT Collector (TEAM-OWNED, single regional ADOT)       |
|   DNS: otel.chat-cli.<region>.amazonaws.dev                          |
|   Auth: SigV4 via Cognito identity chain (same posture as Channel A) |
|   receivers: otlp                                                    |
|   processors: batch, attributes (drop high-card; allowlist below),   |
|               resourcedetection (os.type, host.arch, partition)     |
|   exporters: awsemf  (CloudWatch Logs in EMF format)                 |
|              awscloudwatchlogs  (raw fact rows for the lake)        |
+--------------------------|---------------------------|---------------+
                           |                           |
              EMF events   v                           v   raw JSON
               +-------------------+         +---------------------+
               | CloudWatch Logs   |         | CloudWatch Logs     |
               | /aws/chat-cli/emf |         | /aws/chat-cli/facts |
               +---------|---------+         +-----------|---------+
                         v                               v
               CloudWatch Metrics             Firehose → S3 → Glue/Athena
               namespace: ChatCLI             (cohort/funnel SQL, retention)
               (5 legacy alarms re-pointed,
                + new SLO + product alarms)
                         |
                         v
            PM / SRE / Security dashboards
              (CloudWatch, Athena, QuickSight)

  Telemetry-on-telemetry pipe is a SEPARATE direct PutMetricData
  (~20 series, 5-min interval) so it does not share fate with OTLP.
```

### Why OTel + CW (vs the existing Cognito/Smithy stack)

- **Inheritance.** OTel semantic conventions (GenAI, HTTP), Prometheus naming, ADOT awsemf exporter — we stop maintaining a bespoke pipeline. EMF handles dimension folding, delta→cumulative conversion, and retry inside the collector, not the customer binary.
- **Observability of the pipeline itself.** OTel SDK self-instrumentation + our own meta-meter give us the drop/queue/cardinality counters Channel A/B never had.
- **Standard transport.** OTLP/HTTP-protobuf; no tonic/gRPC dep on customer laptops.
- **KUTS-ready.** When KUTS GAs, swap one exporter; product code unchanged.

### Endpoint and auth (corrected from earlier draft)

There is no AWS-managed public OTLP endpoint at a `otel.<region>.amazonaws.com` hostname. The collector is **team-owned** and reached via `otel.chat-cli.<region>.amazonaws.dev` (placeholder; final DNS owned by chat-cli team in account TBD). Auth is **SigV4 via the existing Cognito identity chain** (`CognitoProvider` from the legacy path) — same posture and trust boundary as Channel A. **No baked Bearer token ships in the binary.** Failover is sticky-per-session across `home_region → us-east-1 → us-west-2`; GovCloud uses the GovCloud-region collector exclusively (no commercial fallback — partition bridging is a SEV1).

### Cardinality discipline (call-out)

> **Cardinality rules — enforced in code, not review.**
>
> 1. Every metric attribute is a closed Rust enum in `kiro-telemetry-schema/schema/types.yaml`. `_other_` bucket is always present.
> 2. The following are **never** metric dimensions: `user_id`, `anonymous_client_id`, `session_id`, `conversation_id`, `request_id`, `tool_use_id`, `message_id`, `panic_location` (file:line), `host_id_hash`, `expected_hash` strings, free-form `reason_desc`/`error_message`, raw MCP `server_name` for user-defined servers, raw MCP/custom `tool_name`. They live on logs/spans/exemplars only.
> 3. `version_full` allowed only on `client_version_seen` observable gauge with LRU cap 200; everywhere else use `version_minor` bucketed to {current, current-1, current-2, older}.
> 4. `tool_name` splits into `builtin_tool_name` (registry enum) and `tool_origin` ∈ {builtin, mcp, custom, subagent_delegate, aws_api}. MCP/custom/user tool names → fact rows only.
> 5. `mcp_server_name` on metrics collapses to `mcp_server_class` ∈ {builtin_<name>, official_third_party, user_defined, internal_amazon}.
> 6. ADOT `attributes` processor allowlist for resource attrs that survive into the metric stream: `service.version` (bucketed), `os.type`, `host.arch`, `deployment.environment`, `client_application`, `partition`. Everything else dropped.
> 7. Per-metric series cap default **5,000**; runtime limiter trips `telemetry.cardinality.overflow_total{metric, attribute}` on exceedance and routes excess to `_other_`.
> 8. CI lint walks the workspace and fails on any `MetricDatum::builder()` or `attributes!()` outside `kiro_telemetry_facade` or `kiro_telemetry_schema`.
> 9. Per-instrument temporality: Counter, Histogram → **Delta**; UpDownCounter, ObservableGauge → **Cumulative**.

---

## 4. Cost envelope

Worked example at proposed cardinality (assumes ~3 partitions, ~10 active model-class buckets, ~15 builtin tools, `version_full` bounded at 200 distinct and `mcp_server_name` at 64 by the collector caps):

| Bucket | Series estimate | Monthly cost ($0.30/series CW custom metric) |
|---|---|---|
| Adoption (12 metrics) | ~150 series each → 1,800 | $540 |
| Feature usage (10) — top-N rolled | ~250 each → 2,500 | $750 |
| Performance (8 histograms) | ~120 each → 960 | $290 |
| Reliability + SLO (10) | ~150 each → 1,500 | $450 |
| Health (process: 6) | ~100 each → 600 | $180 |
| LLM-specific (6) | ~80 each → 480 | $145 |
| Tool-use & MCP (6) — class-bucketed | ~120 each → 720 | $215 |
| Privacy (10 paired posture) | ~30 each → 300 | $90 |
| Telemetry-on-telemetry (10) | ~40 each → 400 | $120 |
| EMF log ingest (~30 GB/mo at proposed event volume × $0.50/GB) | — | $15 |
| Athena scans for nightly DAU/retention batch jobs | — | ~$30 |
| **Total** | **~9,260 series** | **~$2,825/month** |

For comparison, the doc's earlier "$15K/month" scare math reflects what an *unbounded* cardinality regression would cost (50K series × $0.30). The hard cap of 5K series/metric × ~50 active metrics gives a worst-case ceiling near $7,500/month; the runtime cardinality limiter trips a P0 page well before that.

---

## 5. Metric Catalog

Each entry below has: name, kind (counter / gauge / histogram / log_event), unit, **bounded** dimensions, rationale, alert idea, priority. Tables for density. Anything emitted as `log_event` is an OTel Logs SDK record routed to CloudWatch Logs `/aws/chat-cli/facts`, **not** a metric.

### 5.1 Usage & adoption

| Name | Kind | Unit | Dimensions | Rationale | Alert idea | Pri |
|---|---|---|---|---|---|---|
| `kiro_cli_session_started_total` | counter | 1 | `version_full`, `os`, `install_source`, `client_application` | Foundational top-of-funnel; denominator for ratios | Anomaly: -20% WoW per (os, install_source) >1k baseline | P0 |
| `kiro_cli_chat_session_started_total` | counter | 1 | `version_full`, `mode`, `client_application` | Distinct from cli_session — first prompt sent | — | P0 |
| `active_users_daily` | observable_gauge | users | `install_method`, `client_application`, `is_internal_amazon` | Daily rollup from facts; aggregate-only series | DAU drop >15% WoW per install_method | P0 |
| `active_users_weekly` | observable_gauge | users | same | WAU smooths weekend troughs | Growth flatlines (<1% WoW) for 3 weeks | P0 |
| `active_users_monthly` | observable_gauge | users | (none) | Single global gauge | — | P0 |
| `dau_mau_ratio` | observable_gauge | ratio | (none) | Stickiness; >0.20 healthy | <0.15 for 2 consecutive weeks | P0 |
| `new_users_daily` | observable_gauge | users | `install_source` | Acquisition-vs-retention split | — | P0 |
| `client_version_seen` | observable_gauge | users | `version_full` (LRU 200), `release_channel`, `os_type` | Only place full semver allowed; 200 × 3 × 4 = 2,400 series cap | — | P0 |
| `version_adoption_pct` | observable_gauge | percent | `version_full`, `release_channel` | "Are users upgrading?" | Latest stable <50% adoption 14d post-release | P0 |
| `stale_version_users` | observable_gauge | users | `staleness_bucket` ∈ {<30d, 30-60, 60-90, >90} | Long-tail upgrade pressure | — | P1 |
| `kiro_cli_upgrade_completed_total` | counter | 1 | `from_version`, `to_version`, `trigger` ∈ {auto, prompted, manual} | Rollout safety | — | P1 |
| `kiro_cli_client_identity` | log_event | event | `anonymous_client_id`, `install_method`, `install_date_epoch_day`, `first_seen_*`, `is_internal_amazon` | Dim table for ALL cohort joins; high-cardinality fields belong here, not in metrics | — | P0 |
| `kiro_cli_daily_heartbeat` | counter | 1 | `client_application`, `install_method` | MAU computation; `client_version` excluded (resource attr only, 90-day rolling) | Volume drop >10% WoW | P1 |

### 5.2 Feature usage

| Name | Kind | Unit | Dimensions | Rationale | Alert idea | Pri |
|---|---|---|---|---|---|---|
| `kiro_cli_slash_command_invoked_total` | counter | 1 | `command` (registry enum, top-N + `_other_`), `version_full` | "Which slash commands are used" | New command <100 invocations 7d post-release | P0 |
| `kiro_cli_feature_used_total` | counter | 1 | `feature` (registry enum, top-50 + `_other_`), `version_full` | Generic non-slash feature counter | — | P0 |
| `feature_unique_users_weekly` | observable_gauge | users | `feature` | Reach (distinguishes spam from breadth) | New feature <5% WAU after 14d | P0 |
| `kiro_cli_tool_call_total` | counter | 1 | `tool_origin` ∈ {builtin, mcp, custom, subagent_delegate, aws_api}, `builtin_tool_name` (only when `tool_origin=builtin`), `outcome` ∈ {success, error, denied, cancelled} | MCP/custom tool names live on `kiro_cli_tool_invoked` log only | denied/total >5% (UX friction) | P0 |
| `tool_using_sessions_pct` | observable_gauge | percent | (none) | % of sessions invoking ≥1 tool | -5pp WoW (agentic discovery regression) | P0 |
| `kiro_cli_mcp_server_connected_total` | counter | 1 | `mcp_server_class` ∈ {builtin_<name>, official_third_party, user_defined, internal_amazon} | Bucketed; raw names → `kiro_cli_mcp_server_init` log | — | P1 |
| `kiro_cli_model_invocations_total` | counter | 1 | `model_class` ∈ {anthropic_opus, anthropic_sonnet, anthropic_haiku, openai_gpt5, other} | Provider+family bucket; full `model_id` lives on logs | — | P0 |
| `mode_active_users_weekly` | observable_gauge | users | `mode` ∈ {interactive, oneshot, agent, plan, review, tangent, voice, acp_external, generate_agent} | Mode adoption | New mode <2% WAU after 30d | P0 |
| `kiro_cli_feature_first_use` | log_event | event | `anonymous_client_id`, `feature_name`, `first_used_at`, `session_id`, `trigger` | Funnel attribution for activation | — | P1 |
| `voice_session` | log_event | event | (full attrs on log; metric counter `voice_sessions_total` summarizes) | bounded; Whisper backend, model size | — | P2 |

**Demoted to facts/logs only** (per data-eng split rule and reviewer cuts): `feature_first_use_total`, `slash_command_unique_users_weekly`, `feature_funnel_completion_pct`, `skill_invoked_total`, `permission_prompt_total`, `documentation_link_clicked_total`, `referral_install_total`, `session_concurrent_active_users`, `conversation_export_total`, `settings_changed_total`. PMs query Athena.

### 5.3 Performance

| Name | Kind | Unit | Dimensions | Buckets / Notes | Pri |
|---|---|---|---|---|---|
| `kiro_cli.bedrock.stream.ttft` | histogram | s | `model_class`, `prompt_size_bucket`, `tools_enabled` | 0.1, 0.25, 0.5, 1, 2, 5, 10, 30 | P0 |
| `kiro_cli.bedrock.stream.duration` | histogram | s | `model_class`, `completion_reason` | 1, 2, 5, 10, 30, 60, 120, 300 | P0 |
| `kiro_cli.bedrock.request.duration` | histogram | s | `model_class`, `operation`, `outcome` | same buckets | P0 |
| `kiro_cli.bedrock.stream.inter_token_latency` | histogram | s | `model_class` | 0.01, 0.04, 0.1, 0.25, 1, 5 | P1 |
| `kiro_cli.startup.duration` | histogram | s | `version_full`, `cold_start`, `os_type` | 0.1, 0.25, 0.5, 1, 2, 5, 10 | P0 |
| `kiro_cli.agent.loop.iteration_duration` | histogram | s | `loop_phase` ∈ {model_call, tool_exec, parse, render} | 0.1, 0.5, 2, 10, 30, 120, 300 | P0 |
| `kiro_cli_user_turn_duration_seconds` | histogram | s | `model_class`, `chat_conversation_type`, `is_subagent`, `mode` | 1, 2, 5, 10, 30, 60, 120, 300, 600 | P0 |
| `kiro_cli_time_to_first_chunk_ms` | histogram | ms | `model_class`, `client_application`, `is_subagent` | 100, 250, 500, 1000, 2000, 5000, 10000 | P1 |
| `kiro_cli_session_duration_seconds` | histogram | s | `client_application`, `launch_mode`, `primary_model_class` | 30, 120, 600, 1800, 7200, 28800 | P2 |

### 5.4 Reliability

| Name | Kind | Unit | Dimensions | Rationale | Alert idea | Pri |
|---|---|---|---|---|---|---|
| `kiro_cli.session.completed` | counter | 1 | `exit_reason` ∈ {clean, user_interrupt, crash, oom, hang_timeout, auth_failure, upstream_outage}, `agent_kind` (closed enum) | Pairs with started; success ratio numerator | clean/started <0.85 over 15m on any version_minor | P0 |
| `kiro_cli.crash.total` | counter | 1 | `crash_kind` ∈ {panic, segfault, abort, unhandled_signal}, `os_type`, `host_arch` | `panic_location` is **NOT** a metric dim — it goes to a separate `kiro_cli_panic` log with a 16-bit `panic_signature_hash` exemplar | New panic rate >0.1% of sessions on a version | P0 |
| `kiro_cli.startup.failures` | counter | 1 | `failure_stage` ∈ {config, db_migrate, runtime, panic}, `os_type` | Pre-steady-state failures | startup_failures/started >0.5% over 5m | P0 |
| `kiro_cli.bedrock.request.errors` | counter | 1 | `model_class`, `operation`, `error_kind` ∈ {throttling, validation, model_error, server_error, timeout, connection, access_denied}, `status_class` ∈ {2xx,4xx,5xx} | `error_code` is closed enum allowlist; raw codes → log | 5xx-rate >5% over 5m | P0 |
| `kiro_cli.bedrock.empty_response.retries` | counter | 1 | `model_class`, `outcome` ∈ {recovered, still_empty} | Tracks the `a953a204b` empty-response retry; `still_empty` rising = Bedrock brownout | still_empty >0.5% of turns | P0 |
| `kiro_cli.retry.attempts` | counter | 1 | `upstream` ∈ {bedrock, rts, kas, krs, cognito}, `retry_reason` (closed enum), `attempt_number_bucket` ∈ {1,2,3+} | SDK retry classifier | retry/requests >15% over 5m | P0 |
| `kiro_cli.retry.exhausted` | counter | 1 | `upstream`, `final_error_kind` | True user-visible failures | rate >1% over 5m | P0 |
| `kiro_cli.agent.loop.stuck` | counter | 1 | `stuck_phase`, `detection` | Watchdog-emitted; >5m no progress | rate >0.1% sessions over 5m | P0 |
| `kiro_cli.upstream.dependency.up` | observable_gauge | 1 | `dependency` ∈ {bedrock, rts, kas, krs, cognito, oauth_idp}, `partition` | Synthetic 1/0; cleanest tile | any dep=0 over 5m → page | P0 |
| `kiro_cli.slo.success_rate` | derived (recording rule) | 1 | `slo_target` ∈ {turn, session, login} | Computed in CloudWatch from session.completed counters; **NOT** emitted from the binary | Multi-burn-rate alarms (1h+5m@14.4×, 6h+30m@6×) | P0 |
| `kiro_cli.slo.availability` | derived (recording rule) | 1 | `slo_target` | Likewise computed | Same | P0 |

### 5.5 Health (process)

| Name | Kind | Unit | Dimensions | Notes | Pri |
|---|---|---|---|---|---|
| `kiro_cli.process.memory.rss` | observable_gauge | By | `version_full`, `agent_kind` | Sampled per-session; alerts on cohort p95, not per-host | P0 |
| `kiro_cli.process.memory.growth_rate` | histogram | By/s | `version_full`, `agent_kind` | Linear-fit slope over rolling N min; leak detector | P1 |
| `kiro_cli.process.cpu.utilization` | histogram | 1 | `version_full`, `agent_kind`, `state` ∈ {streaming, idle, tool_running, compaction} | Idle CPU = busy-wait detector | P0 |
| `kiro_cli.process.fds.open` | observable_gauge | 1 | `version_full`, `agent_kind` | FD leaks before EMFILE | P1 |
| `kiro_cli.process.threads` | observable_gauge | 1 | `version_full`, `agent_kind` | Tokio pool blowup detector | P2 |

`panic_location` and `host_id_hash` are **excluded** as metric dimensions — both reviewers flagged these as cardinality bombs. Crash signatures live on `kiro_cli_panic` log records; crash-loop detection comes from a separate per-process detector that emits `chat_cli.process.crashloop.detected` (no host dim).

### 5.6 LLM-specific

| Name | Kind | Unit | Dimensions | Pri |
|---|---|---|---|---|
| `kiro_cli_tokens_consumed` | counter | tokens | `model_class`, `token_type` ∈ {input_uncached, input_cache_read, input_cache_write, output, reasoning}, `client_application`, `is_subagent` | P0 |
| `kiro_cli_estimated_cost_usd` | counter | USD | `model_class`, `client_application`, `is_subagent` (`price_table_version` is **NOT** a dim — it's a sidecar `kiro_cli_pricing_table_active` gauge) | P0 |
| `kiro_cli_cache_hit_ratio` | histogram | 1 | `model_class`, `chat_conversation_type`, `client_application` (buckets 0, 0.25, 0.5, 0.75, 0.9, 1.0) | P1 |
| `kiro_cli_context_usage_percentage` | histogram | percent | `model_class`, `client_application`, `is_subagent` (buckets 10, 25, 50, 75, 90, 95, 99) | P1 |
| `kiro_cli_metering_event` | log_event | event | server-reported `MeteringEvent` for finance reconciliation; currently dropped on the floor in `api_client/model.rs` | P0 |
| `kiro_cli_user_turn_completed` | log_event | event | per-turn fact row; carries token counts, cost, IDs, `feature_flag_assignment_bundle_id` (NOT raw flag map — bundle_id joins to `kiro_cli_feature_flag_assignment` log) | P0 |

### 5.7 Tool-use & MCP

| Name | Kind | Unit | Dimensions | Pri |
|---|---|---|---|---|
| `kiro_cli_tool_invoked` | log_event | event | full attrs on log: `tool_use_id`, `tool_name`, `mcp_server_name` (raw, log-only), `is_success`, `execution_duration_ms`, `model_class` | P0 |
| `kiro_cli_tool_invocations` | counter | 1 | `tool_origin`, `outcome` ∈ {success, error, denied, cancelled} | P1 |
| `kiro_cli_tool_execution_duration_ms` | histogram | ms | `tool_origin`, `is_success` (buckets 10, 50, 200, 1000, 5000, 30000, 120000) | P1 |
| `kiro_cli_mcp_server_init` | log_event | event | per-server-startup; raw `mcp_server_name`, full tool name lists hashed-with-salt for user_config sources | P0 |
| `kiro_cli_mcp_server_init_total` | counter | 1 | `mcp_server_class` (NOT raw name), `outcome` ∈ {success, timeout, auth, protocol, other} | P1 |
| `kiro_cli_subagent_invoked` | log_event | event | `subagent_name` (registry enum, ≤50), `depth_bucket` ∈ {1, 2, 3+}, tokens, cost, result | P1 |

### 5.8 Quality / outcomes

| Name | Kind | Unit | Dimensions | Pri |
|---|---|---|---|---|
| `kiro_cli_user_turns` | counter | 1 | `model_class`, `client_application`, `result` ∈ {success, failed, cancelled}, `is_subagent`, `mode` | P0 |
| `kiro_cli_session_outcome_total` | counter | 1 | `outcome` ∈ {user_quit, task_completed, error, timeout, crash} | P0 |
| `kiro_cli_user_feedback_total` | counter | 1 | `sentiment` ∈ {positive, negative, neutral}, `surface` (closed enum) | P0 |
| `kiro_cli_message_regenerated_total` | counter | 1 | `model_class` | P1 |
| `kiro_cli_conversation_completed` | log_event | event | end-of-conversation rollup; `completion_reason`, turn/tool counts, cost, duration | P0 |
| `kiro_cli_compaction_event` | log_event | event | trigger, ratio, duration, result | P2 |

### 5.9 Security & privacy (paired posture)

| Name | Kind | Unit | Dimensions | Pri |
|---|---|---|---|---|
| `kiro_cli_telemetry_opt_out_respected_total` | counter | 1 | `channel`, `event_class` (closed enum) | P0 |
| `kiro_cli_telemetry_opt_out_violation_total` | counter | 1 | (≤2 attrs by design — emergency signal that bypasses opt-out gate) | P0 |
| `kiro_cli_pii_redaction_runs_total` | counter | 1 | `redactor`, `event_class`, `channel`, `result` ∈ {scrubbed, passthrough, error} | P0 |
| `kiro_cli_pii_redaction_matches_total` | counter | 1 | `pii_type` (closed enum: email, aws_access_key, aws_secret, arn, ipv4, home_path, phone, jwt, credit_card), `field_class` ∈ {prompt, context, tool_output, file_content, http_header, other} | P0 |
| `kiro_cli_pii_redaction_errors_total` | counter | 1 | `redactor`, `error_kind`, `fail_action` ∈ {dropped, passthrough} (passthrough must always be 0) | P1 |
| `kiro_cli_pii_redaction_coverage_ratio` | derived (recording rule) | 1 | computed downstream from `kiro_cli_pii_redaction_runs_total` / `telemetry_events_emitted_total` — **NOT** an emitted gauge | P0 |
| `kiro_cli_govcloud_channel_disabled_total` | counter | 1 | `channel`, `partition`, `reason` (closed enum) | P0 |
| `kiro_cli_govcloud_channel_leak_total` | counter | 1 | (≤2 attrs by design) | P0 |
| `kiro_cli_consent_record_integrity_total` | counter | 1 | `check_kind` ∈ {hash, perms, owner, signature}, `result` ∈ {ok, tampered, missing, unreadable} | P0 |
| `kiro_cli_auth_credential_failure_total` | counter | 1 | `auth_provider`, `error_code` (closed allowlist), `operation` (closed), `partition` | P0 |
| `kiro_cli_auth_unexpected_identity_total` | counter | 1 | `expected_partition`, `actual_partition`, `operation` | P0 |
| `kiro_cli_tls_validation_failure_total` | counter | 1 | `destination_class`, `failure_reason` (closed) | P0 |
| `kiro_cli_tool_egress_destinations_total` | counter | 1 | `destination_class` ∈ {public_internet, aws_endpoint, internal_amzn, localhost}, `scheme`, `is_allowlisted` | P1 |

#### Identifier and consent semantics

- `anonymous_client_id` is a 128-bit random UUID generated on first run, stored locally only. **Not derived from email/hostname/username/MAC/install-path.** Pseudonymous; treated as personal data under GDPR/CCPA.
- Opt-out is honored at the SDK boundary, not the network boundary. Zero export attempts when disabled, proven by a fuzzed contract test (§11).
- Opt-out toggle: on flip from opted-in → opted-out, the spool dir is unconditionally truncated and `telemetry.spool.purged_on_optout_total` increments. New `anonymous_client_id` is generated on opt-in→opt-out→opt-in transition (no cross-window linkage).
- Fact-lake retention: 13 months rolling. User-data deletion request honored within 30 days.
- `User-Agent` contains only product name + semver + `os.type`/`os.arch`. No user identifiers, no path, no token.

### 5.10 Telemetry-on-telemetry (P0)

| Name | Kind | Unit | Dimensions | Catches |
|---|---|---|---|---|
| `kiro_cli.telemetry.exporter.send.attempts` | counter | 1 | `exporter`, `signal`, `outcome` ∈ {success, retry, dropped, permanent_failure} | Exporter health |
| `kiro_cli.telemetry.exporter.send.duration` | histogram | s | `exporter`, `signal` | Slow exporter → drops |
| `kiro_cli.telemetry.exporter.dropped` | counter | 1 | `exporter`, `signal`, `drop_reason` (closed) | Single most important on-call counter |
| `kiro_cli.telemetry.queue.depth` | observable_gauge | 1 | `exporter`, `signal` | Leading drop indicator |
| `kiro_cli.telemetry.batch.size` | histogram | 1 | `exporter`, `signal` | Sizing visibility |
| `telemetry.spool.bytes` | observable_gauge | By | `state` ∈ {pending, replaying} | Offline buffer growth |
| `telemetry.spool.evicted_total` | counter | 1 | `reason` ∈ {expired, oversize, corrupt} | Replay correctness |
| `telemetry.spool.purged_on_optout_total` | counter | 1 | (none) | Cross-process opt-out semantics |
| `telemetry.cardinality.overflow_total` | counter | 1 | `metric_name`, `attribute` | LRU bucketing fired |
| `kiro_cli.telemetry.emit.failures` | counter | 1 | `subsystem`, `failure_kind` (closed) | Bugs in our telemetry layer |
| `kiro_cli.telemetry.sdk.up` | observable_gauge | 1 | `partition`, `os_type`, `release_channel` | "Are we seeing data?" |
| `kiro_cli.telemetry.flush_on_exit.dropped_total` | counter | 1 | `shutdown_path` ∈ {clean, signal, panic} | Short-lived process drop accounting |
| `kiro_cli.meta_meter.up` | observable_gauge | 1 | `partition`, `os_type` | Meta-meter watchdog (direct PutMetricData; does not share fate with OTLP) |

**Why direct PutMetricData for the meta-meter.** EMF via ADOT requires the collector to be reachable. If it's not (which is exactly when you need meta-metrics), EMF won't arrive. The meta-meter is ~20 series at 5-minute interval, costs ~$6/month per fleet, and uses the existing SigV4 chain.

**Drop accounting invariant.** Per `signal`: `count(emits) == count(batches with outcome=success) + count(batches with outcome=permanent_failure) + count(dropped)`. The `attempts{outcome=retry}` counter is intentionally NOT in the invariant because retries are per-HTTP-attempt, not per-batch. A `telemetry.accounting.drift` watchdog gauge (computed nightly) pages on non-zero drift.

---

## 6. Instrumentation Plan

### New crate layout

| Crate | Purpose |
|---|---|
| `crates/kiro-telemetry-schema` | Single canonical schema. `schema/metrics.yaml` + `schema/types.yaml` + `schema/deprecated.yaml`; `build.rs` codegen forks `aws-toolkit-telemetry-definitions/build.rs:42-279` swapping JSON→YAML. Generates: legacy `Codewhispererterminal*` structs (re-exported by `aws-toolkit-telemetry-definitions` for SDK consumers), OTel Counter/Histogram/Gauge wrappers, cardinality budget tables. |
| `crates/kiro-telemetry-facade` | `TelemetrySink` trait, fan-out, opt-out gate, PII redactor, runtime cardinality limiter, partition guard, on-disk spool. Single public API used by both binaries. |
| `crates/kiro-telemetry-cloudwatch` | OTLP-to-EMF mapper, IAM credential integration, batching, retry with budget. |
| Deferred (`reject` review feedback) | Six-crate explosion proposed in plan (kiro-otel-base / -metrics / -security / -audit / -slo) collapses into `kiro-telemetry` (foundation) + `kiro-telemetry-cloudwatch`. SLO metrics are recording rules, not a separate crate. Audit logs route through the same OTel Logs pipeline with a different log group. |

### Code change inventory

| File:line | Change | Metric/event emitted |
|---|---|---|
| `crates/chat-cli/src/telemetry/mod.rs:202-220` | `TelemetryThread::new` gains `MeterProvider`/`LoggerProvider` init via `kiro_telemetry::init`; signature unchanged | (init only) |
| `crates/chat-cli-v2/src/telemetry/mod.rs:203-241` | Mirror | (init only) |
| `crates/chat-cli/src/telemetry/mod.rs:609-611` | Wrap `TelemetryClient::new` in `OptOutGate::allow()`; on disabled, return no-op provider | `kiro_cli_telemetry_opt_out_respected_total` |
| `crates/chat-cli-v2/src/telemetry/mod.rs:706-708` | Mirror | same |
| `crates/chat-cli/src/telemetry/mod.rs:213-234` (GovCloud guard) | Add compile-flag (`#[cfg(feature="govcloud")]`) + runtime double-check; emit paired counters | `kiro_cli_govcloud_channel_disabled_total`, `kiro_cli_govcloud_channel_leak_total` |
| `crates/chat-cli/src/telemetry/mod.rs:675-678` (`TelemetryClient::send_event`) | Replace direct sink calls with `Vec<Arc<dyn TelemetrySink>>` fan-out using **per-sink bounded mpsc + 5s/100ms timeouts** (NOT `join_all` — a slow sink would block the queue) | (routing only) |
| `crates/chat-cli/src/telemetry/mod.rs:680-775` (`send_cw_telemetry_event`) | Wrap behind `legacy_codewhisperer_sink` feature flag; routing matrix is now a static table validated by build-time check that every `EventType` appears exactly once | (legacy preserved through Phase 3) |
| `crates/chat-cli/src/telemetry/mod.rs:778-808` (`send_telemetry_toolkit_metric`) | Wrap behind `legacy_toolkit_sink` feature flag | (legacy preserved through Phase 4) |
| `crates/chat-cli-v2/src/telemetry/mod.rs:266-268` | Mirror fan-out with timeouts | — |
| `crates/chat-cli-v2/src/telemetry/observer.rs` (find `TelemetryObserver::spawn` — file is 1091 lines, not 1-50) | Inject `MeterProvider`; in `AgentEvent` match arms, emit OTel histograms for `time_to_first_chunk`, turn duration, tokens; `client_application` from V2 `Event` struct (NOT `app_type` — see resource-vs-attribute table below) | `kiro_cli_time_to_first_chunk_ms`, `kiro_cli_user_turn_duration_seconds`, `kiro_cli_tokens_consumed`, `kiro_cli_user_turn_completed` log |
| `crates/chat-cli/src/agent/rts/mod.rs:~542` | Existing `time_to_first_chunk` log site → also emit OTel histogram | `kiro_cli.bedrock.stream.ttft` |
| `crates/chat-cli/src/api_client/model.rs` (where `MeteringEvent` is currently dropped) | Wire to `kiro_cli_metering_event` log emit | `kiro_cli_metering_event` |
| Empty-response retry path (commit `a953a204b`) | Wire to counter | `kiro_cli.bedrock.empty_response.retries{outcome}` |
| `crates/chat-cli/src/launch.rs:137,143` | `KIRO_TELEMETRY_ENABLED` env precedence stays; add `KIRO_TELEMETRY_OTEL=0/1/2` (off / dual / OTel-only) | (config only) |
| `crates/chat-cli/src/telemetry/cognito.rs` | Stays untouched through Phase 3; deleted in Phase 4 | — |
| `crates/aws-toolkit-telemetry-definitions/build.rs:42-279` | Replaced by 5-line `pub use kiro_telemetry_schema::legacy::*;` | — |
| `crates/chat-cli/src/telemetry/definitions.rs:9` | Becomes `pub use kiro_telemetry_schema::legacy::*;` (CI-enforced single line) | — |
| `crates/chat-cli-v2/src/telemetry/definitions.rs:9` | Mirror | — |

### Resource attributes vs metric attributes

Reviewer-flagged subtle bug: V2's `app_type`, `acp_client_name`, `acp_client_version` (`crates/chat-cli-v2/src/telemetry/core.rs:60-87`) can vary across events within a single process (interactive → ACP mode), so they cannot be Resource attributes (which are set once at provider init).

| Field | Resource | Metric attribute |
|---|---|---|
| `service.name`, `service.version` | ✓ | — |
| `os.type`, `host.arch`, `deployment.environment` | ✓ | — |
| `partition`, `release_channel`, `is_internal_amazon` | ✓ | — |
| `client_application`, `acp_client_name`, `acp_client_version_major` | — | ✓ (per-record) |
| `model_class`, `tool_origin`, `outcome` | — | ✓ |
| `anonymous_client_id`, `session_id`, `conversation_id`, `request_id` | — | LOG-ONLY (never on metrics) |

### Cargo additions

```toml
opentelemetry        = "~0.32.0"
opentelemetry_sdk    = { version = "~0.32.1", features = ["metrics", "logs", "rt-tokio"] }
opentelemetry-otlp   = { version = "~0.32.0", features = ["http-proto", "reqwest-client", "metrics", "logs"] }
opentelemetry-proto  = "~0.32.0"  # for integration test protobuf decode
```

`~` (compatible-update) chosen over `=` so dependabot can land patch-level CVE fixes; the EMF byte-equality contract test (§11) is the safety net against silent breakage.

### CloudWatch resources

| Resource | Config |
|---|---|
| Log group `/aws/chat-cli/emf` | Retention 7d. EMF JSON: namespace `ChatCLI`, dimensions ≤10 per metric. |
| Log group `/aws/chat-cli/facts` | Retention 30d. Subscription → Firehose → S3 → Glue/Athena for cohort SQL. |
| Log group `/aws/chat-cli/audit` | Retention 30d (compliance). Auth events, tool-denied, config-drift, opt-out toggles. |
| ADOT Collector | Team-owned. `otelcol-contrib`. Receivers: otlp (HTTP :4318). Processors: batch (10000/60s), attributes (allowlist resource attrs, drop high-card), resourcedetection. Exporters: awsemf → emf log group, awscloudwatchlogs → facts log group. IAM: scoped `logs:PutLogEvents` only. |
| EMF vs PutMetricData | **Primary EMF** (1 log put = N metrics, ADOT batching). **PutMetricData** reserved for the meta-meter (~20 series, fail-closed). |

### Per-sink bounded mpsc (replaces `join_all`)

```text
TelemetryClient::send_event
   ├─→ try_send(LegacyToolkitSink mpsc, cap=4096)   timeout 5s,  drop_total{sink=legacy_toolkit}
   ├─→ try_send(LegacyCWhispererSink mpsc, cap=2048) timeout 5s,  drop_total{sink=legacy_cw}
   └─→ try_send(OtelSink mpsc, cap=8192)            timeout 100ms, drop_total{sink=otel}
```

A slow sink only drops to its own counter; the others are unaffected.

---

## 7a. Backwards compatibility

What happens to today's Kibana dashboards and Channel A consumers — explicit, with owners.

**Inventory required before Phase 2 exit.** `docs/oncall/telemetry_migration_kibana_consumers.md` enumerates:

1. The five SEV alarms in account `421629052180` and their owners (chat-cli on-call).
2. `ToolkitTelemetryInfrastructure/qcli-metrics.ts` Kibana saved searches (owners: TBD audit).
3. Kinesis-fed dashboards keyed on `Toolkit/CodewhispererTerminal` namespace (TBD audit).
4. `MeteringEvent` consumers (finance — currently bypassed because we drop `MeteringEvent`; new wiring is additive).
5. External consumers of `aws-toolkit-telemetry-definitions/def.json` codegen — enumerated via `InternalCodeSearch` grep across the Brazil version set. The crate stays alive as a thin re-export of `kiro_telemetry_schema::legacy::*` until the consumer set is empty.

**Migration paths.**

- **Legacy metric continuity (Phase 1-3):** dual-write. Both Channel A and OTel emit the same logical event; CloudWatch dashboards keep rendering.
- **Alarm parity (Phase 2):** new EMF metrics shadow the 5 SEV alarms with a `-shadow` suffix for ≥30 days. Cut over only after Δ < 0.5% for 14 consecutive days. Legacy alarms armed through Phase 4.
- **Kibana migration (Phase 2 exit-blocker):** every saved search rewritten as Athena query against `/aws/chat-cli/facts` log group; dashboard owners sign off in writing.
- **Schema migration (Phase 4):** `aws-toolkit-telemetry-definitions/def.json` is retired iff external-consumer count reaches zero; otherwise frozen with a one-way generator from `kiro-telemetry-schema`.

**Explicit non-goal:** the `Toolkit/CodewhispererTerminal` namespace is **not** preserved. Consumers migrate to `ChatCLI` namespace within Phase 2 or accept deprecation.

---

## 8. Phased rollout

Each phase has hard, falsifiable exit gates. Phase N+1 does not start until N's gate is green.

### Phase 0 — Foundations (1 week)

OTel SDK in tree (no-op by default), `kiro-telemetry-schema` crate landed alongside existing JSON files (additive), meta-meter via direct PutMetricData, WAL + crash hook, runtime cardinality limiter.

**Exit gates:**

- Criterion benchmark: no-op `Counter.add()` <50ns on M3 Mac and Linux x86_64.
- Cardinality limiter test: emit 1,000 distinct `tool_name` values, assert exactly 64 distinct dimension values reach the exporter and `cardinality.overflow_total` increments.
- WAL replay: `kill -9` mid-export of N=10,000 metric points, restart, assert all replayed with original timestamps and `replayed=true` resource attribute.
- Panic-hook test: inject panic mid-session, assert `force_flush(deadline=500ms)` completed, WAL contains `crash_signature`.
- Opt-out E2E test (the test that didn't exist before): see §11.
- `kiro_cli.meta_meter.up = 1` across CI, dev, and dogfood fleet for 7 days.

### Phase 1 — Dual-write commercial (4 weeks)

OTel facade ships behind `KIRO_TELEMETRY_OTEL=1`. Internal Amazon dogfood week 0, 10% prod week 2, 100% prod week 4. ALL existing V1+V2 EventTypes emit through both legacy AND OTel sinks. P0 privacy proof-points and telemetry-on-telemetry counters live.

**Parity job design** (must exist before Phase 1 starts; reviewer-flagged gap):

- Lambda runs nightly. For each legacy `EventType`, queries CloudWatch Metrics (legacy namespace) and the new OTel-derived EMF (new namespace) over the prior 24h.
- Compares `count(legacy_metric) vs derived_count(otel_log_or_metric)` per the explicit mapping table in `kiro-telemetry-schema/schema/legacy_mapping.yaml`.
- Per-metric tolerance: high-volume counters ±2%, low-volume ±5%, daily aggregate gauges ±1%.
- Drift breach pages on-call, files a Taskei.

**Exit gates:**

- `kiro_cli_telemetry_opt_out_violation_total = 0`, `kiro_cli_govcloud_channel_leak_total = 0` for 14 consecutive days fleet-wide.
- `kiro_cli.telemetry.exporter.send.attempts{outcome=success}` rate ≥ 99% over rolling 7 days.
- `kiro_cli_pii_redaction_coverage_ratio ≥ 0.99` on outbound free-text fields.
- Parity job within tolerance for every mapped EventType for 14 consecutive days.
- Cardinality budget held: no metric exceeds 5,000 series; `cardinality.overflow_total` p99 ≤ 100/day.
- Cost: standing CW metric cost ≤ $4,000/month per partition.

### Phase 2 — Product KPIs + alarm shadow (6 weeks; 8 weeks at 1 engineer)

Adoption foundations, fact tables, nightly batch job for cohort gauges, PM dashboard v1, alarm-shadow infrastructure for the 5 SEV alarms. **Kibana consumer migration runs in parallel and gates the exit.**

**Exit gates:**

- DAU and `version_adoption_pct` rendering daily; PM signs off DAU matches manual Athena audit within 2%.
- D1/D7/D30 retention curves render from facts; one common PM question ("% stable v1.42+ users used /code in last 7 days?") drops from days to <10 minutes.
- `kiro_cli_user_turn_completed` count ≥ 95% of legacy `RecordUserTurnCompletion` count over 14 days.
- 5 SEV alarms in shadow mode; Δ < 0.5% for 14 consecutive days.
- Every Kibana consumer in `telemetry_migration_kibana_consumers.md` either migrated to Athena/CloudWatch with owner sign-off, or owner has explicitly accepted deprecation.

### Phase 3 — Outcomes, monetization, A/B + alarm cutover (6 weeks)

Conversation outcomes, paywall metrics (if applicable), A/B exposure. Cut 5 SEV alarms over to OTel-derived metrics. Decommission Channel B (Bearer/`SendTelemetryEvent`) — `ChatAddedMessage` and `AgentContribution` flow OTel-only.

**Exit gates:**

- All 5 SEV alarms held parity ≥ 30 days post-cutover.
- Sample-Ratio-Mismatch monitor green for `feature_flag_exposure_total` for 30 days.
- Cost reconciliation: `|client kiro_cli_estimated_cost_usd − server kiro_cli_metering_event| / server < 5%`.
- ApiClient telemetry init removed (the client itself stays — it's the streaming client; only the telemetry-specific code paths are deleted).

### Phase 4 — Decommission Channel A (4 weeks)

Stop dual-writing. Remove `CognitoProvider`-driven `PostMetrics`. Delete V1 + V2 `telemetry_definitions.json`. Retire `def.json` if external-consumer count reached zero.

**Exit gates:**

- `PostMetrics` and `SendTelemetryEvent` call paths removed from product code.
- Cognito identity pool `us-east-1:820fd6d1-...` no longer reachable from `chat-cli`/`chat-cli-v2`.
- ToolkitTelemetryLambda decommissioned or repurposed (owner sign-off in `421629052180`).
- One on-call rotation owns the entire telemetry pipeline; schema change requires no cross-account or cross-repo coordination.

---

## 9. Risks & mitigations

| # | Risk | Severity | Mitigation |
|---|---|---|---|
| R1 | Cardinality explosion (rogue dim → CW bill spike) | Critical | Schema registry (client rejects unregistered metrics/attrs); KUTS ingestion enforces schema `max_distinct` budgets with `_other_` overflow on free-form dims (`mcp_server_name`, `mode`, `version_full`), `telemetry.cardinality.overflow_total` P0 page, weekly cost-anomaly review on `ChatCLI` namespace. |
| R2 | Cost blow-up during dual-write | High | Standing budget $4K/mo per partition; dual-write capped at 30 days; cost reviewed weekly; Kinesis-side legacy sampling toggle if budget breached. |
| R3 | Privacy regression (opt-out leak, PII outbound) | Critical | Paired counters (`kiro_cli_telemetry_opt_out_violation_total`, `kiro_cli_govcloud_channel_leak_total` must = 0; pageable). Redactor fail-closed (drop event, never passthrough). E2E opt-out test in CI. `kiro_cli_pii_redaction_coverage_ratio ≥ 0.999` SLO. |
| R4 | Parity drift during dual-write | High | Quantitative parity job nightly; Phase gates require per-metric tolerance for 14d; PM Athena audit before Phase 3. |
| R5 | CloudWatch alarm cutover breaks on-call | High | 30-day shadow mode; legacy alarms armed through Phase 4; per-alarm rollback runbook in `docs/oncall/cloudwatch_alarms_and_dashboard.md`. |
| R6 | ADOT Collector unavailability (offline/disconnected hosts) | Medium | On-disk WAL with 7-day TTL, 64 MB cap; meta-meter on direct PutMetricData survives ADOT outage; failover sticky-per-session across home → us-east-1 → us-west-2. |
| R7 | Schema drift recurrence | High | Single source `kiro-telemetry-schema`; CI lint walks AST and rejects `MetricDatum::builder()` outside generated code; CODEOWNERS on `schema/*.yaml` requires PM + SRE + DataEng + Sec sign-off. |
| R8 | Kibana/Channel-A consumers go dark | High | `telemetry_migration_kibana_consumers.md` inventory required; Phase 2 exit blocked until every owner signs off or accepts deprecation. |
| R9 | ACP-external client misattribution during dual-write | Medium | `client_application`, `acp_client_name`, `acp_client_version_major` are per-record metric attributes (NOT resource attrs); V1 `Event` struct backported in Phase 1 to carry these fields. |
| R10 | OTel Rust patch CVE response | Medium | `~0.32.0` allows dependabot patch updates; EMF byte-equality contract test (§11) is the safety net; major-version bumps gated by full SLO regression suite. |
| R11 | `anonymous_client_id` reverse-lookup risk | Medium | UUID v4, locally generated, never derived from PII; rotated on opt-in/out cycle; documented as personal data; 13-month retention. |
| R12 | GovCloud partition-bridging compliance SEV | Critical | Compile-flag (`#[cfg(feature="govcloud")]`) + runtime guard; mutually-exclusive features prevent commercial sinks linking; quarterly compliance report from `kiro_cli_govcloud_channel_leak_total = 0`. |
| R13 | Crash-time WAL flush + tokio runtime poisoning | Medium | Panic hook posts to a dedicated `std::thread`-spawned writer that owns the WAL fd and a small mpsc; never re-enters the tokio runtime; 500ms deadline. |
| R14 | Spool replay leaks old events past opt-out | High | On startup, before any flush, if `OptOutGate::allow == false` → unconditionally truncate spool dir; counted by `telemetry.spool.purged_on_optout_total`. Tested in §11. |
| R15 | KUTS GA timeline uncertain | Medium | Decoupled — we ship to ADOT → CW directly. KUTS adoption is a future exporter swap. Coordinate with KUTS team in Phase 1 to confirm OTLP shape compatibility, but do not block. |
| R16 | Free-form `reason_desc` / `error_code` strings leak via metric dim | High | Closed-enum allowlists per metric; CI lint asserts every attribute key in any metric emit has a registered enum; reason_desc max 64 chars and matches `^[a-z_]+$`. |
| R17 | Test fragility: telemetry actor spawn interferes with unit tests | Medium | `cfg!(test)` gates use `InMemoryExporter`; `TelemetryObserver::spawn()` returns no-op handle; events captured in `TelemetryEventStore` for assertion. |
| R18 | OTLP self-fate-sharing for telemetry-on-telemetry | High | Meta-meter uses direct PutMetricData on its own pipeline; cannot share fate with the OTLP path it measures. |

---

## 10. Success criteria

All of the following must hold for ≥30 consecutive days post-Phase-4 cutover.

**Product (PM-facing).**

- DAU, WAU, MAU, `dau_mau_ratio`, `version_adoption_pct`, top-10 feature reach, `funnel_install_to_first_chat_pct` all on a single CloudWatch dashboard, refreshed daily, no manual intervention.
- D1/D7/D30 retention curves render from facts. PM dashboard distinguishes `client_application` on every panel.
- Time-to-answer for the canonical PM question drops from days → <10 minutes (Athena query against `kiro_cli_user_turn_completed`).

**Reliability.**

- All 5 critical CloudWatch alarms fire from OTel-derived metrics with zero parity-drift incidents in the prior 30 days.
- `kiro_cli.slo.success_rate{slo_target=turn} ≥ 0.99` over 30-day window.
- `kiro_cli.slo.availability{slo_target=session} ≥ 0.995`.
- TTFT p95 < 3s, end-to-end turn p95 < 20s.
- `kiro_cli.telemetry.sdk.up ≥ 0.95` across the active fleet.
- `kiro_cli.telemetry.exporter.send.attempts{outcome=success} ≥ 0.99`, `dropped` rate < 1%.
- On-call MTT-localize regressions cut by ≥ 50% vs static-alarm baseline (measured against next 5 release-induced regressions).

**Privacy.**

- `kiro_cli_telemetry_opt_out_violation_total = 0` since Phase 1.
- `kiro_cli_govcloud_channel_leak_total = 0` since Phase 1.
- `kiro_cli_pii_redaction_coverage_ratio ≥ 0.999`.
- `kiro_cli_consent_record_integrity_total{result=tampered|unreadable} = 0`.
- One-page customer-trust artifact backed by these metrics published.

**Schema & cost.**

- One schema crate. V1 and V2 `telemetry_definitions.json` deleted; `aws-toolkit-telemetry-definitions/def.json` deleted or scoped to remaining external consumers.
- Zero V1/V2 schema-drift defects in prior 60 days.
- Standing CW metrics cost ≤ $4K/month per partition; cardinality budget held (no metric > 5K series).

**Migration safety.**

- `PostMetrics` (SigV4 + Cognito) and `SendTelemetryEvent` (Bearer) call paths removed from product code.
- Cognito identity pool `us-east-1:820fd6d1-95c0-4ca4-bffb-3f01d32da842` no longer reachable.
- ToolkitTelemetryLambda decommissioned or repurposed.

**Business outcome.**

- A monthly business review presented from the new dashboards with no manual data wrangling. First such review occurs in the month following Phase 3 gate.
- A monetization or product decision made using the new funnel + retention KPIs within 60 days of Phase 3 — proof that the metrics actually move product.

---

## 11. Open questions and test strategy

### Open questions (need decisions before Phase 0 PR)

1. **Sidecar ADOT vs regional ADOT?** Recommendation: regional team-owned ADOT (`otel.chat-cli.<region>.amazonaws.dev`) signed via SigV4. CLI offline ⇒ WAL buffers; that's acceptable.
2. **Cognito SigV4 reuse vs new IAM chain?** Recommendation: reuse existing `CognitoProvider` for OTLP auth — same trust boundary, no new credential surface, Phase 4 retires it cleanly.
3. **Workspace dep graph.** `chat-cli`, `chat-cli-v2`, `aws-toolkit-telemetry-definitions`, `kiro-telemetry`, `kiro-telemetry-cloudwatch` depend on `kiro-telemetry-schema`. Nothing else.
4. **`Event` IPC format.** Keep `Event` for Phase 1-3 (zero call-site churn), introduce `kiro_telemetry::record!()` macro for new metrics, deprecate `Event` in Phase 4.
5. **Account ownership.** Recommendation: dedicated chat-cli account for OTel EMF; cross-account observability for the 5 SEV alarms during shadow.
6. **Sampling for high-volume CLIs.** Bounded queue + P-tier dropping at backpressure. Revisit if cost > $500/month per fleet.

### Test strategy (gates Phase 0)

**(a) Unit tests — in-memory OTel reader.** `crates/kiro-telemetry/tests/unit_*.rs` use `opentelemetry_sdk::testing::metrics::InMemoryMetricExporter`. Every `EventType → metric` mapping has a test asserting name, unit, attribute set, attribute values, temporality.

**(b) Schema contract — EMF byte-equality.** `crates/kiro-telemetry-schema/tests/emf_contract.rs`: instantiate each metric, run through both `LegacyToolkitSink` (PostMetrics `MetricDatum`) and `OtelSink` (OTLP), pipe both through a mock `awsemf` exporter (lifted from ADOT source), assert EMF JSON byte-identical for the dimensions read by the 5 SEV alarms. **This is what protects production.** Failing this test blocks merge. Reads the namespace dimension from the legacy sink (`product=CodewhispererTerminal`), not from the doc.

**(c) Mock OTLP collector — integration tests.** `crates/chat-cli/tests/telemetry_otlp_integration.rs`: spin up `otelcol` in test (CI already pulls Docker) configured with `file` exporter; set `OTEL_EXPORTER_OTLP_ENDPOINT` to it; run `q chat` against a recorded Bedrock cassette; decode protobuf via `opentelemetry-proto`; assert metric set + resource attributes match expectation.

**(d) Opt-out E2E (the missing test).** `crates/chat-cli/tests/telemetry_opt_out_contract.rs`. Two parts:

- **In-process fuzzed (proptest):** generate every `EventType` variant with random fields under `KIRO_TELEMETRY_ENABLED=0` and assert: zero calls to `OtelSink::emit`, zero to `LegacyToolkitSink::emit`, zero to `LegacyCWhispererSink::emit`, zero bytes written to `${state_dir}/telemetry/`, zero outbound HTTP from a captured `reqwest::Client`. Bypass any `cfg!(test)` short-circuit so the consent path is exercised under test.
- **Cross-process binary test:** spawn the actual `chat-cli` binary with `KIRO_TELEMETRY_ENABLED=0`, run a scripted session covering tool calls, MCP init, voice, tangent, subagent paths. Point `OTEL_EXPORTER_OTLP_ENDPOINT` at a sink that fails the test on any received request. Assert zero export attempts.
- **Spool semantics test:** write spool entries with telemetry on, restart with `KIRO_TELEMETRY_ENABLED=0`, assert spool is empty before any export attempt and `telemetry.spool.purged_on_optout_total` increments.

**(e) GovCloud build verification.** `crates/chat-cli/tests/govcloud_build.rs`, gated `#[cfg(feature="govcloud")]`. Compile-time assertion that `LegacyToolkitSink` and `LegacyCWhispererSink` symbols are unreachable. Runtime test: emit every `EventType`, assert only outbound is the GovCloud ADOT endpoint.

**(f) Cardinality CI check.** Static lint walks `attributes!()` and `KeyValue::new` calls; rejects any attribute key whose enum is not registered in `schema/types.yaml`. Fuzz test emits 10,000 distinct values per dim; asserts the runtime limiter caps at the budget and `cardinality.overflow_total` increments.

**(g) Drop accounting invariant test.** Synthesize a session that emits, retries, drops; assert `success + permanent_failure + dropped == emit_count` per signal at session end.

**(h) Load test (out-of-band).** k6 drives 10k synthetic CLI invocations/sec against a real ADOT collector for 1h. Asserts `dropped{queue_full}=0` at default settings, queue p99 < 50% capacity, p99 export latency < 1s, ADOT memory bounded.

---

## 12. Appendix

### Appendix A — Glossary

- **ADOT** — AWS Distro for OpenTelemetry; the team-owned collector that converts OTLP to EMF/CloudWatch Logs.
- **anonymous_client_id** — 128-bit UUID generated locally on first run; pseudonymous user identifier; never a metric dimension.
- **Channel A** — legacy `PostMetrics` (SigV4 via Cognito) path through `crates/chat-cli/src/telemetry/mod.rs:778-808`.
- **Channel B** — legacy `SendTelemetryEvent` (Bearer via `auth.builder_id`) path through `mod.rs:680-775`, partial (only `ChatAddedMessage`/`AgentContribution`).
- **EMF** — Embedded Metric Format. CloudWatch Logs JSON payload from which CloudWatch auto-extracts metrics.
- **Fact row / fact table** — high-cardinality structured log record (one per turn/tool-call/conversation), routed to `/aws/chat-cli/facts`. Powers cohort/funnel SQL in Athena.
- **KUTS** — Kiro Unified Telemetry Service; emerging EventBridge-based replacement for Toolkit Telemetry. Future exporter target.
- **Meta-meter** — telemetry-on-telemetry pipeline, direct PutMetricData, ~20 series. Does not share fate with OTLP.
- **OTLP** — OpenTelemetry Protocol. We use HTTP/protobuf flavor.
- **Toolkit Telemetry** — the existing pipeline: `PostMetrics` → `ToolkitTelemetryLambda` (account `421629052180`) → CloudWatch Logs (EMF) → CloudWatch Metrics + Kinesis → Kibana.
- **WAL** — write-ahead log; the on-disk FIFO spool at `${state_dir}/chat-cli/telemetry/wal/` used for offline buffering and crash-time persistence.

### Appendix B — Glossary of priority levels

- **P0** — must ship in Phase 1; gates rollout; pages on-call when violated.
- **P1** — required for first PM dashboards (Phase 2); ticket on violation.
- **P2** — nice-to-have, deep-dive metrics; no alert.

### Appendix C — Example OTel Rust setup snippet (illustrative; not load-bearing)

```rust
// crates/kiro-telemetry/src/lib.rs (sketch — see crate for canonical impl)
use opentelemetry::{global, KeyValue};
use opentelemetry_sdk::{
    metrics::{PeriodicReader, SdkMeterProvider, Temporality},
    Resource,
};
use opentelemetry_otlp::{MetricExporter, WithExportConfig, Protocol};
use std::time::Duration;

pub fn init(cfg: TelemetryConfig) -> anyhow::Result<SdkMeterProvider> {
    if !cfg.opt_out_gate.allow_init() {
        cfg.respected_counter.add(1, &[]);
        return Ok(no_op_provider());
    }

    let exporter = MetricExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(cfg.endpoint)            // SigV4-signed via Cognito chain
        .with_timeout(Duration::from_secs(30))
        .with_temporality(Temporality::Delta)   // Counter/Histogram
        .build()?;

    let reader = PeriodicReader::builder(exporter)
        .with_interval(Duration::from_secs(60))
        .build();

    let resource = Resource::builder()
        .with_attribute(KeyValue::new("service.name", "kiro-cli"))
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")))
        .with_attribute(KeyValue::new("partition", cfg.partition.as_str()))
        .with_attribute(KeyValue::new("os.type", std::env::consts::OS))
        .with_attribute(KeyValue::new("host.arch", std::env::consts::ARCH))
        .build();

    let provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(resource)
        .build();

    global::set_meter_provider(provider.clone());
    Ok(provider)
}
```

### Appendix D — Decisions made (carried over from prior open questions)

1. Keep `Event` IPC format for Phase 1-3; introduce `kiro_telemetry::record!()` macro for new metrics; deprecate `Event` in Phase 4.
2. Workspace dep graph: only telemetry crates depend on `kiro-telemetry-schema`.
3. Schema CODEOWNERS require PM + SRE + DataEng + Security sign-off on `schema/*.yaml`.
4. ADOT auth: SigV4 via existing Cognito chain. **No baked Bearer token.**
5. GovCloud: compile-flag is the primary kill-switch; runtime guard is defense-in-depth.
6. Per-instrument temporality: Counter/Histogram=Delta; UpDownCounter/ObservableGauge=Cumulative.
7. SLO metrics are **derived recording rules in CloudWatch**, not emitted from the binary.
8. `kiro_cli_pii_redaction_coverage_ratio`, `telemetry_channel_parity_ratio` are derived recording rules, not exported gauges.
9. `panic_location`, `host_id_hash`, `price_table_version`, `feature_flag_assignments` map are **never** metric dimensions.

---

## Appendix E — KUTS (Kiro Unified Telemetry Service) integration

Sourced from a deep-dive on `code.amazon.com/packages/KiroUnifiedTelemetryService` (mainline). 8 parallel scouts + per-finding adversarial verification (`w5zxrr83v`). Verdicts: 3 confirmed, 4 mostly-confirmed, 1 has-errors (deployment/endpoints — corrections applied below).

### E.1 What KUTS actually is

**KUTS is a thin Java/Netty OTLP/HTTP gateway**, not a new SDK and not a new schema. It sits in front of an ADOT collector and adds auth, throttling, and metadata stamping. The "swap one exporter when KUTS GAs" line in §5 of this doc is correct — there is no SDK work, only configuration.

| Fact | Detail | Source |
|---|---|---|
| Owner | Bindle `KiroTelemetry`; team `kiro-controlplane`; primary committer Jack Wang (hanqiwa@); created 2025-11-20; active. | scout 1 |
| Build | Brazil + Gradle Kotlin DSL + JDK21 + Corretto-21 Docker; deployed to ECS Fargate behind ALB + WAF. | scout 1 |
| Endpoints | `POST /v1/metrics`, `POST /v1/traces`, `GET /health` on port `4318` (standard OTLP/HTTP). | scout 2, 3 |
| Wire format | OTLP — `application/x-protobuf` or `application/json`. KUTS validates only Content-Type; **never deserializes payload**. | scout 2, 3 |
| Max payload | **1 MiB** per request (`HttpObjectAggregator MAX_CONTENT_LENGTH = 1_048_576` in `TelemetryHttpServer.java`). | scout 3 |
| Status codes | 200 / 401 / 415 / 429 / 500. Auth + throttle errors are 4xx; only ADOT-forward failures surface as 5xx. | scout 3 |
| Auth methods | `Authorization: Bearer <token>` (currently a TODO that **rejects all bearer tokens** — `AuthenticationService.java:71-78`); OR `x-kiro-machineid: <id>` (the only working credential path today). 401 if neither. | scout 4 |
| Auth posture | **No SigV4, no Cognito, no AWS-IAM** — KUTS is intentionally unauthenticated-AWS-IAM by design (ingests external customer telemetry through ALB + WAF). | scout 4, 6 |
| Throttling | Dual-stage Wadi: pre-auth on `X-Forwarded-For` IP, post-auth on user-id or machine-id. `SyncFailBehavior.FAIL_OPEN`. | scout 3, 4 |
| Regions | **us-east-1 only.** Wiki R3: "Single region deployment in us-east-1 for now." No multi-region, no GovCloud. | scout 6 |
| Production status | **Prod infrastructure built but DARK — `ENABLE_KUTS_IN_PROD = false`** in `kiro-telemetry-architecture.md:488`. Three stages exist: personal/beta/gamma. | scout 8 |
| Downstream | `KUTS → sidecar ADOT @ localhost:4319 → CloudWatch (EMF metrics) + X-Ray (traces)`. KUTS itself emits ops metrics under namespace `KUTS/Service` with `Stage` dimension. | scout 2, 5 |
| Schema | **None imposed by KUTS.** Standard OTel semantic conventions. KUTS adds (does not require) four metadata headers when forwarding: `X-KUTS-Processed`, `X-KUTS-Auth-Status`, `X-KUTS-User-ID`, `X-KUTS-Machine-ID`. | scout 2, 5 |
| Rust client | **None exists.** `amzn-toolkit-telemetry-client` (legacy Toolkit Telemetry) is **not** KUTS-compatible. `KiroUnifiedTelemetryModelJavaClient` (31+ consumers) models the *internal* KRS/Lambda event model — **not** the OTLP ingest. Use plain `opentelemetry-otlp` (`http-proto`). | scout 7 |
| DNS pattern | Verifier-corrected: prod `telemetry.us-east-1.kiro.dev`; preprod `telemetry.{stage}-{region}.kiroservice.kiro.aws.dev`. Wiki claims a different shape — **must be confirmed with kiro-controlplane before hardcoding**. | scout 6 verifier (verdict: has-errors) |

### E.2 Architecture

```
+----------------------------------------------------------------------+
|                              chat-cli                                |
|                                                                      |
|  product code  ──►  kiro-telemetry facade  ──►  OTel Rust SDK 0.32   |
|                                                  (Counter/Histo/Log) |
|                                                         │            |
|                                                         ▼            |
|                                              opentelemetry-otlp      |
|                                              (http-proto exporter)   |
|                                                         │            |
|              headers: x-kiro-machineid: <stable-id>     │            |
|                       Authorization: Bearer <…> (TODO)  │            |
+---------------------------------------------------------┼------------+
                                                          │ HTTPS :443
                                                          │ POST /v1/metrics
                                                          │ POST /v1/traces
                                                          │ Content-Type:
                                                          │   application/x-protobuf
                                                          │ ≤ 1 MiB / req
                                                          ▼
+----------------------------------------------------------------------+
|  AWS WAF  ──►  ALB  ──►  KUTS (ECS Fargate, Java/Netty :4318)        |
|                          us-east-1 only (prod flag OFF today)        |
|                                                                      |
|   ┌─────────────────────── request pipeline ───────────────────────┐ |
|   │ 1. /health short-circuit                                       │ |
|   │ 2. POST-only check                  → 405                       │ |
|   │ 3. Pre-auth Wadi throttle (X-Fwd-For IP)  → 429                 │ |
|   │ 4. AuthenticationService                                       │ |
|   │      Bearer (TODO, rejects)                                    │ |
|   │      x-kiro-machineid (only working path today)  → 401 if both │ |
|   │ 5. Post-auth Wadi throttle (user-id or machine-id) → 429       │ |
|   │ 6. Content-Type validation (protobuf|json)        → 415        │ |
|   │ 7. Stamp X-KUTS-Processed / -Auth-Status /                     │ |
|   │       -User-ID / -Machine-ID, forward bytes verbatim           │ |
|   └────────────────────────────────────────────────────────────────┘ |
|                                                          │            |
|   KUTS ops metrics ─► CloudWatch ns "KUTS/Service"        │            |
|     (AuthSuccess/Failure, ThrottleAllowed/Rejected,       │            |
|      ForwardSuccess/Failure, *Latency, RequestCount)      │            |
+----------------------------------------------------------┼-----------+
                                                           │ HTTP
                                                           │ localhost:4319
                                                           ▼
+----------------------------------------------------------------------+
|  ADOT Collector (sidecar in same ECS task)                           |
|    receivers: otlp/http :4319                                        |
|    exporters: awsemf  ─► CloudWatch Logs /kiro/metrics  ─► CW Metrics|
|               otlphttp ─► X-Ray (traces, log group aws/spans)        |
+----------------------------------------------------------------------+
                                                           │
                          ┌────────────────────────────────┴─────────┐
                          ▼                                          ▼
                 CloudWatch Metrics                            X-Ray traces
                 (alarms, dashboards)                          (Application
                 us-east-1, prod inactive                       Signals)
```

Compared to §5 of this doc, the only differences are: (a) URL/port, (b) auth (`x-kiro-machineid` vs SigV4-via-Cognito), (c) the downstream EMF log group (`/kiro/metrics` vs `/aws/chat-cli/emf`).

### E.3 Integration recommendation for chat-cli

**TL;DR — do not build a KUTS-specific crate.** KUTS is OTLP/HTTP. Configure the existing `opentelemetry-otlp` exporter (already pinned at `~0.32.0` per §6 Cargo additions) with KUTS's URL and one custom header. The crate layout in §6 — `kiro-telemetry`, `kiro-telemetry-schema`, `kiro-telemetry-cloudwatch` — is sufficient. A `crates/kiro-telemetry-kuts` would be net-negative.

**1. Endpoint configuration.** Add a runtime override (parallel to the `KIRO_TELEMETRY_OTEL` env var already specified at `crates/chat-cli/src/launch.rs:137,143`):

```
KIRO_TELEMETRY_OTLP_ENDPOINT=https://telemetry.us-east-1.kiro.dev                     # KUTS prod (when on; CONFIRM DNS w/ kiro-controlplane)
                            =https://gamma-us-east-1.kiroservice.kiro.aws.dev          # KUTS gamma
                            =https://otel.chat-cli.<region>.amazonaws.dev              # team-owned ADOT (Phase 1-2 default per §5)
```

**2. Auth path.** Two layers, additive over the standard exporter:

```
exporter.with_header("x-kiro-machineid", machine_id())     // required today
exporter.with_header("Authorization", "Bearer <token>")    // future, when Kiro Auth Proxy ships
```

`machine_id()` reuses chat-cli's existing stable per-install identifier. **When swapping to KUTS, drop SigV4** (KUTS does not validate AWS IAM). The "no baked Bearer token ships in the binary" rule from §5 stands — Bearer is optional and currently unimplemented anyway.

**3. Schema mapping.** Nothing to do. The metric catalog in §6 (Counter/Histogram/Gauge with bounded dimensions per OTel semantic conventions) maps onto KUTS unchanged. KUTS forwards bytes.

**4. Rust SDK.** Plain `opentelemetry-otlp` with `http-proto`. No Smithy codegen. The four `X-KUTS-*` headers are server-added on egress, **not client-required**.

**5. Payload size.** KUTS caps at 1 MiB per request (`TelemetryHttpServer.java`). Re-validate the §6 batch sizing (`batch (10000/60s)`) against this ceiling on cutover day. Add a meta-meter counter `kiro_cli_kuts_export_oversize_total` so we catch silent drops.

**6. Downstream EMF log group.** KUTS-side ADOT writes `/kiro/metrics`, **not** `/aws/chat-cli/emf` (the design's chosen group at §6). Cutover to KUTS therefore implies a **second** consumer migration: the 5 SEV alarms (§1) and the Kibana consumer set (§7a) need to point at the KUTS-side log group. Budget 2w for this — see Phase 3.75 below.

### E.4 Sequencing — KUTS *augments then replaces* the team-owned ADOT collector

KUTS is **not** a parallel system to CloudWatch. It is the same shape as the team-owned ADOT in §5 with a different URL, auth header, and downstream EMF log group. The §5 Cognito-SigV4 stance is the right Phase-1 choice; KUTS adoption slides in as a future exporter swap once its prod flag flips and Bearer auth lands.

| Phase (§8) | KUTS interaction |
|---|---|
| **Phase 0 — Foundations** | None. |
| **Phase 1 — Dual-write commercial** | None. OTLP target is the team-owned ADOT. **Add the capability** to override exporter URL via `KIRO_TELEMETRY_OTLP_ENDPOINT` so the future swap is config-only. |
| **Phase 2 — Product KPIs + alarm shadow** | None. Kibana consumer migration assumes `/aws/chat-cli/emf`. Do **not** point them at `/kiro/metrics` yet. |
| **Phase 3 — Outcomes + alarm cutover** | None *in the cutover*. Alarms move to OTel-derived EMF in `/aws/chat-cli/emf`. |
| **NEW Phase 3.5 — KUTS dogfood (4w, gated)** | **Pre-reqs:** kiro-controlplane sets `ENABLE_KUTS_IN_PROD=true`; us-east-1 prod traffic routed; SLO dashboard public; Bearer auth either landed or formally deferred. **Action:** internal Amazon dogfood week 1 → 1% prod week 2 → 10% week 3 → 50% week 4. Dual-emit to *both* team-owned ADOT and KUTS. |
| **NEW Phase 3.75 — KUTS swap (2w)** | 100% prod via KUTS; team-owned ADOT deprovisioned (or kept warm for failover); alarms re-pointed to KUTS-side `/kiro/metrics`; consumers re-signed-off. |
| **Phase 4 — Decommission Channel A** | Unchanged. Toolkit Telemetry retires regardless of KUTS state. |

**Why not earlier?**

1. `ENABLE_KUTS_IN_PROD = false` (scout 8). Cutting over to a service with no live prod traffic violates G3 ("CloudWatch alarm parity ≥30 days at ≤0.5%").
2. Bearer auth is a TODO (scout 4). `x-kiro-machineid` works but lacks a documented format/persistence spec.
3. us-east-1 only (scout 6). The §5 partition-isolation requirement (`home_region → us-east-1 → us-west-2`, GovCloud strict) is unsatisfiable with KUTS today.
4. Phase 2's Kibana consumer migration locks dashboards onto `/aws/chat-cli/emf`; re-pointing them to `/kiro/metrics` is a separate change-management cycle.

**Why not skip the team-owned ADOT and go straight to KUTS?** Tempting but wrong. KUTS prod is dark; we'd be migrating *to* a non-prod target. The 1 MiB cap is unmeasured against our batches. Bearer is unimplemented, so user-scoped dashboards are blocked from day 1.

### E.5 Risks specific to KUTS adoption

| ID | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| K1 | KUTS prod flag never flips, or flips and rolls back | Medium | High — Phase 3.5 stalls | Gate Phase 3.5 entry on a written sign-off from kiro-controlplane that prod has been live ≥30d at ≥99.95%; otherwise stay on team-owned ADOT indefinitely. |
| K2 | 1 MiB request cap drops large batch exports silently | Medium | Medium — metric loss / parity drift | Tune `BatchProcessor max_export_batch_size`; add `kiro_cli_kuts_export_oversize_total` meta-meter; OTel exporter already raises errors on 4xx — wire to existing drop counters. |
| K3 | Bearer auth (Kiro Auth Proxy) never lands; user-scoped dashboards blocked | Medium | Medium — limits Phase 2+ KPI granularity by user | Plan all dashboards machine-scoped first (already aligned with §3 P2 stretch goals). |
| K4 | us-east-1-only KUTS violates partition isolation (§5: GovCloud no-fallback) | High today | High in GovCloud, Medium commercial | Keep team-owned ADOT alive in non-us-east-1 until KUTS multi-region ships. **Do not route GovCloud through KUTS.** |
| K5 | Wadi throttle limits unknown — could 429 normal traffic during incident bursts | Medium | Medium | Exponential backoff in OTel exporter (default), `kuts_throttle_429_total` meta-meter, request documented quota from kiro-controlplane before Phase 3.5. |
| K6 | DNS / endpoint disagreement between wiki and infra code | Low | Medium — could ship wrong URL | scout 6 verifier flagged conflicting forms (`{stage}.us-east-1.telemetry.kiro.aws.dev` wiki vs `telemetry.{region}.kiro.dev` infra). Confirm before Phase 3.5; do not hardcode either. |
| K7 | KUTS-side ADOT writes `/kiro/metrics`, not `/aws/chat-cli/emf` — alarm re-cut required | High (if we adopt KUTS) | Medium — re-runs Phase 2 consumer migration | Explicit Phase 3.75 subtask "re-point 5 SEV alarms + Kibana consumers"; budget 2w. |
| K8 | KUTS is operationally young — minimal runbook, no public SLO dashboard | Medium | High during incidents | Refuse sole-exporter status until kiro-controlplane publishes (a) SLO dashboard, (b) oncall runbook, (c) tickets-per-week baseline. Until then dual-emit. |
| K9 | chat-cli is a customer with no protocol leverage | Low | Medium | Stick to vanilla OTLP; never depend on `X-KUTS-*` headers in product code (server-added, not client-required). |
| K10 | "Inherit KUTS" assertion (§5) underestimates the consumer-side cutover | Medium | Medium | This appendix makes the cutover explicit (Phase 3.5/3.75); §5 should be amended to reference this appendix. |

### E.6 Open questions — need answers before any chat-cli code ships against KUTS

1. **Production endpoint DNS.** Wiki and infra code disagree (see K6). Authoritative answer needed from kiro-controlplane.
2. **`ENABLE_KUTS_IN_PROD` flip date** and the gates kiro-controlplane has on that flip.
3. **`x-kiro-machineid` format requirements** beyond non-empty — UUID? length cap? charset? Affects how chat-cli generates and persists the value.
4. **Bearer / Kiro Auth Proxy identity model.** Will it accept BuilderID, Cognito identity tokens, AWS SSO, or something else? Determines whether chat-cli's existing Cognito chain can be reused for the bearer path or must be replaced.
5. **Wadi throttle quotas** per machine-id and per IP. Drives batch size and flush interval.
6. **Multi-region timeline** (eu-central-1, us-west-2, GovCloud). Without it we cannot retire the team-owned ADOT outside us-east-1.
7. **Gamma account ID `050752648305`** — scout 6 verifier could not confirm in infra code.
8. **Authoritative downstream EMF log group** — `/kiro/metrics` (scout 5), `/aws/chat-cli/emf` (this design), or both? Determines consumer-migration scope for the swap.
9. **OTLP conformance fixtures** — does KUTS publish integration tests with golden OTLP payloads we can reuse?
10. **Kiro InfoSec sign-off** on the unauthenticated-by-design posture (Bearer TODO + machine-id-only) for routing prod customer telemetry through KUTS.

### E.7 Decisions made

1. **Do not build a KUTS-specific crate.** Reuse `opentelemetry-otlp` from `kiro-telemetry`.
2. **Endpoint is config-only.** Introduce `KIRO_TELEMETRY_OTLP_ENDPOINT` in Phase 1 so the future swap is a one-env-var change.
3. **Auth changes on the swap day.** SigV4-via-Cognito for the team-owned ADOT (§5); `x-kiro-machineid` header for KUTS. Bearer optional.
4. **KUTS adoption is Phase 3.5 / 3.75**, gated on the open questions above.
5. **GovCloud and non-us-east-1 partitions stay on the team-owned ADOT** until KUTS ships multi-region.
6. **Update §5 of this doc** in a follow-up edit: the "swap one exporter when KUTS GAs" line is correct in spirit but glosses over the consumer-side cutover documented here as Phase 3.75.

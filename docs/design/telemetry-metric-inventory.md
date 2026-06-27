# Telemetry metric inventory — current state → desired future state

**Status:** Canonical telemetry reference for PR #3191 (drop-collector / KUTS-direct). As-shipped.
**Owner:** vinayshah1998
**Branch of record:** `vinayshah/telemetry-drop-collector-kuts-direct`.

This documents **every** metric on three surfaces: (A) the 26 distinct legacy V2 Toolkit `MetricDatum`
metrics, (B) the OTEL catalog (`metrics.yaml`), (C) the metrics the TUI emits for V2/V3.
Every wired/dormant/dark verdict was verified by reading the source on the branch of record, not inferred
from the catalog — the `legacy_mapping.yaml` map and the catalog's `parity_tolerance` markers describe
intent, not what the translator actually emits.

**Legend** — Current: `wired` = emitted by production code today · `dormant` = constructor exists,
no caller · `server` = computed downstream (batch/derived), never client-emitted · `log-only` =
sampled but shipped as a log, no metric series. Survives = state after the V2 `legacy_sink` is
deleted (stage-5): `metric` / `log` / **dark** / `fidelity-loss` (bare counter survives, dims lost).

---

## What shipped (PR #3191)

The big-picture architecture: **TUI and the Rust launcher both emit OTLP directly to KUTS** — no
bundled collector, no notarized sidecar, no installer weight.

```
┌─ user machine ────────────────────────────────────────────────┐
│                                                                │
│  thin Rust launcher ──OTLP/HTTP protobuf (+x-kiro-machineid)── │──► KUTS
│   • cli_session_started (pre-TUI)                              │   prod
│   • cli_session_completed (post-TUI, true exit_reason)         │
│   • install / login / auth / consent / heartbeat               │
│   • in-process OTel SDK, flush-on-exit                          │
│                                                                │
│  TUI (TS) ──────────OTLP/HTTP metrics (fire-and-forget)──────  │──► KUTS
│   • render/input/turn UX metrics, KAS turn-completion          │   (same endpoint)
│   • lossy by design: drop + emitFailedTotal, no retry          │
│                                                                │
│  KAS subprocess ── its own OTel pipeline, its own endpoint ──  │──► (not ours)
└────────────────────────────────────────────────────────────────┘
```

What shipped, corrected against the mid-flight plan:

- **KUTS is metrics-only.** KUTS accepts OTLP **metrics** (`/v1/metrics` 200); `/v1/logs` 404s. The
  entire OTLP **logs** path — `emitOtlpLog`, the `kas-*` host-side log events, the local Loki stack — was
  **removed**. Signals that mattered for dashboards are preserved as metrics (§C2, §C4). What remains of
  `otlp-emit.ts` is the shared in-process drop counter the metrics SDK reports export failures through.
- **TUI metrics go through the OTel JS SDK** (`packages/tui/src/utils/meter.ts`); the observer is
  `tui-telemetry-observer.ts`, engine-neutral, on meter scope `kiro.tui` (§C, §D, §H).
- **Launcher owns the endpoint.** `launch.rs` resolves the effective KUTS endpoint (the same one the Rust
  host uses) and passes it to the TUI via `KIRO_TELEMETRY_OTLP_ENDPOINT` — unset ⇒ disabled, never
  loopback. No bundled collector; the `kiro-telemetry-collector` crate stays in-tree as a dormant,
  **opt-in** path (`KIRO_TELEMETRY_COLLECTOR_BIN`) for the rare multi-emitter topology (kiro-bot, build
  farms), never default-spawned.
- **Legacy→OTEL parity closed** before the V2 `legacy_sink` deletion (§A, §G): `UserLoggedIn`,
  `AuthFailed`, V1 `DailyHeartbeat`, and the three `UiMode*` events. The
  `every_event_type_emits_an_otel_metric_or_log` regression test is the guardrail (it caught the
  `UiMode*` arms, which were absent from `legacy_mapping.yaml`). Consciously dropped: `AgentContribution`
  line counts and the `kiro_cli_feature_used_total` fidelity collapse.
- **Raw `model` dimension, no pricing.** The old `model_class` bucket enum was replaced by a free-form
  raw `model` dimension; the `estimated_cost_usd` / pricing-table machinery was removed entirely. Cost is
  computed downstream from raw per-model usage. See §C4/§F/§H — "economics" now means tokens +
  context_usage only.
- **`kiro_cli_` prefix standardization** applied catalog-wide (§I).
- **Validated end-to-end** against the local finch Prometheus/Grafana stack
  (`dev/telemetry/validate-metrics-e2e.sh`, fixtures `dev/telemetry/emit-tui-metrics.fixture.ts`) and
  against prod KUTS (raw `engine=v2`/`engine=v3` probes accepted, queryable end-to-end; no prod KUTS
  traffic from the local stack). The `engine` split survives ingest — see §H.5.
- **Production continuity (Gate A, resolved):** KUTS accepts the TUI's OTLP + machine-id; the
  prefix rename splits historical series at cutover, so queriers must be repointed (§I).

---

## A. Legacy V2 Toolkit metrics (the `Toolkit` / `CodewhispererTerminal` namespace)

Source: `event_to_metric_datum()` in `kiro-telemetry-legacy/src/event_translation.rs:75-798`
(V2) and `chat-cli/src/telemetry/core.rs:111` (V1), POSTed via
`V2LegacySink` (`chat-cli-v2/src/telemetry/legacy_sink.rs:334-364`). **All of this dies when the
sink is deleted** — the "Survives" column is the whole point of this table.

| # | Legacy metric | EventType | V1/V2 | Survives deletion | OTEL successor (future) |
|---|---|---|---|---|
| 1 | `userLoggedIn` | UserLoggedIn | both | **metric** ✓ | `kiro_cli_user_logged_in_total` — wired `event_translation.rs:807` (PR #3191 closed this; was the one hard-dark gap) |
| 2 | `refreshCredentials` | RefreshCredentials | both | fidelity-loss | `kiro_cli_auth_credential_failure_total` bare counter via `_=>` fallthrough; requestId/result/reason/oauthFlow dropped |
| 3 | `authFailed` | AuthFailed | both | **metric** ✓ | `kiro_cli_auth_credential_failure_total` — rich arm wired `event_translation.rs:815` (`auth_failed_login_from_names`); keeps auth_method+error_code |
| 4 | `cliSubcommandExecuted` | CliSubcommandExecuted | both | **metric** (partial) | `kiro_cli_feature_used_total` (feature=subcommand); clientApplication/inCloudshell dropped |
| 5 | `chatSlashCommandExecuted` | ChatSlashCommandExecuted | both | **metric** (partial) | `kiro_cli_slash_command_invoked_total`; subcommand/result/reason dropped |
| 6 | `chatStart` | ChatStart | both | fidelity-loss | `kiro_cli_chat_session_started_total` bare counter via `_=>`; model/conversation dropped. (Rich arm is on the separate `ChatSessionStarted` event.) |
| 7 | `chatEnd` | ChatEnd | both | **log** ✓ | metric arm is `Vec::new()` (`:822`) but emits `kiro_cli_conversation_completed` log; `completion_reason` hardcoded |
| 8 | `chatAddMessage` | ChatAddedMessage | both | **metric** (rich fan-out) | `kiro_cli_user_turns` + ~15-record `ModelResponseMetrics` fan-out (ttft, tokens, cache, bedrock.*; raw `model` dim, no cost). V1 datum omits token/appType dims; V2 carries them |
| 9 | `recordUserTurnCompletion` | RecordUserTurnCompletion | both | **log + metric** ✓ | `kiro_cli_user_turn_completed` log + `UserTurnCompletionMetrics` fan-out (turns, tokens, duration; raw `model` dim, no cost) |
| 10 | `tangentModeSession` | TangentModeSession | both | fidelity-loss | `kiro_cli_feature_used_total` bare counter via `_=>`; duration/entriesRemoved VALUE has no OTEL home. (Piggybacks on `chatSlashCommandExecuted` datum.) |
| 11 | `toolUseSuggested` | ToolUseSuggested | both | **metric + log** ✓ | `kiro_cli_tool_call_total` + `kiro_cli_tool_invocations` + `kiro_cli_tool_execution_duration_ms` + `kiro_cli_tool_invoked` log; token-size dims dropped |
| 12 | `agentContribution` | AgentContribution | both | **DARK** ✗ | metric arm hard `Vec::new()` (`:821`, ALLOWED_SILENT). **No OTEL metric, no log.** `linesByAgent`/`linesByUser` consciously dropped. Also loses its CodeWhisperer accepted-line event with the sink |
| 13 | `mcpServerInit` | McpServerInit | both | **metric + log** ✓ | `kiro_cli_mcp_server_init_total` + `kiro_cli_mcp_server_connected_total` + `kiro_cli_mcp_server_init` log; tool-name lists dropped |
| 14 | `agentConfigInit` | AgentConfigInit | both | fidelity-loss | `kiro_cli_feature_used_total` bare counter via `_=>`; agentsLoaded/migration dims dropped (no feature name) |
| 15 | `didSelectProfile` | DidSelectProfile | both | fidelity-loss | `kiro_cli_feature_used_total` bare counter via `_=>`; source/result/region/profileCount dropped |
| 16 | `profileState` | ProfileState | both | fidelity-loss | `kiro_cli_feature_used_total` bare counter via `_=>`; all dims dropped |
| 17 | `messageResponseError` | MessageResponseError | both | **metric** ✓ | `kiro_cli.bedrock.request.errors` (model+reason+status_code); conversation/clientApp dropped. Backs the QCLIFault/QCLIError alarms |
| 18 | `dailyHeartbeat` | DailyHeartbeat | both | **metric** ✓ (gains dims) | `kiro_cli_daily_heartbeat` (client_application+install_method). PR #3191 wired V1 to match V2 |
| 19 | `subagentInvocation` | SubagentInvocation | both | **log** ✓ | `kiro_cli_subagent_invoked` log; builtinToolUses/mcpToolUses counts go dark |
| 20 | `voiceInput` | VoiceInput | both | fidelity-loss | `kiro_cli_feature_used_total` bare counter via `_=>`; all voice backend/duration dims dropped |
| 21 | `processHealthMetric` | ProcessHealthMetric | **V2 only** | **metric** (partial) | `kiro_cli.process.memory.rss` + cpu via `process_health_records`; render/heap/yoga dims dropped (see §C/§E) |
| 22 | `modeChanged` | ModeChanged | **V2 only** | fidelity-loss | `mode_active_users_weekly` gauge(1.0) via `_=>`; from/to/source dropped |
| 23 | `goalCompleted` | GoalCompleted | **V2 only** | **metric** ✓ | `kiro_cli_session_outcome_total` (terminal_state); iterations/duration dropped |
| 24 | `uiModeSessionStart` | UiModeSessionStart | **V2 only** | **metric** ✓ | `kiro_cli_ui_mode_session_started_total` — wired `event_translation.rs:998`. **No `legacy_mapping.yaml` entry**; caught by parity regression test |
| 25 | `uiModeChanged` | UiModeChanged | **V2 only** | **metric** ✓ | `kiro_cli_ui_mode_changed_total` — wired `:1004`. Not in yaml map |
| 26 | `uiModeDefaultChanged` | UiModeDefaultChanged | **V2 only** | **metric** ✓ | `kiro_cli_ui_mode_default_changed_total` — wired `:1007`. Not in yaml map |

**Legacy tally (26 distinct metrics):** 15 survive as a metric (some partial) · 2 survive as a log only ·
8 fidelity-loss (bare counter, dims dropped) · **1 fully dark (`agentContribution`)**. (The translator has
27 datum-emitting `EventType` arms: the 26 distinct names above plus a separate `ChatSessionStarted` arm
that routes to the same `kiro_cli_chat_session_started_total`.) The 3 `UiMode*` rows (24–26) are V2-only datums
absent from `legacy_mapping.yaml`; PR #3191's `every_event_type_emits_an_otel_metric_or_log` test is what
prevents them (and any future arm) from silently going dark.

`agentContribution` (`:821`) and `ChatEnd` (`:822`) are the only two OTEL metric arms hard-coded to
`Vec::new()`; `ChatEnd` still emits the `kiro_cli_conversation_completed` log, so `agentContribution` is
the sole fully-dark metric. Note the distinction the source forces: the legacy *datum builder*
(`event_translation.rs:75-798`) and the *OTEL record* function (lines 800+) are separate matches — a
`Vec::new()` in the datum builder (e.g. `:759`) does not mean the OTEL side is dark. `userLoggedIn`,
`authFailed`, and all three `UiMode*` events are wired on the OTEL side (`:807`/`:815`/`:998-1007`).

---

## B. OTEL catalog (`kiro-telemetry-schema/schema/metrics.yaml`) — 89 entries

Constructors in `kiro-telemetry/src/metric.rs` + `log.rs`. **46 wired** today · **29 dormant**
(constructor exists, no production caller) · **11 server-computed** adoption gauges (nightly batch,
not per-process) · **3 derived** SLO (no constructor at all). 46 + 29 + 11 + 3 = 89.
(The two pricing metrics `kiro_cli_estimated_cost_usd` and `kiro_cli_pricing_table_active` were
removed in PR #3191 — cost is computed downstream from the raw `model` dimension.)

### B1. Wired today (46) — emitted by production code

Lifecycle/UI: `kiro_cli_session_started_total`, `kiro_cli_chat_session_started_total`, `kiro_cli.session.completed`,
`kiro_cli_daily_heartbeat`, `kiro_cli_user_logged_in_total`, `kiro_cli_ui_mode_session_started_total`,
`kiro_cli_ui_mode_changed_total`, `kiro_cli_ui_mode_default_changed_total`.
Model/turn/tokens: `kiro_cli_model_invocations_total`, `kiro_cli_user_turns`,
`kiro_cli_user_turn_duration_seconds`, `kiro_cli_time_to_first_chunk_ms`, `kiro_cli_tokens_consumed`,
`kiro_cli_cache_hit_ratio`,
`kiro_cli_context_usage_percentage`, `kiro_cli.bedrock.stream.ttft`, `kiro_cli.bedrock.stream.duration`,
`kiro_cli.bedrock.request.duration`, `kiro_cli.bedrock.stream.inter_token_latency`,
`kiro_cli.bedrock.request.errors`, `kiro_cli.bedrock.empty_response.retries`, `kiro_cli.retry.attempts`,
`kiro_cli.retry.exhausted`.
Tools/MCP: `kiro_cli_tool_call_total`, `kiro_cli_tool_invocations`, `kiro_cli_tool_execution_duration_ms`,
`kiro_cli_mcp_server_init_total`, `kiro_cli_mcp_server_connected_total`.
Features/session: `kiro_cli_feature_used_total`, `kiro_cli_slash_command_invoked_total`, `kiro_cli_session_outcome_total`.
Process health (V2 host only): `kiro_cli.process.memory.rss`, `kiro_cli.process.cpu.utilization`.
Security/SDK self-obs: `kiro_cli_auth_credential_failure_total`, `kiro_cli_pii_redaction_runs_total`,
`kiro_cli_pii_redaction_matches_total`, `kiro_cli_consent_record_integrity_total`, `kiro_cli_govcloud_channel_disabled_total`,
`kiro_cli_govcloud_channel_leak_total`, `kiro_cli_kuts_export_oversize_total`.
Log events (6): `kiro_cli_metering_event`, `kiro_cli_user_turn_completed`, `kiro_cli_tool_invoked`,
`kiro_cli_mcp_server_init`, `kiro_cli_subagent_invoked`, `kiro_cli_conversation_completed`.

### B2. Dormant (29) — constructor exists, **no production caller** (future: wire or delete)

| Group | Metrics | Future intent |
|---|---|---|
| **Process/perf (host or TUI)** | `kiro_cli.process.fds.open` (`metric.rs:3948`), `kiro_cli.process.threads` (`:3955`), `kiro_cli.process.memory.growth_rate` (`:3937`), `kiro_cli.startup.duration` (`:4217`), `kiro_cli.startup.failures` (`:4206`), `kiro_cli.crash.total`, `kiro_cli.agent.loop.iteration_duration`, `kiro_cli.agent.loop.stuck`, `kiro_cli.upstream.dependency.up` | **WIRE these** — they back the §E perf expansion at near-zero cost (constructors ready) |
| **Product** | `kiro_cli_upgrade_completed_total`, `kiro_cli_user_feedback_total`, `kiro_cli_message_regenerated_total`, `kiro_cli_client_identity` (log; the DAU distinct-count source — Phase 4), `kiro_cli_feature_first_use` (log) | Wire as product signals mature; `client_identity` ties to the flock→distinct-count DAU plan |
| **Telemetry self-obs** | `kiro_cli.telemetry.exporter.send.attempts`/`.duration`/`.dropped`, `kiro_cli.telemetry.queue.depth`, `kiro_cli.telemetry.batch.size`, `kiro_cli.telemetry.emit.failures`, `kiro_cli.telemetry.sdk.up`, `kiro_cli.telemetry.flush_on_exit.dropped_total`, `kiro_cli.meta_meter.up` | Wire when SDK health monitoring lands. (`kiro_cli_kuts_export_oversize_total` is the **only** `kiro_cli.telemetry.*`/self-metric wired today) |
| **Security** | `kiro_cli_telemetry_opt_out_respected_total`, `kiro_cli_telemetry_opt_out_violation_total`, `kiro_cli_auth_unexpected_identity_total`, `kiro_cli_tls_validation_failure_total`, `kiro_cli_tool_egress_destinations_total`, `kiro_cli_pii_redaction_errors_total` (error variant never built by `RedactionOutcome::metric_records`) | Wire as the security/consent surface is exercised |

### B3. Server-computed adoption gauges (11) — constructor present but only ever batch-emitted

`active_users_daily/weekly/monthly`, `dau_mau_ratio`, `new_users_daily`, `client_version_seen`,
`version_adoption_pct`, `stale_version_users`, `feature_unique_users_weekly`, `tool_using_sessions_pct`,
`mode_active_users_weekly`. **Future: stay server/batch-computed** (`metric.rs:3117` notes them as
nightly rollups, never per-process). Client constructors exist only for the golden-catalog test.

### B4. Derived SLO (3) — no constructor, computed downstream

`kiro_cli.slo.success_rate`, `kiro_cli.slo.availability`, `kiro_cli_pii_redaction_coverage_ratio`.
**Future: stay derived** (recording rules in the metrics backend).

---

## C. V3/TUI client metrics

Source: `packages/tui/src/utils/tui-telemetry-observer.ts` + `process-health-collector.ts`, emitted via
the OTel JS SDK (`meter.ts`) direct to KUTS. Engine is told apart by a first-class **`engine`** attribute
(`v2`/`v3`) on every metric. The TUI metric scope is `kiro.tui` (engine-neutral) for both engines.

**The TUI layer is under-instrumented.** Only 4 metrics are emitted, dimensioning is inconsistent
(2 of 4 carry `client_application`, none carry a first-class `engine`), and the richest signals the TUI
already holds — per-turn tokens, tool outcomes, CPU/memory — leave the process unobserved. The
sub-sections below give (C1) what exists, (C3) the standard every TUI metric must follow, and (C4) the
metrics to add.

### C1. Metrics emitted today (4)

| Metric | Kind | Engine dim today | Notes |
|---|---|---|---|
| `kiro_cli_chat_session_started_total` | counter | scope **+ `client_application`** | `recordV3SessionStarted`; `version_minor_bucket` hardcoded `current` |
| `kiro_cli_user_turns` | counter | scope **+ `client_application`** | `recordV3UserTurn`; `is_subagent` hardcoded `false` (sub-agent turns never reach wire) |
| `kiro_cli_user_turn_duration_seconds` | histogram | **scope only** | catalog forbids `client_application`; histogram point suppressed when KAS omits duration (no phantom 0s) |
| `kiro_cli_tool_call_total` | counter | **scope only** | catalog forbids `client_application`; `outcome` only ever `success`/`error` on V3; `tool_origin` ≈ always `builtin` |

### C2. Logs — REMOVED (KUTS is metrics-only)

The TUI used to emit six OTLP **logs** over the `/v1/logs` path (the `kiro_cli_tool_invoked`
tool-name log + five `kas-*` host-side events). **KUTS does not support OTLP logs
(`/v1/logs` 404s), so this entire path was removed** — along with the hand-rolled log emitter,
the host-side CLI shim, and the local dev Loki stack. The data that
mattered for dashboards is preserved as **metrics**: the `kas-*` session/turn events
became the §C4 `recordTui*` emissions, and tool latency rides
`kiro_cli_tool_execution_duration_ms`. The only thing genuinely dropped is the
high-cardinality `tool_name` breakdown (it was metric-forbidden, log-only). What
remains of `otlp-emit.ts` is just the shared in-process drop counter
(`recordEmitDrop`/`getEmitFailedTotal`) the metrics SDK reports export failures through.

Resource attrs on **every** TUI metric (`buildResource`, `meter.ts`): `service.name=kiro-tui`
(identical for v2/v3/kas — engine **not** distinguished at resource level), `service.version`,
`kiro.machine_id` (+ `x-kiro-machineid` header), `kiro.user_id` (when set). Emission is fire-and-forget:
disabled when `KIRO_TELEMETRY_OTLP_ENDPOINT` unset; drops counted in-process only.

### C3. Standard dimensioning contract (apply to every TUI metric)

**`engine` is the single canonical discriminator. Every TUI metric carries it; no metric needs
`client_application`.**

- **`engine`** = `v2 | v3` — the coarse, always-present engine split. New required attribute (§D).
- **`version_minor_bucket`** — stop hardcoding `current` (today's `recordV3SessionStarted` does); pass the
  real bucket from the launcher.

`engine` rides as a **per-metric attribute**, not a resource attribute (`service.name` is identically
`kiro-tui` across engines, so resource-level can't distinguish them). This requires adding `engine` to the
schema allowed-attribute set for the two metrics that reject `client_application` today
(`kiro_cli_user_turn_duration_seconds`, `kiro_cli_tool_call_total`). Scope stays as a secondary signal for back-compat.

**Why not `client_application` at the TUI.** The TUI hardcodes `client_application = chat_cli_v3`
(`tui-telemetry-observer.ts:37`) on every record — and `ClientApplication::from_name` maps `v3`/`kas` →
`chat_cli_v3` (`metric.rs:149`), so on the TUI path `client_application` *is* `engine` with a `chat_cli_`
prefix: 1:1, zero added information. The values that make `client_application` interesting — `kiro_ide`,
`acp_external` — are an **embedder/surface** axis that is only ever set on the Rust/host path
(`get_cli_client_application` env var; observer `AppType::Acp → AcpExternal`). The TUI is structurally always
the CLI, so a surface dimension there would be a constant. Drop `client_application` from the TUI contract;
leave it (or a future dedicated `surface` attribute) as a host-only concern. See §D.

### C4. TUI metrics to add (close the thin-layer gap)

The TUI already has the data for all of these — most are a metric-emission of a value currently sent only
as a log. All carry the §C3 contract.

| Proposed metric | Kind | Source the TUI already has | Why |
|---|---|---|---|
| `kiro_cli_tokens_consumed` (V3 emitter) | counter | `kas-turn-completion` token counts (`normalizeKasTurnCompletion`) | per-engine token economics — wired on V2 host, **no V3 emitter today** |
| `kiro_cli_turn_outcome_total` | counter | `status` field of `kas-turn-completion` | bucketed `failure_reason`; today only in the log |
| `kiro_cli_model_invocations_total` (V3 emitter) | counter | raw `model` id (already in observer) | per-engine model mix (raw `model` dim); V2-only today |
| `kiro_cli_tool_execution_duration_ms` (V3 emitter) | histogram | `executionDurationMs` on `recordV3ToolCall` | tool latency by engine; the value is captured but only logged |
| `kiro_cli_context_usage_percentage` (V3 emitter) | gauge | `contextUsagePercentage` in turn-completion | context pressure by engine; V2-only today |
| `kiro_cli_subagent_delegations_total` | counter | `_meta.kiro.pipeline` (tool-origin detection) | v3/KAS sub-agent fan-out; today only the `kiro_cli_subagent_invoked` log, counts go dark |
| `kiro_cli_mode_active_total` | counter | `currentModeId` (already on session/turn metrics) | per-engine mode usage; avoids the `mode_active_users_weekly` lost-dims problem |
| **Process/perf set** (CPU, RSS, heap, peak RSS, event-loop, input latency, render) | gauge/histogram | `process-health-collector.ts` snapshot | **see §E** — captured at the TUI level today, log-only |

Also fix the two correctness gaps in the existing emitters: `is_subagent` is hardcoded `false` on
`kiro_cli_user_turns` (sub-agent turns never reach the wire), and `version_minor_bucket` is hardcoded
`current` on `kiro_cli_chat_session_started_total`.

---

## D. Engine dimensioning — consolidate three overlapping axes onto one `engine` attribute

**Today there are three near-duplicate engine signals, none of them clean:**
1. OTLP **scope** name — universal on TUI
   records, but Prometheus does not reliably preserve scope as a queryable label, so a dashboard **cannot**
   split V2/V3 by it for the duration histogram or tool counters (they are scope-only — no usable label at
   all). This is why scope is **not** the discriminator; `engine` is. (The TUI scope is now uniformly
   `kiro.tui` — see §H.3 — so it does not encode the engine at all.)
2. **`client_application`** (`chat_cli | chat_cli_v2 | chat_cli_v3 | kiro_ide | acp_external | _other_`) —
   conflates **engine** (`_v2`/`_v3`) with **surface/embedder** (`kiro_ide`/`acp_external`). On the TUI it's
   a hardcoded constant (`chat_cli_v3`), so it duplicates engine; the surface values only ever appear on the
   host path.
3. **`agent_kind`** (`v1 | v2 | subagent | kas`) on the `kiro_cli.process.*` metrics — a finer host-side
   runtime detail that also encodes the engine (`kas` = v3).

**Future — one canonical axis.** Add a first-class `engine = v2 | v3` to `types.yaml` (no `engine` attr
exists today; only `agent_kind` at `types.yaml:90`) and put it on **every** perf/product metric, including
the two that reject `client_application`. Source it directly:
- **TUI:** thread `engine` through every record fn in `tui-telemetry-observer.ts` (defaulting to `v3` for
  the KAS path); the v2 RustAcpClient path passes `engine: 'v2'` explicitly.
- **Host:** derive from `agent_kind` (`kas → v3`; `v1 | v2 | subagent → v2`). `agent_kind` stays as the
  finer host-side detail; `engine` is the coarse 2-value rollup layered on top.

It rides as a **per-metric attribute** (not resource-level — `service.name` is identically `kiro-tui` across
engines). Scope stays as a back-compat secondary signal.

**On the other two axes:** drop `client_application` from the TUI contract entirely (§C3 — it's a constant
there). It remains meaningful only on the **host**, and only for its `kiro_ide` / `acp_external`
**surface/embedder** values — ideally split into a dedicated `surface` attribute later so `engine` and
`surface` stop overlapping. `agent_kind` is fine to keep where it is. Net target model:
**`engine` (everywhere) · `agent_kind` (host runtime detail) · `client_application`→`surface` (host
embedder only).**

---

## E. Performance metrics — current (captured, unemitted) → future (emit as metric)

**CPU% and memory are already sampled at the TUI level** every 60s by `process-health-collector.ts`,
which runs *inside the bun TUI process* and reads `process.memoryUsage().rss`, `process.cpuUsage()`,
`process.resourceUsage()` (peak RSS), bun/JSC heap, event-loop delay, input latency, and render timings.
The gap is the **transport**: this snapshot used to ship as a host-side process-health log (now removed
with the logs path, §C2) or an ACP notification to the host (V2), so **there is no Prometheus series for
CPU or memory** and no engine-split perf panel. The work is to **emit the existing sampler as metrics**
plus wiring the already-dormant `kiro_cli.process.*` constructors — not new instrumentation.

> **RSS** (Resident Set Size) = the process memory currently resident in physical RAM (excludes
> swapped-out and never-touched pages). It is the standard "real memory in use right now" figure;
> `peak_rss` is its high-water mark over the process lifetime.

| Future metric | Kind | Captured | Today | Action |
|---|---|---|---|---|
| `kiro_cli.process.cpu.utilization` ✅req | gauge¹ | TUI + host | unemitted (V3); wired (V2 host) | emit as OTLP gauge; constructor `metric.rs:3887` |
| `kiro_cli.process.memory.rss` ✅req | gauge | TUI + host | unemitted (V3); wired (V2 host) | emit (bytes); constructor `:3872` |
| `kiro_cli.process.memory.peak_rss` | gauge | TUI | unemitted | emit; **needs exit-flush** (monotonic) |
| `kiro_cli.process.memory.heap_used` | gauge | TUI | unemitted | emit; JS-only (no Rust analog) |
| `kiro_cli.tui.event_loop.delay` | histogram | TUI | unemitted | emit raw histogram (re-aggregatable) |
| `kiro_cli.tui.input.latency` | histogram | TUI | unemitted | emit raw histogram |
| `kiro_cli.tui.render.duration` | histogram | TUI | unemitted | emit twinki render timings |
| `kiro_cli.process.fds.open` | gauge | host | dormant | wire `:3948` (host owns pid tree) |
| `kiro_cli.process.threads` | gauge | host | dormant | wire `:3955` |
| `kiro_cli.startup.duration` ✅req | histogram | host | dormant | wire `:4217` (v2-vs-v3 launch cost) |
| `chat_cli.subprocess.spawn.duration` | histogram | host | none | **new** constructor (v3 bun+KAS spawn) |
| `kiro_cli.process.memory.growth_rate` | — | — | dormant `:3937` | **derive downstream** via PromQL `deriv()`; don't emit |

¹ Existing constructor is a histogram; a gauge is recommended for a 60s point sample.

**Ownership:** host samples the native pid tree (its own + KAS subprocess + children via `sysinfo`:
RSS/CPU/FDs/threads); TUI samples itself + JS-runtime-only signals (heap, event-loop, input, render). Both
tag a proposed `process_role` (`host|tui|kas_subprocess`) so the same metric reassembles the tree
downstream. **Flush-on-exit bug (load-bearing):** the TUI teardown (`process-health-collector.ts`) only
`clearInterval` — it never takes a final sample, so peak-RSS and crash-adjacent CPU are lost (compounded by
fire-and-forget transport). Fix: final snapshot + bounded-timeout `await emitOtlpMetric` on exit/SIGINT.
Treat exit samples as lossy — population quantiles, never per-session SLOs.

---

## F. Product metrics — engine-dimensioned, cross-surface (future)

The concrete TUI product metrics to add are in **§C4**. This section is the cross-cutting rule: the same
metric name must be emitted from **both** engines with the **§C3 contract** so V2-vs-V3 is a clean label
split, not a scope or surface artifact.

- **Backfill the V2-only metrics on the V3 path.** `kiro_cli_model_invocations_total`, `kiro_cli_tokens_consumed`,
  `kiro_cli_context_usage_percentage`, and `kiro_cli_session_outcome_total` are wired
  on the V2 host but have **no V3 emitter** — V3 usage is invisible for these until the TUI emits them
  (§C4). Reuse the same metric name + add `engine`, don't fork a `_v3` variant.
- **Add `engine` to the metrics already emitted on both** (`kiro_cli_chat_session_started_total`,
  `kiro_cli_user_turns`, `kiro_cli_tool_call_total`) so the existing series become splittable without scope.
- **New cross-engine product signals:** `kiro_cli_turn_outcome_total` (bucketed `failure_reason`),
  `kiro_cli_subagent_delegations_total`, `kiro_cli_mode_active_total` — defined in §C4.

---

## G. Gaps & sequencing (summary)

1. **`agentContribution` is the only fully-dark metric** — conscious drop (`linesByAgent`/`linesByUser`
   have no OTEL home). Decide: accept, or add a `lines_contributed` counter before sink deletion.
2. **8 fidelity-loss legacy metrics** collapse to bare `kiro_cli_feature_used_total`/`kiro_cli_auth_credential_failure_total`
   counters via the `_=>` fallthrough — accepted per PR #3191, but dims are gone for good.
3. **Sequencing (hard rule):** parity arms (§A) must land **before** the stage-5 `legacy_sink` deletion,
   or every fidelity-loss + dark metric regresses. The `every_event_type_emits_an_otel_metric_or_log` test
   is the guardrail.
4. **Engine attribute (§D)** is the prerequisite for any V2-vs-V3 dashboard split on the duration histogram
   and tool counters — add `engine=v2|v3` to `types.yaml` first, then apply the §C3 contract everywhere.
5. **The TUI layer is thin and inconsistently dimensioned (§C).** Only 4 metrics, none carry a first-class
   `engine`, and dimensioning is split-brained across scope/`client_application`. Consolidate onto
   `engine`-only (§C3, §D) and add the §C4 metrics —
   most are just metric-emitting a value the TUI already logs.
6. **Perf is a promotion, not new capture (§E)** — CPU%/RSS are sampled at the TUI level today but log-only;
   wire the dormant `kiro_cli.process.*`/`startup.*` constructors and fix the flush-on-exit bug.
7. **29 dormant catalog metrics (§B2)** — triage into wire-now (perf), wire-later (product/security/SDK),
   or delete-from-catalog. Don't leave them as permanent dead schema.

---

## H. v2 metrics from the TUI — the TUI's experience of the agent (plan, not yet built)

> **Status: design for review.** The SDK migration (PR #3191) made the TUI a first-class `engine`-tagged
> emitter, but the observer is wired into `KasAcpClient` (v3/KAS) **only** and hardcodes `engine='v3'`.
> This section plans extending it so the TUI **also** emits for the v2 engine, as the client's own
> experience of the agent. (Note: §C1–C2 above describe the pre-SDK state and are superseded by the PR
> #3191 implementation — `meter.ts` + `engine=v2|v3`; they're kept for historical contrast.)

### H.1 Why the TUI should observe v2 at all (the host already emits v2)

The Rust **host** already emits v2 metrics server-side (`service.name=kiro-cli`, meter scope
`kiro-telemetry`, `otel.rs:299,413`). But the host's view is the *server's* view. The **TUI is the only
vantage that sees what the user actually experiences**: wall-clock turn latency including render, the tool
calls as they surface in the UI, session lifecycle as the user drives it, and the TUI process's own
CPU/RSS/event-loop. That client-experience signal is valuable for v2 exactly as it is for v3 — and the TUI
is the single client that talks to **both** engines (`RustAcpClient` = v2, `KasAcpClient` = v3, both extend
`BaseAcpClient` and receive the same `ToolCall`/`ToolCallFinished`/turn-completion events).

### H.2 Decision: mirror the v3 set for v2, minus the economic metrics

`RustAcpClient` emits the same **client-experience** metric set `KasAcpClient` does, with `engine=v2` — no
`_v2`-suffixed variants, same names/scope/attributes. **Exception (H.6 resolved):** the **economic**
metrics — `kiro_cli_tokens_consumed` and `kiro_cli_context_usage_percentage` —
are **NOT** emitted from the TUI on the v2 path; they stay host-authoritative (the Rust host already emits
them and is the source of truth for billing; a TUI copy adds no fidelity, only double-count risk).
(Cost is no longer a metric at all — it is computed downstream from the raw per-model usage; §C4.)

**v2-from-TUI metric set (engine=v2):** `kiro_cli_chat_session_started_total`, `kiro_cli_user_turns`,
`kiro_cli_user_turn_duration_seconds`, `kiro_cli_tool_call_total`, `kiro_cli_tool_execution_duration_ms`,
`kiro_cli_turn_outcome_total`, `kiro_cli_model_invocations_total`, `kiro_cli_mode_active_total`,
`kiro_cli_subagent_delegations_total`, and the process-health set (`kiro_cli.process.memory.rss`/`peak_rss`/
`heap_used`, `kiro_cli.process.cpu.utilization`, `kiro_cli.tui.event_loop.delay`/`input.latency`/
`render.duration`). The v3/KAS path is **unchanged** — it still emits the full set including economics,
because for v3 the TUI is the only client-side vantage.

### H.3 The double-counting problem (the one real risk) and how it's resolved

Mirroring means the **same metric name** (`kiro_cli_user_turns`, `kiro_cli_tokens_consumed`, …) is now
emitted for v2 by **two** producers: the Rust host AND the TUI. A naive `sum(kiro_cli_user_turns{engine="v2"})`
would double-count. The disambiguator is the OTLP resource attribute **`service.name`** (`kiro-cli` for the
host, `kiro-tui` for the TUI), set on every record's resource — no new attribute needed:

| Producer | `service.name` | meter scope | `process_role` |
|---|---|---|---|
| Rust host (v2 server view) | `kiro-cli` | `kiro-telemetry` | `host` |
| TUI (v2 client experience) | `kiro-tui` | `kiro.tui`¹ | `tui` |

¹ The TUI meter scope is the engine-neutral `kiro.tui` — **done** in this work: the previous engine-tagged
scope was a misnomer once the observer served both engines, so it was renamed so scope denotes *surface*
(TUI) and `engine` denotes *engine*. The two axes no longer overlap (same "stop conflating engine with
surface" principle as §D).

> **⚠️ Where `service.name` survives — verified against live KUTS (2026-06-25).** `service.name` is a
> **resource attribute**, and whether you can split on it depends on which destination you read:
> - **The new `KiroCLI` CloudWatch namespace** (KUTS ADOT change `990d295`, log group `/kuts/kiro-cli/metrics`):
>   the collector's `filter/kirocli` keys on `resource.attributes["service.name"] in {kiro-cli, kiro-tui}`,
>   and `awsemf/kirocli` emits there. `service.name` is intact at the collector's OTLP input (confirmed: the
>   raw OTLP we send carries it as a resource attr), so this is the destination where the host-vs-TUI split
>   works. **This is the canonical place to read these metrics.**
> - **The legacy `/kiro/metrics` forwarded path** (`kuts.forwarded=true` records, namespace `kiro`):
>   `service.name` is **stripped/flattened away** by the time records land here — the live records carry
>   `OTelLib`, `engine`, `model`, etc. but **no `service.name`** and **no `_aws` block** (so they are
>   logs only, not CloudWatch metrics). Do **not** rely on `service.name` for dedup here; it isn't present.
> Net: the host-vs-TUI dedup is a property of the **`KiroCLI` namespace path**, not the flattened
> `/kiro/metrics` logs.

**Dashboard rule (must document for consumers):** in the `KiroCLI` namespace, for any metric the host also
emits, split by `service.name` (`kiro-cli` = server truth, `kiro-tui` = client experience). They are **not**
additive — pick one per panel. Use `kiro-tui` for UX/latency panels (what the user feels), `kiro-cli` for
economic panels (the authoritative server count). Token/context usage especially stays
**host-authoritative** for billing; there is no TUI copy on the v2 path (see H.6).

### H.4 Implementation sketch

1. **Parameterize the observer.** ✅ Done. The hardcoded engine in the observer became an
   `engine: 'v2' | 'v3'` argument threaded through every record fn (and the tool-call observer), defaulting
   to `v3` so KAS call sites are unchanged. The module and its exports were renamed to drop the `V3` prefix
   (now `tui-telemetry-observer.ts`, `recordTui*`, `TuiToolCallObserver`, `TUI_SCOPE`, `DEFAULT_ENGINE`)
   since it's no longer v3-only.
2. **Wire `RustAcpClient`.** Mirror the `KasAcpClient` wiring: a `ToolCallObserver` instance, the
   `recordSessionStarted/UserTurn/ToolCall/...` calls on the same ACP events, passing `engine='v2'`. Both
   subclasses share the event shapes via `BaseAcpClient`, so the wiring is near-identical — consider lifting
   the shared observe-and-emit calls into `BaseAcpClient` with an abstract `engine` getter so neither
   subclass duplicates it.
3. **Scope rename** ✅ Done: the TUI meter scope is now the engine-neutral `kiro.tui` (H.3¹).
4. **Process-health already does v2.** `process-health-collector.ts` already derives `agent_kind`
   v2-vs-kas from the snapshot — generalize its `engine` the same way (it currently forces `engine=v3`).
5. **Schema:** no new metric names; `engine=v2` is already an allowed value (verified accepted by KUTS
   prod *and* visible as a queryable EMF field end-to-end — see H.5). `service.name`/scope are
   resource/scope identity, already present. Downstream destination exists: the `KiroCLI` CloudWatch
   namespace (KUTS ADOT `990d295`) routes `service.name in {kiro-cli, kiro-tui}` to `/kuts/kiro-cli/metrics`.

### H.5 Validation

- Unit: observer tests assert `engine=v2` on every metric when constructed for the v2 engine; `RustAcpClient`
  wiring tests mirror the `KasAcpClient` ones.
- E2E (local stack): `dev/telemetry/validate-metrics-e2e.sh` (fixtures
  `dev/telemetry/emit-tui-metrics.fixture.ts`) emits an `engine=v2` and an `engine=v3` batch and asserts
  both `engine="v2"` and `engine="v3"` series appear in Prometheus, distinct by `service.name`.
- **Wire + CloudWatch (proven against KUTS prod, 2026-06-25):**
  - KUTS `/v1/metrics` accepts `engine=v2` counters + gauges (HTTP 200 "Accepted"). `/v1/logs` 404s — KUTS
    is metrics-only, which is why the OTLP logs path was removed (§C2).
  - All new §C/§E TUI metrics were confirmed landed (TUI SDK emit + a raw `engine=v2` probe), queried via
    Logs Insights on `/kiro/metrics` (account `615299732016`, profile `kuts_telemetry_prod_read-only`).
    **`engine` is a queryable field there with both `v2` and `v3` values** — so the engine split survives
    ingest end-to-end. This de-risks the whole v2-from-TUI dimensioning before a line of it is written.
  - **Caveat:** those `/kiro/metrics` records have **no `_aws` EMF block**, so they exist as **logs only**,
    not CloudWatch *metrics*. Native, graphable/alarmable metrics come from the **`KiroCLI` namespace**
    (KUTS ADOT change `990d295`, log group `/kuts/kiro-cli/metrics`) — validate the v2-from-TUI work there
    once deployed (filter `service.name="kiro-tui"`, split by `engine`).
- **Query gotcha (cost us time once):** `version_minor_bucket` is a **closed enum**
  (`current|current-1|current-2|older|_other_`) clamped client-side in `versionMinorBucketFromEnv`
  (`tui-telemetry-observer.ts`); an arbitrary probe tag is silently rewritten to `_other_`, so it can't be
  used as a query needle. Identify a run by **`OTelLib`** (meter scope) + the **metric name** instead —
  `machineId` is also absent from these records. TUI records now carry `OTelLib = "kiro.tui"` (the
  engine-neutral scope from the §H.3 rename).

### H.6 Resolved: full-mirror minus economics

**Decision (2026-06-25):** the v2-from-TUI path mirrors the client-experience metrics but **omits the
economic metrics** (`kiro_cli_tokens_consumed` and `kiro_cli_context_usage_percentage`). Rationale: the
TUI only relays token numbers the agent already reported — it adds no fidelity over the host, which is the
billing source of truth, and a duplicate copy only risks double-counting. The exact emitted set is in H.2.
(v3/KAS is unaffected — it keeps the full set, since for v3 the TUI is the only client vantage. Cost is no
longer a metric on either path; it is computed downstream from raw per-model usage.)

---

## I. Metric prefix standardization (`kiro_cli_`)

Every CLI-emitted metric gets a `kiro_cli` prefix so the catalog reads as one product namespace.
Four decisions:

1. **Catalog-wide, this PR.** All emittable product + SDK-self metrics are renamed in one sweep, not
   drip-fed — a half-prefixed catalog is worse than either end state (two conventions to remember, two
   query patterns).
2. **Backend adoption gauges excluded.** The 11 batch-computed adoption gauges (`active_users_*`,
   `dau_mau_ratio`, `new_users_daily`, `client_version_seen`, `version_adoption_pct`,
   `stale_version_users`, `feature_unique_users_weekly`, `tool_using_sessions_pct`,
   `mode_active_users_weekly` — §B3) are **not** CLI-emitted; they're computed downstream in the nightly
   rollup and named by the data-eng pipeline. The client never owns those names, so prefixing them here
   would only desync from the producer.
3. **SDK self-metrics folded into `kiro_cli`.** The telemetry-on-telemetry set (§B2 self-obs, §5.10) is
   our own emission too, so it joins the namespace: `telemetry.*` → `kiro_cli.telemetry.*`,
   `meta_meter.up` → `kiro_cli.meta_meter.up`, `kuts_export_oversize_total` →
   `kiro_cli_kuts_export_oversize_total`.
4. **Dotted hierarchy kept.** Dotted names stay dotted — only the leading segment swaps
   (`chat_cli.<...>` → `kiro_cli.<...>`); the `.`-segmented hierarchy is the OTel-idiomatic form and the
   Prometheus mangling (`kiro_cli_<...>`) falls out of it unchanged.

**Session-name collision.** Two session counters mangle toward the same Prometheus name if naively
prefixed, so they're disambiguated explicitly: `chat_session_started_total` →
`kiro_cli_chat_session_started_total` (first prompt sent) and `cli_session_started_total` →
`kiro_cli_session_started_total` (process launch). Keep them distinct — one is top-of-funnel, the other
is first-engagement.

**Old → new (representative).**

| Old | New |
|---|---|
| `cli_session_started_total` | `kiro_cli_session_started_total` |
| `chat_session_started_total` | `kiro_cli_chat_session_started_total` |
| `tool_call_total` | `kiro_cli_tool_call_total` |
| `model_invocations_total` | `kiro_cli_model_invocations_total` |
| `feature_used_total` | `kiro_cli_feature_used_total` |
| `slash_command_invoked_total` | `kiro_cli_slash_command_invoked_total` |
| `session_outcome_total` | `kiro_cli_session_outcome_total` |
| `ui_mode_*_total` | `kiro_cli_ui_mode_*_total` |
| `auth_credential_failure_total` | `kiro_cli_auth_credential_failure_total` |
| `pii_redaction_*` | `kiro_cli_pii_redaction_*` |
| `govcloud_channel_*_total` | `kiro_cli_govcloud_channel_*_total` |
| `telemetry_opt_out_*_total` | `kiro_cli_telemetry_opt_out_*_total` |
| `chat_cli.session.completed` | `kiro_cli.session.completed` |
| `chat_cli.bedrock.*` | `kiro_cli.bedrock.*` |
| `chat_cli.process.*` | `kiro_cli.process.*` |
| `chat_cli.tui.*` | `kiro_cli.tui.*` |
| `chat_cli.slo.*` | `kiro_cli.slo.*` |
| `telemetry.exporter.*` / `.queue.*` / `.batch.*` / `.emit.*` / `.sdk.*` / `.flush_on_exit.*` | `kiro_cli.telemetry.<same>` |
| `meta_meter.up` | `kiro_cli.meta_meter.up` |
| `kuts_export_oversize_total` | `kiro_cli_kuts_export_oversize_total` |

> **⚠️ Production continuity.** These were **live names** in CloudWatch and Grafana — renaming them
> **splits the historical time series** at cutover (old name stops, new name starts; no automatic
> stitching). Any dashboard panel, recording rule, or alarm querying an old name **must be migrated** to
> the new name, or it silently goes flat. Inventory the queriers (the §5 SEV alarms, the local Grafana
> dashboards, any Athena/Logs-Insights saved queries) and repoint them as part of the rename, not after.
> The `chat_cli`/`chat_cli_v2`/`chat_cli_v3` **`client_application` attribute values are unchanged** — only
> metric *names* move.

# KUTS telemetry architecture and metric dimension review

**Status:** Client implementation and local validation complete; production rollout pending.
**Last updated:** 2026-07-24.

This is the canonical reference for the Kiro CLI KUTS architecture, emitted metric catalog, product
questions, and CloudWatch dimension contracts. The schema source of truth is
`crates/kiro-telemetry-schema/schema/metrics.yaml`; this document explains why that catalog has its
current shape and records the compatibility-first production rollout.
Developers adding or changing metrics should follow the
[telemetry development SOP](../../dev/telemetry/README.md).

## Current architecture

V1, V2, and V3/KAS emit OTLP metrics directly to the regional KUTS endpoint. The product does not
bundle an OpenTelemetry collector and does not send OTLP logs to KUTS. KUTS forwards accepted metrics
to its server-side ADOT collector, which writes direct EMF records to `/kuts/kiro-cli/metrics` and
creates `KiroCLI` CloudWatch metric series from explicit metric declarations. A separately tagged copy
continues through KiroTelemetry to `/kiro/metrics`.

Legacy Toolkit and CodeWhisperer telemetry remain separate compatibility channels until their
independent cutovers are approved. Those events continue to support existing Kibana workflows; they
are not additional KUTS metric dimensions. In particular, the V1 `amazonq_startChat` and
`amazonq_endChat` lifecycle events remain Toolkit-only.

Metric ownership follows the surface that can count the event exactly once:

- V1 owns V1 session, turn, model, tool, and process observations.
- The interactive TUI owns interactive V2 session and turn observations.
- The Rust V2 host owns session and turn observations for external ACP clients.
- V3/KAS and TUI observations share the reviewed contracts and use `agent_engine=v3`.
- Host and TUI process observations use `process_role` to identify the process being measured.

The schema deliberately separates two concepts:

- `attributes` are fields carried on the OTLP datapoint and retained in the raw EMF record.
- `cloudwatch_dimensions` are the bounded fields promoted into CloudWatch metric series.

The KUTS `metric_declarations` must match `cloudwatch_dimensions`. A diagnostic attribute such as
`mcp_server_name` can therefore remain queryable in Logs Insights without creating one CloudWatch
series per raw value. The declarations are deployed independently from this repository and must land
before the client catalog is released.

## Audit baseline

The review started from the following production state:

- The KUTS change deployed on 2026-07-20 dropped `user_id`, but the `awsemf/kirocli` exporter had no
  per-metric dimension declarations.
- The EMF exporter therefore promoted every datapoint attribute to a CloudWatch dimension.
- The client schema had no runtime mechanism that capped open attribute values or controlled the
  exported dimension set.
- The 2026-07-22 pre-review inventory had 100 entries: 47 recently active metrics, 42 dormant
  instruments, 8 log events, and 3 derived metrics. The active metrics occupied 35,131 CloudWatch
  series.
- `OTelLib` identified the OTel instrumentation scope (`kiro-telemetry` for Rust and `kiro.tui` for
  the TUI). It was producer provenance, not a product-analysis dimension.

That 100-entry inventory is retained below only as the reconciliation ledger. The implemented client
catalog contains 42 emitted metrics.

## Review rules

1. Every metric must have one explicit product or operational question.
2. A dimension is included only when a dashboard, alarm, or investigation needs that breakdown.
3. Exact released and nightly versions remain selectable through one stable `version_full` key.
4. User, session, conversation, request, and other unique identifiers are never metric dimensions.
5. `OTelLib` is excluded from product metrics. Producer provenance belongs in raw EMF logs or
   dedicated telemetry-pipeline health signals.
6. Percentages and rollups are derived from base counters when CloudWatch metric math can compute
   them reliably.
7. High-cardinality discovery questions belong in logs or analytics unless a bounded metric
   vocabulary is explicitly agreed.
8. For `install_method`, unsupported or unattributable values collapse to `unknown`; there is no
   separate user-facing `_other_` value.
9. Custom Kiro metric names use underscore-separated `kiro_cli_...` names. Dots do not create
   hierarchy in CloudWatch and are rewritten by some Prometheus-compatible tooling.
10. A raw diagnostic field may ride on a metric datapoint only when the KUTS EMF declaration
    explicitly excludes it from CloudWatch dimensions. This still creates an attribute set in the
    emitting client's OTel SDK, so use the pattern only for low-frequency, locally bounded events.

## Shared dimensions

| Dimension | Meaning | Decision |
|---|---|---|
| `version_full` | Exact CLI version, including stable and nightly versions such as `2.5.1` and `2.5.2-nightly.14` | Use on the selected usage metrics and the adoption heartbeat. Do not collapse nightlies into a shared bucket. |
| `release_channel` | Release family independent of exact version | Use on the adoption heartbeat. Target values include `stable`, `nightly`, and any separately supported prerelease channel. |
| `session_interface` | How the session was invoked | Bounded values: `interactive_cli`, `noninteractive_cli`, `external_acp`. |
| `agent_engine` | Agent implementation/runtime used to serve the session | Bounded values: `v1`, `v2`, `v3`, `unknown`. KAS is normalized to `v3`. |
| `agent_mode` | Product mode selected for the session | Bounded values: `default`, `plan`, `spec`, `autonomous`, `custom`. All other agent profile IDs map to `custom`. |
| `os_type` | Operating system of the active installation | Bounded values: `linux`, `macos`, `windows`, `unknown`. |
| `install_method` | How the CLI was installed | Bounded values: `brew`, `internal_toolbox`, `unknown`. Until an installation receipt exists, the website installation script is reported as `unknown`. |

`client_application` is overloaded today. For session and turn usage, `session_interface` expresses the
user-visible invocation surface while `agent_engine` expresses the implementation. Those two dimensions
replace `client_application` on the reviewed usage metrics.

## Confirmed metric decisions

Rows in this section compare the pre-review catalog with the approved contract. The implementation
status and final names are recorded in the current emitted catalog below.

| Metric | Question and count point | Dimensions | Decision |
|---|---|---|---|
| `kiro_cli_chat_session_started_total` | How many chat sessions are created? Count at session creation, before the first prompt. | `version_full`, `session_interface`, `agent_mode`, `agent_engine` | Keep. Remove custom agent name, raw mode, `client_application`, and `OTelLib`. |
| `kiro_cli_user_turns` | How many completed user turns occur in each usage cohort? | `version_full`, `session_interface`, `agent_mode`, `agent_engine` | Keep. Do not break down by custom agent name. |
| `kiro_cli_model_invocations_total` | How many logical model requests did the agent make, and which model handled them? Count each tool-loop continuation as another invocation, but count transport retries separately. | `version_full`, `agent_engine`, `model` | Keep. Preserve the canonical service-provided model ID without a client-side allowlist. |
| `kiro_cli_tokens_consumed` | How many model tokens did Kiro consume, by model and engine, for workload and cost analysis? | `version_full`, `agent_engine`, `model`, `token_type` | Keep. Omit session interface, agent mode, and execution context. |
| `kiro_cli_metering_event` | Previously attempted to export request-level service metering as an OTLP fact log. | None | Replace with `kiro_cli_credits_consumed`; KUTS does not accept OTLP logs. |
| `kiro_cli_credits_consumed` | How many service-reported credits are consumed by each CLI version and model? | `version_full`, `model` | Add as an additive counter. Emit only finite, non-negative values whose canonical service unit is `credit`; do not include request IDs or unit strings. |
| `kiro_cli_cache_hit_ratio` | What percentage of input-token volume was served from cache? | None | Remove from client producers. Derive the token-weighted percentage from `kiro_cli_tokens_consumed` in CloudWatch metric math. |
| `kiro_cli_context_usage_percentage` | Previously sampled how full the model context was at turn completion. | None | Remove from client producers. It is turn-weighted rather than user-weighted and has no current dashboard decision. Use context-limit outcomes and a supported analytics sink instead. |
| `kiro_cli_user_turn_duration_seconds` | How long does Kiro actively spend completing a successful top-level user turn? | `version_full`, `session_interface`, `agent_mode`, `agent_engine` | Keep. Exclude failed, cancelled, and subagent turns. Exclude time waiting for user approval where measurable. |
| `kiro_cli_time_to_first_chunk_ms` | Previously measured from a Rust-host model request to the first arbitrary stream event. | None | Replace with explicit model first-content and client first-visible-response metrics. |
| `kiro_cli.bedrock.stream.ttft` | Duplicate backend first-event timer with different units and dimensions. | None | Remove and replace with the same two explicit latency metrics. |
| `kiro_cli_model_time_to_first_content_ms` | After Kiro sends a logical model request, how long until the host receives the first text, reasoning, or tool-use content? | `version_full`, `agent_engine`, `model` | Add as the backend/model diagnostic. Ignore metadata-only events. |
| `kiro_cli_time_to_first_visible_response_ms` | After a user submits a prompt, how long until a first-party client visibly renders the first meaningful agent output? | `version_full`, `session_interface`, `agent_mode`, `agent_engine` | Add as the product responsiveness metric for clients Kiro owns. Do not claim coverage for external ACP renderers. |
| `kiro_cli.bedrock.stream.duration` | Duplicates logical model-request duration with completion-reason dimensions. | None | Remove and replace with `kiro_cli_model_request_duration_seconds`. |
| `kiro_cli.bedrock.request.duration` | Duplicates logical model-request duration with operation and outcome dimensions. | None | Remove and replace with `kiro_cli_model_request_duration_seconds`. |
| `kiro_cli_model_request_duration_seconds` | How long does each logical model request take from sending the request until its stream completes, fails, or is cancelled? | `version_full`, `agent_engine`, `model`, `model_request_outcome` | Add as the canonical model-request duration histogram. Include internal retry time. |
| `kiro_cli.bedrock.request.errors` | Previously counted model-request failures with raw failure codes and HTTP status classes. | None | Replace with `kiro_cli_model_request_failure_total`. |
| `kiro_cli_model_request_failure_total` | How often do logical model requests fail, including failures later recovered, and what type of problem caused them? | `version_full`, `agent_engine`, `model`, `error_kind` | Add as a failure-only counter. Keep raw failure reasons and HTTP status details in local logs and legacy telemetry. |
| `kiro_cli.bedrock.stream.inter_token_latency` | Previously measured gaps between arbitrary stream chunks rather than tokens. | None | Remove without replacement. The high-volume observations do not support a current dashboard or alarm decision. |
| `kiro_cli_turn_outcome_total` | Previously counted non-success turns while mixing user interruptions with product failures. | None | Replace with `kiro_cli_turn_failure_total`. |
| `kiro_cli_turn_failure_total` | How many top-level user turns fail, and for what bounded reason? | `version_full`, `session_interface`, `agent_mode`, `agent_engine`, `turn_failure_reason` | Add as a failure-only counter. Exclude user cancellations and recovered failures. |
| `kiro_cli_turn_cancelled_total` | How many top-level user turns are explicitly cancelled before a terminal result? | `version_full`, `session_interface`, `agent_mode`, `agent_engine` | Add as the denominator adjustment for client-turn availability. Count once per user-cancelled turn; do not treat it as a failure. |
| `kiro_cli_session_started_total` | Previously counted top-level CLI launch starts under an ambiguous session name. | None | Replace with `kiro_cli_run_started_total`; keep distinct from chat-session creation. |
| `kiro_cli_run_started_total` | How many top-level CLI invocations begin? | `version_full`, `session_interface`, `agent_engine`, `os_type` | Add as the denominator for run outcomes and crash rates. |
| `kiro_cli.session.completed` | Previously labeled any nonzero observed child or one-shot exit as a crash and had no V1 producer. | None | Replace with `kiro_cli_run_outcome_total`. It is not a crash detector. |
| `kiro_cli_run_outcome_total` | How do top-level CLI invocations terminate when the launcher can observe the result? | `version_full`, `session_interface`, `agent_engine`, `os_type`, `run_outcome` | Add as the canonical launcher-observed run outcome counter. |
| `kiro_cli.crash.total` | How many CLI runs experienced an abnormal process termination, and in which process role? | `version_full`, `agent_engine`, `os_type`, `process_role`, `crash_kind` | Rename to `kiro_cli_crash_total` and implement a durable crash-receipt producer. The pre-review instrument was catalog/test-only. |
| `kiro_cli.startup.duration` | How long from process entry until the selected interface is ready for useful work? | `version_full`, `session_interface`, `agent_engine`, `os_type` | Rename to `kiro_cli_startup_duration_seconds` and wire a real producer. Record successful startups only; remove `cold_start`. |
| `kiro_cli.startup.failures` | How many CLI runs fail before becoming ready, and at which bounded stage? | `version_full`, `session_interface`, `agent_engine`, `os_type`, `startup_failure_stage` | Rename to `kiro_cli_startup_failure_total` and wire a real producer. Count one terminal startup failure per run. |
| `kiro_cli.retry.attempts` | Previously counted each additional automatic retry attempt separately. | None | Replace with `kiro_cli_automatic_retries_per_operation`. |
| `kiro_cli.retry.exhausted` | Previously counted retry sequences that ended without recovery. | None | Remove as a dedicated metric. Use `kiro_cli_automatic_retries_per_operation{retry_outcome=exhausted}`. |
| `kiro_cli.bedrock.empty_response.retries` | V1-specific automatic empty-response recovery with recovered/still-empty outcomes. | None | Remove as a dedicated metric. Fold it into `kiro_cli_automatic_retries_per_operation` with `retry_reason=empty_response`. |
| `kiro_cli_automatic_retries_per_operation` | How often do operations require automatic retries, how deep are the retry sequences, and do they recover? | `version_full`, `agent_engine`, `retry_reason`, `retry_outcome` | Add as a histogram observed once when an automatically retried operation finishes. Record the number of additional attempts, excluding the original attempt and user-initiated retries. |
| `kiro_cli.agent.loop.stuck` | Proposed detecting agent loops that stop making progress, but no production watchdog emits it. | None | Remove the dormant metric, constructor, and schema entry. Use request, tool, and turn timeout signals plus local diagnostic logs instead. |
| `kiro_cli.upstream.dependency.up` | Proposed a client-emitted `1/0` gauge for upstream service availability, but has no production emitter. | None | Remove the dormant metric, constructor, and schema entry. Use service-side canaries for global availability and client request, retry, and authentication failures for user impact. |
| `kiro_cli_ui_mode_session_started_total` | How many first-party interactive CLI launches start in the full TUI versus Lite? Count once after resolving the launch layout. | `version_full`, `ui_mode` | Keep. Bound `ui_mode` to `tui` or `lite`; move selection source and persisted-default detail to logs or Amplitude. |
| `kiro_cli_ui_mode_changed_total` | Previously counted successful switches between the full TUI and Lite during a session. | None | Remove from CloudWatch metrics. Repeated toggles make event counts a poor proxy for preference or adoption; use Amplitude for switching funnels. |
| `kiro_cli_ui_mode_default_changed_total` | Previously counted edits to the persisted TUI-versus-Lite preference. | None | Remove from CloudWatch metrics. It counts setting changes rather than the current active-installation preference; use actual launch mode for usage and Amplitude for preference funnels. |
| `mode_active_users_weekly` | A mode-change event was incorrectly translated into a client-emitted gauge value of `1`, which cannot represent weekly active users. | None | Remove the client constructor, schema entry, and `ModeChanged` translation. Do not emit active-user rollups from individual clients. |
| `kiro_cli_mode_active_total` | Counted an agent mode at chat-session creation, duplicating the mode and engine already present on the chat-session counter. | None | Remove. Use `kiro_cli_chat_session_started_total{agent_mode=...}` for sessions and `kiro_cli_user_turns{agent_mode=...}` for actual mode activity. |
| `kiro_cli_daily_heartbeat` | How many active installation-version pairs ran during a UTC day? | `version_full`, `release_channel`, `os_type`, `install_method` | Keep as the base adoption counter and make the local daily guard version-aware. |
| `client_version_seen` | Previously intended as a batch-computed active-user gauge | None | Remove. There is no nightly batch producer; derive version counts from heartbeats. |
| `version_adoption_pct` | What share of active installations used each version? | None | Remove as an emitted gauge. Derive it from heartbeat sums with metric math. |
| `kiro_cli_session_outcome_total` | Previously reported terminal states from explicit `/goal` workflows under an ambiguous session name. | None | Replace with `kiro_cli_goal_outcome_total`. |
| `kiro_cli_goal_outcome_total` | Of explicit `/goal` workflows that reach a terminal state, how do they end? | `version_full`, `agent_engine`, `goal_outcome` | Add as a terminal-outcome counter. Do not interpret it as a completion rate because goals abandoned by process exit are not observed. |

Creation-to-first-prompt drop-off is useful, but it is a funnel question better suited to Amplitude than
an additional CloudWatch metric.

## Session attribution requirements

- Interactive Kiro TUI/Lite sessions map to `interactive_cli`.
- `--no-interactive` sessions map to `noninteractive_cli`.
- Applications directly embedding the ACP agent map to `external_acp`.
- Attribution must use a trusted launcher marker, not an arbitrary client-provided name.
- V1 records chat-session creation in the V1 session constructor.
- The interactive TUI owns interactive V2 chat-session and turn records. The V2 host suppresses its
  duplicate copy for that trusted client.
- The V2 host owns chat-session and turn records for external ACP clients.
- Existing pre-migration CloudWatch data cannot be retroactively corrected into these cohorts.

## Active-installation adoption

The adoption denominator is active installations, not unique people. A local installation that runs a
version during a UTC day contributes one heartbeat for that version. If the same installation runs
stable and nightly on the same day, it contributes once to each version; the precise measure is therefore
an active installation-version day.

The heartbeat guard is version-aware, so stable and nightly versions used from the same installation
can each contribute one heartbeat during the UTC day.

CloudWatch should derive:

- Active installations by exact version, release channel, OS, and install method.
- Version adoption percentage: heartbeat sum for one version divided by heartbeat sum for all versions.
- Stable versus nightly adoption.
- Windows versus macOS versus Linux adoption.
- Package-manager versus installation-script adoption.

Unique-person adoption requires backend identity deduplication and is intentionally out of scope for
this metric.

### Obsolete adoption rollups

Remove `active_users_daily`. It has no server-side batch producer and duplicates the daily
`SUM(kiro_cli_daily_heartbeat)` active installation-version count retained in the CloudWatch derived
metric inventory.

Remove `active_users_weekly`. Summing seven daily heartbeat periods produces installation-days and can
count one installation seven times. A true weekly active-installation value requires backend
deduplication by installation identity, and no such batch producer exists.

Remove `active_users_monthly` for the same reason. Summing daily heartbeat periods produces
installation-days rather than distinct monthly active installations. Add a monthly rollup only after
backend installation-level deduplication exists.

Remove `dau_mau_ratio`. DAU can be derived from one UTC day of heartbeats, but MAU cannot currently be
deduplicated across days. Dividing DAU by summed installation-days would produce a misleading
engagement ratio.

Remove `new_users_daily`. There is no trustworthy new-installation producer or backend first-seen
batch job. First observed telemetry can come from an existing installation that upgraded, opted in,
returned after inactivity, or only recently gained heartbeat support. Reintroduce this question only
with a durable installation receipt and backend first-seen deduplication.

Remove `stale_version_users`. It has no producer, and clients do not own an authoritative release
calendar for assigning staleness buckets. Exact-version heartbeat counts expose the active long tail.
A backend may later join those counts with maintained release metadata if a stale-version rollup
becomes necessary.

Remove `tool_using_sessions_pct`. Tool-call counts cannot identify how many distinct sessions had at
least one tool call; dividing tool calls by session starts measures calls per session, not the
percentage of tool-using sessions. Use a supported session-deduplicated analytics sink or Amplitude if
this question is needed.

### Installation attribution limitation

The published `curl -fsSL https://cli.kiro.dev/install | bash` script leaves no durable installation
receipt. The current CLI detector can identify Homebrew and internal Toolbox; manual downloads, local
builds, installation-script installs, and detector failures all fall into `unknown`. Existing
`unknown` data must not be relabeled as installation-script usage.

Taskei task
[P477556496](https://taskei.amazon.dev/tasks/d38593ff-9c01-4aa9-9602-353a9a8d0508)
tracks adding an `installation_script` receipt to the installer and consuming it in the CLI.

## Implemented client work

- The schema now contains exactly 42 emitted metrics with separate `attributes` and
  `cloudwatch_dimensions` contracts.
- Typed Rust constructors and TUI observer functions emit only the reviewed dimensions.
- V1, V2, KAS, and TUI producers use bounded `agent_mode`, trusted `session_interface`, and
  normalized `agent_engine` values.
- Session-start ownership is deduplicated and chat sessions count at creation.
- Heartbeats carry exact version, release channel, OS, and install method, and the local guard is
  version-aware.
- Automatic retry depth records once at terminal operation completion with recovered or exhausted
  outcome; a missing typed transport cause maps to `retry_reason=unknown`.
- Retired client metrics and unproduced adoption gauges are removed.
- Crash receipts and telemetry-export-drop receipts are bounded, replayed after recovery, and removed
  after a successful flush.
- `kiro_cli_session_outcome_total` is replaced by `kiro_cli_goal_outcome_total`.
- Local catalog and TUI producer validation covers the final Prometheus series and engine split.

Remaining rollout work:

- Deploy the compatibility KUTS declarations in `CR-291574951` before publishing the client revision.
- Add the installation-script receipt tracked by P477556496; until then, unattributable installs
  remain `unknown`.
- Validate generated EMF, production CloudWatch series counts, and MCP diagnostic queries after each
  rollout stage.

## Catalog reconciliation

The pre-review `metrics.yaml` contained exactly 100 entries. Every historical entry has one
disposition below:

| Disposition | Count | Meaning |
|---|---:|---|
| Keep | 12 | Keep the metric name and core count point, but apply the reviewed contract. |
| Rename | 16 | Preserve the core signal under a clearer canonical name and reviewed contract. |
| Replace | 20 | Remove the pre-review entry and migrate its producer or intent to the named replacement. |
| Derive | 7 | Remove the client-emitted entry and calculate the signal from retained base metrics. |
| Remove | 45 | Remove without a client-emitted metric replacement. |
| **Total** | **100** | Complete coverage of the 2026-07-23 catalog. |

Renames and replacements converge on shared targets, so the reconciled client catalog contains 42
unique emitted instruments rather than 60 retained rows. CloudWatch metric-math expressions remain
dashboard configuration and are not additional client instruments.

### Current emitted catalog

This is the 42-metric catalog implemented by the client change. The dimension column lists CloudWatch
dimensions, not every OTel datapoint attribute.

| Final metric | Kind | CloudWatch dimensions and notes |
|---|---|---|
| `kiro_cli_run_started_total` | Counter | `version_full`, `session_interface`, `agent_engine`, `os_type` |
| `kiro_cli_login_success_total` | Counter | `version_full`, `auth_method`, `auth_flow` |
| `kiro_cli_chat_session_started_total` | Counter | `version_full`, `session_interface`, `agent_mode`, `agent_engine` |
| `kiro_cli_cloud_session_lifecycle_total` | Counter | `version_full`, `cloud_event` |
| `kiro_cli_cloud_session_ready_seconds` | Histogram | `version_full` |
| `kiro_cli_ui_mode_session_started_total` | Counter | `version_full`, `ui_mode` |
| `kiro_cli_daily_heartbeat` | Counter | `version_full`, `release_channel`, `os_type`, `install_method` |
| `kiro_cli_slash_command_invoked_total` | Counter | `version_full`, `agent_engine`, `command` |
| `kiro_cli_top_level_command_invoked_total` | Counter | `version_full`, `command` |
| `kiro_cli_tool_call_total` | Counter | `version_full`, `agent_engine`, `tool_origin`, `tool_outcome`, `execution_context`; add bounded `builtin_tool_name` only for built-in tools |
| `kiro_cli_model_invocations_total` | Counter | `version_full`, `agent_engine`, `model` |
| `kiro_cli_model_time_to_first_content_ms` | Histogram | `version_full`, `agent_engine`, `model` |
| `kiro_cli_time_to_first_visible_response_ms` | Histogram | `version_full`, `session_interface`, `agent_mode`, `agent_engine`; first-party clients only |
| `kiro_cli_model_request_duration_seconds` | Histogram | `version_full`, `agent_engine`, `model`, `model_request_outcome` |
| `kiro_cli_user_turn_duration_seconds` | Histogram | `version_full`, `session_interface`, `agent_mode`, `agent_engine` |
| `kiro_cli_run_outcome_total` | Counter | `version_full`, `session_interface`, `agent_engine`, `os_type`, `run_outcome` |
| `kiro_cli_crash_total` | Counter | `version_full`, `agent_engine`, `os_type`, `process_role`, `crash_kind` |
| `kiro_cli_startup_duration_seconds` | Histogram | `version_full`, `session_interface`, `agent_engine`, `os_type` |
| `kiro_cli_startup_failure_total` | Counter | `version_full`, `session_interface`, `agent_engine`, `os_type`, `startup_failure_stage` |
| `kiro_cli_model_request_failure_total` | Counter | `version_full`, `agent_engine`, `model`, `error_kind` |
| `kiro_cli_automatic_retries_per_operation` | Histogram | `version_full`, `agent_engine`, `retry_reason`, `retry_outcome` |
| `kiro_cli_process_memory_rss_bytes` | Observable gauge | `version_full`, `os_type`, `agent_engine`, `process_role` |
| `kiro_cli_process_cpu_utilization_ratio` | Histogram | `version_full`, `os_type`, `agent_engine`, `process_role` |
| `kiro_cli_process_open_file_descriptor_count` | Observable gauge | `version_full`, `os_type`, `agent_engine`, `process_role`; macOS and Linux only |
| `kiro_cli_process_handle_count` | Observable gauge | `version_full`, `agent_engine`, `process_role`; Windows only |
| `kiro_cli_process_thread_count` | Observable gauge | `version_full`, `os_type`, `agent_engine`, `process_role` |
| `kiro_cli_tokens_consumed` | Counter | `version_full`, `agent_engine`, `model`, `token_type` |
| `kiro_cli_credits_consumed` | Counter | `version_full`, `model` |
| `kiro_cli_tool_execution_duration_ms` | Histogram | `version_full`, `agent_engine`, `tool_origin`, `tool_outcome`, `execution_context`; add bounded `builtin_tool_name` only for built-in tools |
| `kiro_cli_mcp_server_init_total` | Counter | `version_full`, `agent_engine`, `mcp_server_source`, `mcp_init_outcome`; keep `mcp_server_name`, `mcp_error_kind`, and `mcp_failure_stage` as non-dimension EMF fields |
| `kiro_cli_user_turns` | Counter | `version_full`, `session_interface`, `agent_mode`, `agent_engine` |
| `kiro_cli_goal_outcome_total` | Counter | `version_full`, `agent_engine`, `goal_outcome` |
| `kiro_cli_prohibited_telemetry_channel_enabled_total` | Counter | `version_full`, `telemetry_channel` |
| `kiro_cli_telemetry_export_dropped_total` | Counter | `version_full`, `drop_reason` |
| `kiro_cli_turn_failure_total` | Counter | `version_full`, `session_interface`, `agent_mode`, `agent_engine`, `turn_failure_reason` |
| `kiro_cli_turn_cancelled_total` | Counter | `version_full`, `session_interface`, `agent_mode`, `agent_engine` |
| `kiro_cli_process_peak_rss_bytes` | Histogram | `version_full`, `os_type`, `agent_engine`, `process_role` |
| `kiro_cli_tui_heap_used_bytes` | Observable gauge | `version_full`, `os_type`, `agent_engine` |
| `kiro_cli_tui_event_loop_delay_p99_seconds` | Histogram | `version_full`, `os_type`, `agent_engine` |
| `kiro_cli_tui_input_to_render_p95_seconds` | Histogram | `version_full`, `os_type`, `agent_engine` |
| `kiro_cli_tui_render_duration_seconds` | Histogram | `version_full`, `os_type`, `agent_engine`, `render_kind` |
| `kiro_cli_auth_failure_total` | Counter | `version_full`, `auth_method`, `auth_flow`, `auth_operation`, `auth_failure_reason` |

### Current-to-final ledger

Rows follow the pre-review `metrics.yaml` order. A target in braces identifies the bounded subset used
instead of a separate metric.

| # | Current catalog entry | Disposition | Final target or source |
|---:|---|---|---|
| 1 | `kiro_cli_session_started_total` | Replace | `kiro_cli_run_started_total` |
| 2 | `kiro_cli_user_logged_in_total` | Rename | `kiro_cli_login_success_total` |
| 3 | `kiro_cli_chat_session_started_total` | Keep | Same name with reviewed session dimensions |
| 4 | `kiro_cli_cloud_session_total` | Rename | `kiro_cli_cloud_session_lifecycle_total` |
| 5 | `kiro_cli_cloud_session_ready_seconds` | Keep | Same name with `version_full` only |
| 6 | `kiro_cli_cloud_repo_attach_total` | Remove | Use Amplitude for the repository-picker funnel |
| 7 | `kiro_cli_ui_mode_session_started_total` | Keep | Same name with `version_full`, `ui_mode` |
| 8 | `kiro_cli_ui_mode_changed_total` | Remove | Use Amplitude for mode-switch funnels |
| 9 | `kiro_cli_ui_mode_default_changed_total` | Remove | Use actual launch mode and Amplitude |
| 10 | `active_users_daily` | Derive | Daily `SUM(kiro_cli_daily_heartbeat)` |
| 11 | `active_users_weekly` | Remove | Requires backend installation deduplication |
| 12 | `active_users_monthly` | Remove | Requires backend installation deduplication |
| 13 | `dau_mau_ratio` | Remove | MAU denominator is unavailable |
| 14 | `new_users_daily` | Remove | Requires backend first-seen deduplication |
| 15 | `client_version_seen` | Derive | Version counts from `kiro_cli_daily_heartbeat` |
| 16 | `version_adoption_pct` | Derive | Version heartbeat share in CloudWatch |
| 17 | `stale_version_users` | Remove | Join heartbeat versions to release metadata downstream if needed |
| 18 | `kiro_cli_upgrade_completed_total` | Remove | Use version-aware heartbeat adoption |
| 19 | `kiro_cli_client_identity` | Remove | No approved identity-aware analytics sink |
| 20 | `kiro_cli_daily_heartbeat` | Keep | Same name with reviewed adoption dimensions |
| 21 | `kiro_cli_slash_command_invoked_total` | Keep | Same name with bounded canonical commands |
| 22 | `kiro_cli_feature_used_total` | Replace | `kiro_cli_top_level_command_invoked_total` |
| 23 | `feature_unique_users_weekly` | Remove | Requires downstream identity-aware aggregation |
| 24 | `kiro_cli_tool_call_total` | Keep | Canonical tool-attempt counter |
| 25 | `tool_using_sessions_pct` | Remove | Tool counts cannot produce a session-deduplicated percentage |
| 26 | `kiro_cli_mcp_server_connected_total` | Derive | `kiro_cli_mcp_server_init_total{mcp_init_outcome=success|degraded}` |
| 27 | `kiro_cli_model_invocations_total` | Keep | Same name at the logical model-request boundary |
| 28 | `mode_active_users_weekly` | Remove | Requires downstream identity-aware aggregation |
| 29 | `kiro_cli_feature_first_use` | Remove | Use Amplitude or a future analytics sink |
| 30 | `kiro_cli.bedrock.stream.ttft` | Replace | `kiro_cli_model_time_to_first_content_ms` and `kiro_cli_time_to_first_visible_response_ms` |
| 31 | `kiro_cli.bedrock.stream.duration` | Replace | `kiro_cli_model_request_duration_seconds` |
| 32 | `kiro_cli.bedrock.request.duration` | Replace | `kiro_cli_model_request_duration_seconds` |
| 33 | `kiro_cli.bedrock.stream.inter_token_latency` | Remove | No approved actionable signal |
| 34 | `kiro_cli.startup.duration` | Rename | `kiro_cli_startup_duration_seconds` |
| 35 | `kiro_cli.agent.loop.iteration_duration` | Remove | Covered by request, tool, render, and turn latency |
| 36 | `kiro_cli_user_turn_duration_seconds` | Keep | Same name at the successful top-level turn boundary |
| 37 | `kiro_cli_time_to_first_chunk_ms` | Replace | `kiro_cli_model_time_to_first_content_ms` and `kiro_cli_time_to_first_visible_response_ms` |
| 38 | `kiro_cli.session.completed` | Replace | `kiro_cli_run_outcome_total` |
| 39 | `kiro_cli.crash.total` | Rename | `kiro_cli_crash_total` |
| 40 | `kiro_cli.startup.failures` | Rename | `kiro_cli_startup_failure_total` |
| 41 | `kiro_cli.bedrock.request.errors` | Replace | `kiro_cli_model_request_failure_total` |
| 42 | `kiro_cli.bedrock.empty_response.retries` | Replace | `kiro_cli_automatic_retries_per_operation{retry_reason=empty_response}` |
| 43 | `kiro_cli.retry.attempts` | Replace | `kiro_cli_automatic_retries_per_operation` |
| 44 | `kiro_cli.retry.exhausted` | Replace | `kiro_cli_automatic_retries_per_operation{retry_outcome=exhausted}` |
| 45 | `kiro_cli.agent.loop.stuck` | Remove | Use typed timeout failures and structured diagnostics |
| 46 | `kiro_cli.upstream.dependency.up` | Remove | Use service canaries and client failure incidence |
| 47 | `kiro_cli.slo.success_rate` | Derive | Explicit login success and turn failure-incidence expressions |
| 48 | `kiro_cli.slo.availability` | Derive | Separate turn, startup, and login availability expressions |
| 49 | `kiro_cli.process.memory.rss` | Rename | `kiro_cli_process_memory_rss_bytes` |
| 50 | `kiro_cli.process.memory.growth_rate` | Remove | Use sustained and peak RSS |
| 51 | `kiro_cli.process.cpu.utilization` | Rename | `kiro_cli_process_cpu_utilization_ratio` |
| 52 | `kiro_cli.process.fds.open` | Rename | `kiro_cli_process_open_file_descriptor_count`; add Windows `kiro_cli_process_handle_count` |
| 53 | `kiro_cli.process.threads` | Rename | `kiro_cli_process_thread_count` |
| 54 | `kiro_cli_tokens_consumed` | Keep | Same name with reviewed token dimensions |
| 55 | `kiro_cli_cache_hit_ratio` | Derive | Token-weighted cache-hit percentage from `kiro_cli_tokens_consumed` |
| 56 | `kiro_cli_context_usage_percentage` | Remove | No approved dashboard decision |
| 57 | `kiro_cli_metering_event` | Replace | `kiro_cli_credits_consumed` |
| 58 | `kiro_cli_user_turn_completed` | Remove | Aggregate questions are covered by retained turn metrics |
| 59 | `kiro_cli_tool_invoked` | Remove | Aggregate questions are covered by retained tool metrics |
| 60 | `kiro_cli_tool_invocations` | Replace | `kiro_cli_tool_call_total` |
| 61 | `kiro_cli_tool_execution_duration_ms` | Keep | Same name with canonical tool dimensions |
| 62 | `kiro_cli_mcp_server_init` | Remove | Carry approved raw context as non-dimension fields on `kiro_cli_mcp_server_init_total` |
| 63 | `kiro_cli_mcp_server_init_total` | Keep | Same name with source, outcome, and explicit EMF dimensions |
| 64 | `kiro_cli_subagent_invoked` | Remove | Count delegation through `kiro_cli_tool_call_total` |
| 65 | `kiro_cli_user_turns` | Keep | Same name at terminal top-level turn completion |
| 66 | `kiro_cli_session_outcome_total` | Replace | `kiro_cli_goal_outcome_total` |
| 67 | `kiro_cli_user_feedback_total` | Remove | Analyze feedback in its owning service or Amplitude |
| 68 | `kiro_cli_message_regenerated_total` | Remove | No feature or production producer exists |
| 69 | `kiro_cli_conversation_completed` | Remove | Not a trustworthy session-completion signal |
| 70 | `kiro_cli_telemetry_opt_out_respected_total` | Remove | Opt-out must emit nothing |
| 71 | `kiro_cli_telemetry_opt_out_violation_total` | Remove | Enforce locally without compounding a violation |
| 72 | `kiro_cli_pii_redaction_runs_total` | Remove | Redaction is infallible; enforce outbound-field coverage in tests |
| 73 | `kiro_cli_pii_redaction_matches_total` | Remove | Match counts do not prove complete outbound coverage |
| 74 | `kiro_cli_pii_redaction_errors_total` | Remove | No reachable redaction-error state exists |
| 75 | `kiro_cli_pii_redaction_coverage_ratio` | Remove | Enforce schema-aware coverage in tests |
| 76 | `kiro_cli_govcloud_channel_disabled_total` | Replace | `kiro_cli_prohibited_telemetry_channel_enabled_total` |
| 77 | `kiro_cli_govcloud_channel_leak_total` | Replace | `kiro_cli_prohibited_telemetry_channel_enabled_total` |
| 78 | `kiro_cli_kuts_export_oversize_total` | Replace | `kiro_cli_telemetry_export_dropped_total{drop_reason=oversize}` |
| 79 | `kiro_cli_consent_record_integrity_total` | Remove | Validate settings locally and fail closed |
| 80 | `kiro_cli_auth_credential_failure_total` | Rename | `kiro_cli_auth_failure_total` |
| 81 | `kiro_cli_auth_unexpected_identity_total` | Replace | `kiro_cli_auth_failure_total{failure_reason=identity_mismatch}` |
| 82 | `kiro_cli_tls_validation_failure_total` | Remove | No typed interception point exists across the client networking stacks |
| 83 | `kiro_cli_tool_egress_destinations_total` | Remove | Observe egress at an enforcement boundary |
| 84 | `kiro_cli.telemetry.exporter.send.attempts` | Remove | Measure request health at KUTS |
| 85 | `kiro_cli.telemetry.exporter.send.duration` | Remove | Measure request latency at KUTS |
| 86 | `kiro_cli.telemetry.exporter.dropped` | Rename | `kiro_cli_telemetry_export_dropped_total` |
| 87 | `kiro_cli.telemetry.queue.depth` | Remove | Keep local diagnostics; use permanent drops for fleet impact |
| 88 | `kiro_cli.telemetry.batch.size` | Remove | Measure incoming request bytes at KUTS |
| 89 | `kiro_cli.telemetry.emit.failures` | Remove | Use validation tests, local diagnostics, and permanent drop accounting |
| 90 | `kiro_cli.telemetry.sdk.up` | Remove | Use heartbeat receipt and KUTS service health |
| 91 | `kiro_cli.telemetry.flush_on_exit.dropped_total` | Remove | No producer can count records lost during a shutdown timeout reliably |
| 92 | `kiro_cli.meta_meter.up` | Remove | It is not independent of the OTel exporter |
| 93 | `kiro_cli_turn_outcome_total` | Replace | `kiro_cli_turn_failure_total` plus `kiro_cli_turn_cancelled_total` |
| 94 | `kiro_cli_subagent_delegations_total` | Remove | Use `kiro_cli_tool_call_total{tool_origin=builtin,builtin_tool_name=use_subagent}` |
| 95 | `kiro_cli_mode_active_total` | Remove | Use session and turn metrics by `agent_mode` |
| 96 | `kiro_cli.process.memory.peak_rss` | Rename | `kiro_cli_process_peak_rss_bytes` and change gauge to histogram |
| 97 | `kiro_cli.process.memory.heap_used` | Rename | `kiro_cli_tui_heap_used_bytes` |
| 98 | `kiro_cli.tui.event_loop.delay` | Rename | `kiro_cli_tui_event_loop_delay_p99_seconds` |
| 99 | `kiro_cli.tui.input.latency` | Rename | `kiro_cli_tui_input_to_render_p95_seconds` |
| 100 | `kiro_cli.tui.render.duration` | Rename | `kiro_cli_tui_render_duration_seconds` |

### Implemented reconciliation rules

No historical catalog entry lacks a disposition, and no two final emitted metrics have conflicting
kinds. The client implementation applies these rules:

- The reviewed `agent_mode=default|plan|spec|autonomous|custom` mapping is consistent across V1, V2,
  and KAS. Built-in identifiers normalize to their matching bucket, and all other agent profile IDs
  become `custom`.
- `agent_engine` is bounded to `v1|v2|v3|unknown`. KAS normalizes to `v3`; `kas` is not exposed as a
  separate value.
- Schema datapoint attributes are separate from CloudWatch dimensions. In particular,
  `kiro_cli_mcp_server_init_total` needs three non-dimension EMF fields while every other final metric
  exports only its declared dimension keys.
- The schema defines the ten dimension or diagnostic attributes introduced by the review:
  `session_interface`, `auth_method`, `auth_flow`, `agent_mode`, `execution_context`, `run_outcome`,
  `mcp_server_source`, `goal_outcome`, `telemetry_channel`, and `mcp_error_kind`. Retained attributes
  such as `agent_engine`, `os_type`, `install_method`, metric-specific outcomes, stages, and failure
  reasons use their reviewed vocabularies.
- Legacy Toolkit and CodeWhisperer events stay in place until their independent channel cutover is
  approved. This ledger governs the KUTS OTel catalog only.

KUTS must mechanically validate that every `awsemf/kirocli.metric_declarations` entry matches the
schema. Once explicit declarations are enabled, an undeclared metric is dropped rather than gaining an
implicit dimension set.

KUTS declarations require a compatibility rollout:

1. First deploy the declarations in `CR-291574951` for all 89 pre-migration metric instruments plus
   all 42 final names. Use only the safe intersection of historical attributes and reviewed dimensions for old names, with a
   dimensionless fallback where necessary. This stops new cardinality growth without dropping metrics
   from clients that have not upgraded. The eight `log_event` and three `derived` catalog entries do
   not receive EMF metric declarations.
2. Then deploy the client schema and producer migration. Validate that all 42 final names arrive with
   only their declared dimensions and that MCP diagnostic fields remain queryable in EMF.
3. After the supported client-adoption window, remove declarations for retired names. Old CloudWatch
   series stop receiving data immediately after step 1 but may remain discoverable until their prior
   samples age out of the dashboard query window.

## Review method

Each metric or metric family was reviewed using the same questions:

1. The exact question it answers.
2. When and how often it records.
3. The minimum dimensions needed to answer that question.
4. Whether raw detail belongs in CloudWatch metrics, raw EMF, local or legacy telemetry, or Amplitude.
5. Whether to keep, rename, consolidate, or remove it.

Completed review order:

1. Tool and MCP usage, reliability, and latency.
2. Turn outcome, model, token, cache, context, and latency metrics.
3. Process exit, goal outcome, crash, startup, retry, and reliability metrics.
4. UI mode, slash command, and feature-usage metrics.
5. Process and TUI performance metrics.
6. Authentication, privacy, security, and telemetry-pipeline health metrics.
7. Dormant instruments and derived metrics: wire, derive, or delete.

## Tool and MCP metrics

Review these together because they overlap:

- `kiro_cli_tool_call_total`
- `kiro_cli_tool_invocations`
- `kiro_cli_tool_execution_duration_ms`
- `kiro_cli_mcp_server_connected_total`
- `kiro_cli_mcp_server_init_total`

Reviewed questions:

- How often are built-in and MCP tools invoked, and with what outcomes?
- How long do tool calls take, and which bounded tool category is slow or unreliable?
- Which named MCP servers are connected or used, and how reliably do they initialize?

Confirmed dimensions for high-frequency tool-call metrics:

`version_full`, `agent_engine`, `tool_origin`, `tool_outcome`, and `execution_context`, with a bounded
`builtin_tool_name` only for built-in tools. `execution_context` has exactly two values: `main` and
`subagent`. Do not put raw custom tool names, subagent names, or subagent IDs on these metrics. Do not
include `session_interface` or `agent_mode`.

`tool_origin` identifies the subsystem that supplied the tool, not the session interface or exact tool
name:

| Value | Meaning | Example |
|---|---|---|
| `builtin` | A Kiro-provided local tool | `fs_read`, `execute_bash` |
| `mcp` | A tool exposed by an MCP server | A GitHub or database MCP tool |
| `unknown` | The producer could not establish the tool source | Version-skewed or malformed metadata |

The pre-review schema also allowed `custom`, `aws_api`, and `_other_`. Pre-review producers did not use
`custom` consistently: Rust classified legacy custom tools as MCP. `aws_api` was not a distinct
provider architecture: it identified the
transitional `use_aws` built-in, which still exists in V1 and V2, invokes the local `aws` CLI process,
and is deprecated when agent configurations migrate to KAS/V3. The implemented catalog retains
`builtin`, `mcp`, and `unknown`; classify `use_aws` as `builtin` with
`builtin_tool_name=use_aws`.

`builtin_tool_name` uses the explicit schema allowlist. Cross-engine aliases normalize to one identity;
for example, `read` and `fsRead` become `fs_read`, `shell` and `executeCmd` become `execute_bash`, and
`subagent` and `agent_crew` become `use_subagent`. A built-in name outside the allowlist becomes
`unknown` rather than creating a new series.

### Subagent tool attribution

The parent's delegation call is an ordinary built-in tool execution:
`tool_origin=builtin,builtin_tool_name=use_subagent,execution_context=main`. Nested calls retain their
actual origin (`builtin` or `mcp`) and use `execution_context=subagent`.

Pre-review producer behavior was inconsistent:

| Engine path | Parent delegation | Tools executed inside the subagent |
|---|---|---|
| V1 | Counted as the ordinary `use_subagent` tool call. | Not emitted as individual KUTS tool-call metrics. A separate legacy subagent event carried aggregate built-in/MCP counts, but the pre-review OTel translation did not turn those counts into tool-call series. |
| V2 TUI | Counted when the parent event carries the expected pipeline metadata. | Explicitly excluded from TUI tool-call metrics. |
| V2 Rust host | Counted by the host observer. | Emitted by each subagent's host observer, but the tool metric has no subagent attribute, so nested and main calls are indistinguishable. |
| KAS/V3 TUI | Counted as the parent orchestration tool. | Explicitly excluded after the event is routed to its subtask. |

The implementation gives the Rust host ownership of V2 tool metrics, so the TUI no longer duplicates
V2 tool calls. The KAS TUI path records both main and routed subagent tool events with the appropriate
execution context.

The implemented contract replaces this engine-dependent behavior:

- Count all actual tool executions.
- Record the parent's delegation call as
  `tool_origin=builtin,builtin_tool_name=use_subagent,execution_context=main`.
- Record each nested tool with its real `tool_origin` and `execution_context=subagent`.
- Do not record subagent identity as a metric dimension.

Adding `execution_context` increases the theoretical tool-series cardinality by at most 2x: any existing
combination can have a `main` and a `subagent` series. Realized growth should be lower because some
combinations occur in only one context; for example, the parent delegation tool is a main-context call.
Exact `version_full` values remain the larger ongoing multiplier because every active version repeats
the bounded tool combinations.

### Tool metric consolidation

- Keep `kiro_cli_tool_call_total` as the canonical tool-attempt counter and remove
  `kiro_cli_tool_invocations`. Producers that emitted only the latter moved to the canonical counter.
- Remove `kiro_cli_subagent_delegations_total`. Count parent delegation attempts through
  `kiro_cli_tool_call_total{tool_origin=builtin,builtin_tool_name=use_subagent,execution_context=main}`
  and use its `tool_outcome`
  dimension for success and failure rates. The dedicated metric fires at the same parent tool-call
  completion point, its `subagent_name_class` accepts arbitrary normalized names, and its `model` does
  not reliably identify the child model. If actual child-process starts later become a distinct
  question, instrument a bounded `kiro_cli_subagent_started_total` at the child-spawn boundary rather
  than inferring it from tool completion.
- Keep `kiro_cli_tool_execution_duration_ms` for measured execution latency.
- Keep `kiro_cli_mcp_server_init_total` and remove `kiro_cli_mcp_server_connected_total`. A connected
  count is the subset of initialization attempts whose handshake completed and can be queried by
  summing `kiro_cli_mcp_server_init_total` where `mcp_init_outcome` is `success` or `degraded`.

### MCP initialization dimensions

Before the review, `kiro_cli_mcp_server_init_total` had:

- `mcp_server_class`: `builtin_fs`, `builtin_code`, `builtin_knowledge`,
  `official_third_party`, `user_defined`, `internal_amazon`, or `_other_`.
- `outcome`: for this metric, `success`, `timeout`, `auth`, `protocol`, or `other`.

The pre-review server-class mapping lowercased the configured server name and applied these rules in
order:

| Class | Pre-review rule |
|---|---|
| `builtin_fs` | Exact name `fs`, `filesystem`, `file-system`, or `builtin_fs` |
| `builtin_code` | Exact name `code`, `code-agent`, or `builtin_code` |
| `builtin_knowledge` | Exact name `knowledge` or `builtin_knowledge` |
| `official_third_party` | Name starts with `awslabs` or `aws-` |
| `internal_amazon` | Name starts with `amzn-` or `amazon-`, or contains `internal` anywhere |
| `_other_` | Empty name |
| `user_defined` | Every other non-empty name |

This is a name heuristic, not trusted provenance. The name is the user- or registry-selected key from
`mcpServers`; the classifier does not inspect package identity, URL, publisher, registry metadata, or
configuration source. A user can therefore choose a name that enters any bucket, and ordinary names
such as `github`, `postgres`, or `playwright` all become `user_defined`. The
`official_third_party` label was also misleading because its pre-review rule recognized AWS-prefixed
names, not third-party provenance.

The pre-review outcome mapping lowercased the rendered initialization error text and applied these
rules in order:

| Outcome | Pre-review rule |
|---|---|
| `success` | No initialization failure string was supplied |
| `timeout` | Error contains `timeout` or `timed out` |
| `auth` | Error contains `auth`, `unauthorized`, or `forbidden` |
| `protocol` | Error contains `protocol`, `jsonrpc`, or `initialize` |
| `other` | Every other failure string |

This was also heuristic. Ordering meant an authentication timeout became `timeout`, while a generic
message such as `failed to initialize: command not found` could become `protocol`. The producer
rendered a structured launch error to a string before classification. In the V2 agent path, failures
listing tools or prompts after the MCP handshake are logged but do not fail initialization, so
The pre-review `outcome=success` meant the service launched and completed its handshake, not necessarily that every
advertised capability was fetched successfully.

The raw `mcp_server_name` is not a CloudWatch dimension. KUTS does not accept OTLP logs, so it
cannot be delivered through a separate `kiro_cli_mcp_server_init` fact log. Carry it as diagnostic
context on the `kiro_cli_mcp_server_init_total` metric datapoint and explicitly omit it from that
metric's EMF dimension declaration. The ADOT `awsemf` exporter retains all datapoint labels as fields
in the EMF JSON even when `metric_declarations` excludes them from the CloudWatch dimension set.

Replace `mcp_server_class` with `mcp_server_source`. Source must be assigned from trusted configuration
provenance and carried through initialization; it must never be inferred from the configured server
name.

| Source | Determination |
|---|---|
| `registry` | The server started as an explicit registry placeholder or was materialized from an MCP registry reference. Assign this before registry resolution converts the entry to a local or remote configuration. |
| `global` | A direct local or remote entry from the user's global `mcp.json`. |
| `workspace` | A direct local or remote entry from the workspace `mcp.json`. |
| `agent` | A server declared by the selected agent configuration. |
| `acp_injected` | The server came from the ACP client's session creation request. |
| `unknown` | Provenance was absent on a legacy or unexpected path. |

Do not add a `kiro_managed` value yet. Built-in filesystem, code, and knowledge tools are not MCP
servers. Add that value only if Kiro later introduces an explicit first-party MCP server injection
path.

When sources collide, the winning configuration carries its own source. For example, an ACP-injected
server that overrides a configured server remains `acp_injected`, and a registry reference remains
`registry` after it is resolved to a launchable local or remote configuration.

The dimension set is `version_full`, `agent_engine`, `mcp_server_source`, and `mcp_init_outcome`.
`execution_context` is explicitly omitted. This answers whether a CLI release or agent engine
introduced an initialization regression and whether reliability differs by trusted server source.

Replace the pre-review error-string-derived outcome with three structured metric values:

| Outcome | Determination |
|---|---|
| `success` | The MCP handshake completed and every advertised tools/prompts capability that Kiro attempted to discover was listed successfully. |
| `degraded` | The MCP handshake completed, but at least one advertised tools/prompts capability failed discovery. |
| `failure` | The MCP handshake did not complete. |

Detailed causes do not belong in the CloudWatch dimension set. Carry two structured, non-dimension EMF
fields when a typed cause is available:

- `mcp_failure_stage`: `configuration`, `process_launch`, `connection`, `handshake`,
  `capability_discovery`, or `unknown`.
- `mcp_error_kind`: `timeout`, `authentication`, `configuration`, `process_launch`, `connection`,
  `protocol`, or `unknown`.

Select both fields from typed errors at the failure source rather than rendered error text. Do not
attach the rendered error message. Successful initializations do not need either field.

Do not add an MCP server identity dimension. The MCP ecosystem and registry contents change too
quickly for a maintained identity allowlist to remain useful. Use the metric for aggregate
availability:

- Initialization attempts: all `kiro_cli_mcp_server_init_total` records.
- Fully available: `mcp_init_outcome=success`.
- Connected but degraded: `mcp_init_outcome=degraded`.
- Unavailable: `mcp_init_outcome=failure`.
- Overall MCP call volume: `kiro_cli_tool_call_total{tool_origin=mcp}` without server identity.

Use the EMF records produced from `kiro_cli_mcp_server_init_total` for server-specific investigation.
Keep raw `mcp_server_name`, bounded `mcp_error_kind`, and bounded `mcp_failure_stage` as datapoint
attributes but declare only `version_full`, `agent_engine`, `mcp_server_source`, and `mcp_init_outcome`
as CloudWatch dimensions. This lets CloudWatch Logs Insights identify which servers are failing
without creating one CloudWatch metric series per server.

These diagnostic attributes still create distinct OTel attribute sets before export. That cost is
local to one CLI process rather than global across all installations, and initialization records are
bounded in normal use by the servers configured on that installation. The Rust OTel SDK currently
overflows after 2,000 distinct attribute sets per instrument by default. Add a lower explicit
per-instrument limit during implementation only if load testing shows the normal server and retry
combinations fit beneath it without hiding useful failures.

## Turn and model count points

`kiro_cli_user_turns` and `kiro_cli_model_invocations_total` measure different levels of work:

- A user turn records once when one user prompt reaches a terminal success, failure, or cancellation.
- A model invocation records once for each logical request from the agent loop to a model. A turn that
  requests two tools and then produces a final answer normally has one user turn and three model
  invocations.
- Retries performed inside one logical model request do not create additional model invocations. Retry
  metrics record those transport attempts separately.

The implementation uses authoritative count points for both levels. V1 and external ACP use their
Rust host for top-level turns, while the interactive TUI owns interactive V2 and V3 turns. Logical
model requests are counted by the host for V1/V2; KAS reports its model-invocation count in terminal
turn metadata. The interactive V2 TUI does not emit a duplicate model-invocation count.

`model` is an evolving controlled dimension, not a client-compiled enum. Preserve the canonical model
ID supplied by the trusted Kiro engine or model service so newly launched models become visible without
a CLI release. Its expected cardinality is operationally limited by the Kiro model catalog, but the
catalog can grow. Monitor distinct model values and series growth at ingestion instead of mapping
unrecognized canonical IDs to `unknown`. Arbitrary user-provided model labels are not canonical model
IDs and must not become metric dimension values.

### Token accounting

`kiro_cli_tokens_consumed` is an additive counter. Record usage reported by the model service for each
logical request, including failed requests when the service reports usage. Do not infer missing token
counts. Use these `token_type` values:

- `input_uncached`
- `input_cache_read`
- `output`
- `reasoning`, only when the model service reports it separately

Do not include `input_cache_write` in this counter's mutually summable token types. In the current
producer contract, cache-write tokens are a subset of uncached input tokens, so summing both values
double-counts input usage. Review cache-write volume as a separate cache metric if that question is
needed.

Remove the client-emitted `kiro_cli_cache_hit_ratio` histogram. Its per-request observations produce an
unweighted average in aggregate, giving small and large requests equal influence. Define the cache-hit
percentage in CloudWatch dashboard metric math instead:

```text
100 * SUM(input_cache_read) / (SUM(input_cache_read) + SUM(input_uncached))
```

Both inputs come from `kiro_cli_tokens_consumed` with the same version, engine, model, time range, and
period filters. Guard the expression against a zero denominator. Keep the expression in dashboard or
alarm configuration rather than emitting or storing another metric.

### First-response latency

Replace `kiro_cli_time_to_first_chunk_ms` and `kiro_cli.bedrock.stream.ttft`; they recorded the same
host-side timer with different units and dimensions.

`kiro_cli_model_time_to_first_content_ms` starts immediately before the logical model request and stops
when the Rust host receives the first semantic text, reasoning, or tool-use content. Metadata, metering,
and other non-content stream events do not stop the timer. This signal isolates model/request-path
responsiveness and records once per logical model request that produces content.

`kiro_cli_time_to_first_visible_response_ms` starts when a first-party client accepts a top-level user
prompt and stops when that client visibly renders the first meaningful agent output. A locally rendered
loading indicator does not count. This includes client-to-host transport and rendering overhead, making
it the product responsiveness signal. It can be authoritative for interactive CLI and CLI one-shot
output, but Kiro cannot measure when an external ACP application renders an update.

### Turn failures

Replace `kiro_cli_turn_outcome_total` with the failure-only `kiro_cli_turn_failure_total`. Its
`agent_mode` dimension aligns with `kiro_cli_user_turns`, allowing CloudWatch to derive mode-specific
failure rates. Exact custom-agent identities remain in logs; `agent_mode=custom` only separates custom
traffic from built-in modes.

User cancellations do not count as failures. A recoverable model or tool error also does not count when
the top-level turn eventually succeeds. Select the terminal `turn_failure_reason` from typed errors at
the failure source:

| Failure reason | Typed causes |
|---|---|
| `context_limit` | Context-window overflow |
| `timeout` | Model, tool, or overall turn deadline |
| `model_error` | Upstream failure, throttling, invalid model, refusal, or empty/invalid model response |
| `tool_error` | An unrecovered tool-execution failure that terminates the turn |
| `execution_limit` | Maximum output tokens, turn requests, or iteration budget |
| `internal_error` | Agent, IPC, request-validation, storage, or other client failure |
| `unknown` | A terminal failure with no structured cause |

Do not classify from rendered error text. KAS currently often reports only a generic failed status, so
those records remain `unknown` until its turn-completion contract carries a typed cause.

Add `kiro_cli_turn_cancelled_total` with the same non-reason dimensions as `kiro_cli_user_turns`.
Count it once when the user explicitly cancels a top-level turn before a terminal result. It exists to
exclude cancellations from the client-turn availability denominator; it is not an error counter.

### CLI run outcomes

`kiro_cli_run_outcome_total` records once per top-level CLI invocation when a surviving launcher can
observe the result. Use these values:

- `success`
- `user_interrupt`
- `failure`
- `unknown`

Launcher-observed nonzero command, startup, or child exits are `failure`. This metric does not infer
that every nonzero exit is a crash and cannot observe every fatal termination of its own launcher
process. Dedicated crash detection owns panic and durable unclean-termination accounting.

Keep `kiro_cli_run_started_total` separate from chat-session creation. It records once when a top-level
CLI invocation begins and supplies the denominator for run-outcome and crash rates. Both run metrics
carry `os_type`, allowing platform rates to be compared rather than mistaking a platform's larger
installation population for worse reliability.

### Crash receipts

`kiro_cli_crash_total` cannot depend on a dying process exporting telemetry. Keep one small, private
receipt per active top-level run under a persistent Kiro telemetry-state `run-receipts` directory.
Create the receipt atomically with mode `0600`, hold an advisory lock for the process lifetime, mark it
completed before clean deletion, and let a panic hook or supervising parent attach a typed crash cause
when possible. On a later launch, an unlocked incomplete receipt represents a dead prior run. Emit it
using the original run's version, agent engine, OS, and process role, then delete it.

Receipts contain only telemetry metadata: schema version, internal run ID, PID/start time,
`version_full`, `agent_engine`, `os_type`, `process_role`, and optional `crash_kind`. They never contain
commands, prompts, paths, arguments, user IDs, or metric dimension identifiers. The internal run ID is
not a metric dimension.

Use `panic`, `unclean_exit`, and `unknown` crash kinds. Signals, OOMs, and watchdog exits become
`unclean_exit` unless a durable typed producer is added.

Receipt cleanup is bounded independently of exporter health:

- Maximum individual receipt size: 1 KiB.
- Maximum receipt count: 32.
- Maximum receipt-directory size: 64 KiB.
- Maximum receipt age: 30 days.
- Never prune a currently locked receipt.

One process acquires a directory-level reaper lock, deletes completed/malformed/expired receipts, claims
stale receipts atomically, records their metrics, and performs one bounded OTel force-flush. Delete
successfully flushed receipts. Failed exports may remain for a later attempt only within the limits;
prune the oldest unlocked receipts when any limit is exceeded. The current client `emit()` is only a
local OTel SDK handoff, not a network acknowledgment, so this path needs the provider flush result.
When telemetry is disabled, do not create receipts and delete existing unlocked receipts without
reporting them.

### Startup duration

`kiro_cli_startup_duration_seconds` starts at process entry and stops when the selected interface can
perform useful work:

- `interactive_cli`: the prompt is rendered and accepts input.
- `noninteractive_cli`: initialization is complete and the supplied prompt can be dispatched.
- `external_acp`: ACP initialization is complete and the server can accept a session.

Record successful startups only. Do not use a `cold_start` dimension: one Boolean cannot consistently
describe runtime extraction, local caches, database state, and KAS initialization. Detailed phase
timings belong in structured startup logs when diagnosis is needed.

`kiro_cli_startup_failure_total` records once when a run terminates before reaching its interface-ready
point. It carries the same dimensions as run starts plus one `startup_failure_stage`:

- `runtime_setup`
- `agent_launch`
- `protocol_init`
- `interface_init`
- `unknown`

Record the terminal stage, not every intermediate error. User interruption is a run outcome rather than
a startup failure. Detailed failure causes remain in structured startup logs.

### Automatic retries

`kiro_cli_automatic_retries_per_operation` is a histogram observed once when an automatically retried
operation finishes. Its value is the number of additional attempts initiated automatically by Kiro. It
does not include the original attempt or a user manually trying again, and it is not observed for
operations that required no retry.

Transport retries happen inside one logical model request and therefore do not add a model invocation.
Agent recovery retries issue another logical model request and increment
`kiro_cli_model_invocations_total`. Both types produce one retry histogram observation after the retry
sequence finishes. Normal tool-loop continuations are model invocations but are not retries.

Use bounded `retry_outcome` values:

- `recovered`
- `exhausted`
- `cancelled`

Use typed `retry_reason` values:

- `throttled`
- `timeout`
- `connection`
- `server_error`
- `empty_response`
- `context_recovery`
- `unknown`

Transport retries use their typed retry classifier when it is exposed. `context_recovery` is emitted
only by the actual typed auto-compaction or truncation lifecycle, never inferred from a final context
window error and an HTTP attempt count. Recovered requests use `unknown` when the transport exposes no
typed cause. Do not parse warning or error text. In CloudWatch, `Sum` gives total additional attempts,
`SampleCount` gives operations requiring retries, `Average` gives mean retry depth, and
`retry_outcome` filters
show recovery and exhaustion rates. Percentiles expose unusually deep retry sequences.

### Model request failures

`kiro_cli_model_request_failure_total` counts each failed logical model request, including a failure
that Kiro later recovers from by issuing another request. Internal transport attempts do not each
increment it.

Use bounded `error_kind` values:

- `throttling`
- `context_limit`
- `timeout`
- `connection`
- `server_error`
- `access_denied`
- `invalid_request`
- `model_error`
- `unknown`

Do not use `operation`, `status_class`, or `failure_reason_code` as CloudWatch dimensions. The current
producer always reports `operation=stream`, while status classes overlap with `error_kind`. Preserve
the raw service reason, HTTP status code, and status class in local logs and legacy telemetry.

### Goal outcomes

`kiro_cli_goal_outcome_total` counts an explicit `/goal` workflow once when the goal controller observes
a terminal state. It does not describe ordinary chat sessions.

Use bounded `goal_outcome` values:

- `completed`
- `cancelled`
- `iteration_limit`
- `dispatch_failure`
- `reinjection_failure`
- `agent_error`
- `unknown`

Do not include `session_interface`, `agent_mode`, or `model`. The goal feature is currently specific to
the V2 goal controller, and a goal can span multiple requests or models. Keep exact iteration counts and
duration in local goal diagnostics rather than dimensions.

Closing the CLI while a goal is active does not currently produce a terminal event. Therefore, this
metric answers the distribution of observed terminal outcomes, not the percentage of all started goals
that complete. Do not add a goal-start counter unless the lifecycle is also made durable enough to
observe abandonment.

### Agent loop stuck

Remove `kiro_cli.agent.loop.stuck`. It has no production emitter, and a trustworthy implementation
would require a watchdog with explicit progress events, timeout thresholds, and deduplication. Existing
request, tool, and turn timeout metrics cover the actionable completed-failure cases; retain exact hang
diagnostics in local logs rather than introducing an untrustworthy client metric.

### Upstream dependency availability

Remove `kiro_cli.upstream.dependency.up`. A client-emitted `1/0` gauge cannot distinguish a global
service outage from one user's network or configuration problem, and an outage may also prevent the
client from exporting the zero value. Aggregating the latest observations from many installations does
not produce a meaningful global availability signal.

Measure global dependency availability with service-side canaries and service-owned alarms. Use
`kiro_cli_model_request_failure_total`, `kiro_cli_automatic_retries_per_operation`, and authentication
failure metrics to measure client-observed impact.

### UI mode at launch

`kiro_cli_ui_mode_session_started_total` counts the layout selected for each first-party interactive CLI
launch after environment and persisted-setting resolution. UI mode means the full TUI or Lite layout;
it is separate from agent modes such as interactive, plan, review, or custom.

Use bounded `ui_mode` values:

- `tui`
- `lite`

Do not include `session_interface`, because this metric only applies to Kiro's interactive client. Do
not include `ui_mode_source` or `ui_mode_default` as CloudWatch dimensions. Preserve those details in
local logs or Amplitude when analyzing configuration and adoption funnels.

### UI mode changes

Remove `kiro_cli_ui_mode_changed_total` from CloudWatch metrics. The launch-mode metric already provides
the TUI-versus-Lite usage split. Mid-session changes count toggle events rather than users, so repeated
switching can dominate the result without showing preference, retention, or satisfaction. Capture mode
switches as Amplitude events when a product funnel needs them.

### UI mode default changes

Remove `kiro_cli_ui_mode_default_changed_total` from CloudWatch metrics. It counts preference edits, not
the current number of active installations configured for each default, and repeated edits can distort
the result. Use `kiro_cli_ui_mode_session_started_total` for actual layout usage and Amplitude for the
preference-change funnel.

### Agent mode adoption

Remove `mode_active_users_weekly` from client telemetry. Mapping an individual `ModeChanged` event to a
gauge value of `1` does not compute weekly active users and discards the event's transition details.
Weekly unique-user questions require a downstream identity-aware aggregation.

Also remove `kiro_cli_mode_active_total`. It is emitted at the same point as chat-session creation and
duplicates the initial agent mode and engine. Use `kiro_cli_chat_session_started_total` for sessions by
agent mode and `kiro_cli_user_turns` for actual activity by agent mode. Capture mode-switch funnels in
Amplitude rather than CloudWatch.

### Slash command invocations

`kiro_cli_slash_command_invoked_total` counts each recognized slash-command invocation attempt,
including attempts whose handler reports failure. It answers which command families users invoke and
how frequently; it does not represent unique installations or users.

Use these dimensions:

- `version_full`
- `agent_engine`
- `command`

Keep canonical first-party command names such as `/help`, `/model`, and `/chat`. Collapse dynamic
prompt, skill, and steering names to `/prompt`, `/skill`, and `/steering`, respectively. Collapse any
remaining user-defined command names to `/custom`. Do not include subcommand, result, or raw error
attributes as CloudWatch dimensions. Preserve those details in local logs or Amplitude.

### Top-level CLI command invocations

Replace `kiro_cli_feature_used_total` with `kiro_cli_top_level_command_invoked_total`. The existing
metric's generic name is misleading, and its producer only records `chat`, `login`, `profile`, and
`issue` while omitting other user-facing root commands.

The replacement counts each parsed, user-initiated root command once before command execution. Include
the complete bounded root-command enum and exclude hidden internal, IPC, and test commands.

Use these dimensions:

- `version_full`
- `command`

Do not include `agent_engine`: most root commands do not use an agent engine, and engine-specific chat usage
is already measured by the session and turn metrics. This metric answers which shell-level `kiro-cli`
commands users invoke; it measures invocation attempts rather than successful outcomes or unique
installations.

### Weekly unique feature users

Remove `feature_unique_users_weekly`. An individual client cannot calculate a global weekly unique-user
count, and there is no aggregation job producing this gauge. The generic `feature` vocabulary also no
longer exists after replacing `kiro_cli_feature_used_total` with the top-level command metric.

When needed, calculate unique active installations downstream from identity-bearing event logs. Unique
people require a backend identity-aware aggregation and must not be approximated by a client-emitted
gauge.

### Upgrade completions

Remove `kiro_cli_upgrade_completed_total`. It has no production producer, records only completed
upgrades without an attempt denominator, and cannot observe upgrades managed by Homebrew, Toolbox, the
installation script, or Cargo. Exact source-to-target version pairs would also grow quickly for nightly
builds. Use version-aware daily heartbeats to measure release adoption.

### Cloud session lifecycle

Rename `kiro_cli_cloud_session_total` to `kiro_cli_cloud_session_lifecycle_total`. It counts bounded
cloud-session lifecycle transitions rather than sessions and is separate from the all-session
`kiro_cli_chat_session_started_total`.

Use these `cloud_event` values:

- `created`
- `create_failed`
- `ready`
- `provision_failed`
- `reattached`
- `detached`
- `turned_off`
- `fell_back_local`

Use these dimensions:

- `version_full`
- `cloud_event`

Do not include `agent_engine`, because cloud sessions currently always use KAS/V3. Do not include session
identifiers or detailed failure causes; preserve them in local or service-side logs. The event counts
provide the cloud creation, provisioning, reattachment, and shutdown funnel.

### Cloud session ready latency

Keep `kiro_cli_cloud_session_ready_seconds`. It measures the full user-visible wait from immediately
before sending the cloud `session/new` request until the first live cloud-session status. Record one
histogram observation only when the session becomes ready; provisioning failures remain represented by
`kiro_cli_cloud_session_lifecycle_total`.

Use `version_full` as the only CloudWatch dimension. Do not include `agent_engine`, because it is constant for
the cloud path, or OS, agent mode, repository count, and session identifiers. The producer starts its
timer immediately before `session/new`.

### Cloud repository picker

Remove `kiro_cli_cloud_repo_attach_total` from CloudWatch metrics. It measures `/repo` picker opens and
submissions rather than confirmed repository attachments, excludes repositories selected during
initial cloud-session creation, and can be distorted by repeated picker opens. The current submission
path sends a natural-language clone prompt and cannot observe whether attachment succeeds.

Capture the picker funnel in Amplitude. Keep CloudWatch cloud metrics focused on lifecycle reliability
and provisioning latency.

### User feedback sentiment

Remove `kiro_cli_user_feedback_total`. It has no production producer, and the CLI's `/feedback` command
opens an external feedback URL rather than collecting the metric's positive, negative, or neutral
sentiment values. Aggregate submission counts are selection-biased and lack a useful denominator.
Analyze feedback in the feedback service or Amplitude, where the surrounding product context can be
retained.

### Message regeneration

Remove `kiro_cli_message_regenerated_total`. Neither CLI architecture exposes a regenerate-message
feature or production producer. Raw regeneration counts by model would also lack the request denominator
and interaction context needed to interpret them as model-quality evidence. Analyze any future
regeneration workflow in Amplitude.

### Resident memory

Rename `kiro_cli.process.memory.rss` to `kiro_cli_process_memory_rss_bytes`. Sample resident memory every
60 seconds while a process is active and once more on graceful exit. Ensure that each physical process
has one authoritative sampler.

Use these dimensions:

- `version_full`
- `os_type`
- `agent_engine`
- `process_role`

`process_role` is the standard physical-process discriminator for process-performance metrics. Its
values are `host`, `tui`, and `kas_subprocess`. It is distinct from `agent_engine`, which identifies the
logical agent architecture serving the session. Remove `agent_kind`, which overlaps these two
dimensions and is inconsistently emitted by the current Rust and TUI producers.

The metric answers whether a release, platform, engine, or process role has unusually high sustained
memory usage. Because long sessions contribute more periodic samples, aggregate results are
duration-weighted fleet memory pressure rather than unique-installation measurements.

### Memory growth rate

Remove `kiro_cli.process.memory.growth_rate`. It has no production producer. A derivative of aggregated
RSS mixes unrelated processes starting and stopping, while a client-computed per-process slope depends
heavily on the selected time window and workload phase. Process or session identity cannot be added to
make the aggregation exact because those identifiers are forbidden metric dimensions.

Use sustained RSS and peak RSS to detect release-level memory regressions, then investigate suspected
leaks with local profiling or structured diagnostics.

### CPU utilization

Rename `kiro_cli.process.cpu.utilization` to `kiro_cli_process_cpu_utilization_ratio`. Observe process
CPU utilization over fixed 60-second windows so dashboards can compare averages and upper percentiles.

Use these dimensions:

- `version_full`
- `os_type`
- `agent_engine`
- `process_role`

Remove `agent_kind`, which overlaps `agent_engine` and `process_role`. Also remove `state`: current production
translation always reports `_other_`, and assigning the process state at sample time to the entire
60-second CPU window would be misleading. A future idle-CPU signal would need CPU accounting across
actual state transitions rather than an instantaneous label.

### Open file descriptors and Windows handles

Rename `kiro_cli.process.fds.open` to `kiro_cli_process_open_file_descriptor_count`. On macOS and Linux,
sample each process's current open file-descriptor count every 60 seconds and on graceful exit. This
detects file, socket, pipe, and subprocess descriptor leaks that can cause resource-exhaustion failures.

Use these dimensions:

- `version_full`
- `os_type`
- `agent_engine`
- `process_role`

Remove `agent_kind`. Do not emit this metric on Windows because Windows process handles are not
semantically equivalent to POSIX file descriptors.

Add the Windows-only `kiro_cli_process_handle_count` gauge using `GetProcessHandleCount`, with these
dimensions:

- `version_full`
- `agent_engine`
- `process_role`

Do not include `os_type` on the Windows metric because it is constant. Windows handles cover a broader
set of resources, including files, sockets, pipes, registry keys, events, mutexes, processes, and
threads. Use the two metrics in platform-specific panels and never compare their absolute values across
operating systems.

### Process thread count

Rename `kiro_cli.process.threads` to `kiro_cli_process_thread_count`. Sample each physical process's
thread count every 60 seconds and on graceful exit.

Use these dimensions:

- `version_full`
- `os_type`
- `agent_engine`
- `process_role`

Remove `agent_kind`. This metric detects runaway thread creation and runtime or executor regressions
that may not be clearly attributable from memory and CPU signals alone.

### Agent-loop iteration duration

Remove `kiro_cli.agent.loop.iteration_duration`. It has no production producer, and its proposed phases
overlap the canonical model-request duration, tool-execution duration, TUI render duration, overall turn
duration, and first-visible-response metrics. The remaining `parse` phase has no current dashboard or
alarm decision. Use traces or targeted profiling when phase-level agent-loop diagnosis is needed.

### Peak resident memory

Rename `kiro_cli.process.memory.peak_rss` to `kiro_cli_process_peak_rss_bytes` and change it from a gauge
to a histogram. Emit one process-lifetime high-water-mark observation per physical process during
graceful shutdown.

Use these dimensions:

- `version_full`
- `os_type`
- `agent_engine`
- `process_role`

Periodic RSS measures sustained, duration-weighted fleet memory pressure. Peak RSS instead captures
short memory spikes and gives each completed process one observation. Abruptly terminated processes may
not flush this metric, so it is not a crash-complete signal.

### TUI JavaScript heap

Rename `kiro_cli.process.memory.heap_used` to `kiro_cli_tui_heap_used_bytes`. Sample the Bun/JSC managed
heap every 60 seconds and on graceful exit.

Use these dimensions:

- `version_full`
- `os_type`
- `agent_engine`

Do not include `process_role`, because the metric applies only to the TUI process. This signal separates
JavaScript application-state retention from native runtime, renderer, and library memory represented by
RSS.

### TUI event-loop delay

Rename `kiro_cli.tui.event_loop.delay` to `kiro_cli_tui_event_loop_delay_p99_seconds`. For each fixed
60-second window, calculate the p99 Bun event-loop delay and record that single value as one histogram
observation.

Use these dimensions:

- `version_full`
- `os_type`
- `agent_engine`

Do not include `process_role`, because this metric applies only to the TUI. It detects blocking work even
when the user is not actively typing. Dashboards must interpret it as a distribution of per-window p99
values, not as the raw event-loop-delay distribution.

### TUI input-to-render latency

Rename `kiro_cli.tui.input.latency` to `kiro_cli_tui_input_to_render_p95_seconds`. Measure each completed
keypress from the TUI input callback until the React render completes. For every 60-second window with
samples, calculate the p95 total latency and record that value as one histogram observation.

Use these dimensions:

- `version_full`
- `os_type`
- `agent_engine`

Do not include `process_role`, because this metric applies only to the TUI. The measurement excludes
keyboard hardware, terminal delivery before the TUI callback, and terminal paint after React rendering.
It combines the in-process input handler, state update, and render delay into one local interaction
signal.

### TUI render duration

Rename `kiro_cli.tui.render.duration` to `kiro_cli_tui_render_duration_seconds`. Record every completed
Twinki render into an in-process OTel histogram using that render's actual duration and kind. The SDK
aggregates observations before export rather than sending one network event per frame.

Use these dimensions:

- `version_full`
- `os_type`
- `agent_engine`
- `render_kind`

Bound `render_kind` to `full` or `partial`. Do not include `process_role`, because this metric applies
only to the TUI. Unlike input-to-render latency, this signal also covers streaming and background
renders.

### Successful logins

Rename `kiro_cli_user_logged_in_total` to `kiro_cli_login_success_total`. Count once after an explicit
interactive login flow stores usable credentials. Do not count background credential refreshes or reuse
of credentials that were already present.

Use these dimensions:

- `version_full`
- `auth_method`
- `auth_flow`

Bound `auth_method` to `builder_id`, `identity_center`, `social`, `external_idp`, or `unknown`. Bound
`auth_flow` to `pkce`, `device_code`, or `unknown`. Remove `client_application`, because login happens
outside an agent session and does not identify an interface or engine. Carry the selected typed login
method and actual completed flow to telemetry rather than inferring credential kind from the SSO start
URL. This metric counts successful login events, not unique users.

### Authentication failures

Rename `kiro_cli_auth_credential_failure_total` to `kiro_cli_auth_failure_total`. Count once when a
login or credential-refresh operation reaches an unrecovered terminal failure. Exclude user
cancellations.

Use these dimensions:

- `version_full`
- `auth_method`
- `auth_flow`
- `auth_operation`
- `auth_failure_reason`

Use the same `auth_method` vocabulary as `kiro_cli_login_success_total`. Bound `auth_flow` to `pkce`,
`device_code`, `not_applicable`, or `unknown`, and `auth_operation` to `login` or `refresh`. Use typed
`auth_failure_reason` values: `authorization_denied`, `invalid_or_expired_credential`, `network`,
`timeout`, `service_error`, `configuration`, `storage`, `identity_mismatch`, or `unknown`.

Remove raw `error_code` from CloudWatch dimensions and preserve it in local logs and legacy telemetry.
Replace
`auth_provider` with the shared `auth_method` name. Omit `partition` until a dashboard or alarm requires
commercial-versus-GovCloud authentication rates. For login operations, failures divided by failures
plus `kiro_cli_login_success_total` gives the login failure rate.

### Unexpected authentication identity

Remove `kiro_cli_auth_unexpected_identity_total`. Reject an authentication operation whose resulting
identity or partition does not match the expected security context, then record it through
`kiro_cli_auth_failure_total` with `failure_reason=identity_mismatch`. Preserve expected and actual
partition plus other identity diagnostics in a structured security log. Alarm on any bounded
`identity_mismatch` authentication failures.

### TLS validation failures

Remove `kiro_cli_tls_validation_failure_total`. Kiro CLI currently uses several networking stacks and
none exposes one typed interception point that can distinguish terminal TLS validation failures from
DNS, connection, proxy, or generic request errors. A partial producer would present incomplete coverage
as a fleet-wide security signal. Continue surfacing request failures through their owning metrics and
local diagnostics. Add a TLS-specific metric only when the networking boundary provides exhaustive,
typed classification and a reachable producer.

### Tool egress destinations

Remove `kiro_cli_tool_egress_destinations_total`. Kiro cannot observe arbitrary network egress performed
by shell commands or external MCP subprocesses, so instrumenting only Kiro-owned HTTP clients would
produce a misleadingly incomplete security signal. `is_allowlisted` is also not meaningful without one
authoritative enforcement point and policy.

Perform reliable egress auditing at a sandbox, network proxy, or policy-enforcement boundary. Preserve
declared destinations from controlled built-in tools in structured security logs when useful.

### Telemetry opt-out enforcement

Remove both `kiro_cli_telemetry_opt_out_respected_total` and
`kiro_cli_telemetry_opt_out_violation_total` from exported telemetry. When opt-out is respected the
client must export nothing, including a metric claiming compliance. Exporting a violation metric after
an opt-out failure would compound the privacy violation.

Enforce opt-out with local contract tests at the SDK and exporter boundary. Use non-exporting local
diagnostics when runtime investigation is necessary.

### PII redaction failures

Remove `kiro_cli_pii_redaction_runs_total`, `kiro_cli_pii_redaction_matches_total`,
`kiro_cli_pii_redaction_errors_total`, and `kiro_cli_pii_redaction_coverage_ratio` without a replacement
metric. The current redactor is infallible, so a failure counter would have no reachable producer.
Per-field run and match counters also cannot prove complete outbound coverage. Enforce schema-aware
coverage at every telemetry serialization boundary and add a failure metric only if redaction gains a
real fallible outcome that drops the unsafe payload.

### Prohibited telemetry channels

Replace `kiro_cli_govcloud_channel_disabled_total` and `kiro_cli_govcloud_channel_leak_total` with one
failure-only counter: `kiro_cli_prohibited_telemetry_channel_enabled_total`.

Count once per process startup when Kiro detects that a telemetry channel forbidden for the active AWS
partition was initialized or enabled. Block that channel before any send. Use these dimensions:

- `version_full`
- `telemetry_channel`

Any value above zero should alarm. Expected GovCloud channel blocking is a tested configuration
invariant and emits no per-event metric. The existing leak metric only detects that a prohibited client
was constructed; it does not prove that data left the process.

### Consent record integrity

Remove `kiro_cli_consent_record_integrity_total`. The current implementation does not validate a
cryptographic hash or signature: its `hash` check only parses the global settings file as JSON, its
permission and ownership checks apply only on Unix, and the Windows implementations always report
success. Missing or unreadable files also produce three redundant records.

Validate telemetry settings at the actual read boundary and fail closed when the file is unsafe or
unreadable. Use non-exporting local diagnostics and contract tests for this privacy invariant rather
than a metric that observes only telemetry-enabled installations and reports a problem after exports
have already been enabled.

### Oversize KUTS exports

Remove the standalone `kiro_cli_kuts_export_oversize_total` metric. Record each OTLP batch rejected
locally for exceeding KUTS's 1 MiB request limit through the general exporter-drop counter with these
dimensions:

- `version_full`
- `drop_reason=oversize`

Also write a bounded local diagnostic and prevent or split oversized batches. The current counter is
recorded through the same OTel metrics exporter whose payload was rejected, so it cannot reliably
report a sustained oversized metrics pipeline without an independent delivery channel.

### Telemetry exporter send attempts

Remove `kiro_cli.telemetry.exporter.send.attempts`. It is currently constructor-only and has no live
producer. KUTS should measure received request success and retries at the service boundary. Persistent
client failures cannot reliably report themselves through the failing exporter, `dropped` is not a send
attempt, and client-side data loss is covered by the general exporter-drop counter.

### Telemetry exporter send duration

Remove `kiro_cli.telemetry.exporter.send.duration`. It is currently constructor-only, and its intended
observation does not define whether duration covers one HTTP attempt, retries and backoff, or the
complete batch lifecycle. KUTS should monitor service-side request latency. Client networking details
belong in bounded local diagnostics and cannot reliably report the slowest or failed exports through
the same unhealthy pipeline.

### Telemetry export drops

Rename `kiro_cli.telemetry.exporter.dropped` to `kiro_cli_telemetry_export_dropped_total`. Count
telemetry records, not attempts or batches, once they are permanently lost before KUTS accepts them.

Use these dimensions:

- `version_full`
- `drop_reason`

Bound `drop_reason` to `oversize`, `invalid_record`, `encoding_failure`, `retry_exhausted`,
`permanent_rejection`, or `unknown`. The current exporter is always KUTS metrics, so constant
`exporter` and `signal` dimensions add no diagnostic value. Do not classify intentional opt-out
suppression as a drop.

Persist only a fixed-size aggregate keyed by the bounded dimension tuple so failures can be reported
after the pipeline recovers. Retain the original producing `version_full`, emit the aggregate on a
later successful run, and delete the receipt after successful emission. Opt-out must delete or ignore
the receipt without exporting it.

### Telemetry queue depth

Remove `kiro_cli.telemetry.queue.depth`. It is currently constructor-only. Metrics and logs use
different buffering models, and thousands of independent client queue depths would collapse into fleet
statistics that are difficult to interpret. The gauge also shares fate with the blocked exporter when
queue pressure matters most. Keep queue state as a bounded local diagnostic and use
`kiro_cli_telemetry_export_dropped_total` to measure actual data loss.

### Telemetry batch size

Remove `kiro_cli.telemetry.batch.size`. It is currently constructor-only, and record count does not
reliably predict encoded request bytes because telemetry records vary greatly in size. KUTS should
measure incoming request size in bytes and alarm when its p95 or p99 approaches the 1 MiB limit. Keep
client batch details in bounded local diagnostics.

### Telemetry emit failures

Remove `kiro_cli.telemetry.emit.failures`. It is currently constructor-only, references client
subsystems such as a limiter and WAL that no longer exist, and overlaps the general export-drop
counter. Represent schema rejections such as unknown metrics or attributes with
`drop_reason=invalid_record`. Preserve exact validation errors in bounded local diagnostics and
contract tests; PII redaction failures remain under their dedicated metric.

### Telemetry SDK up

Remove `kiro_cli.telemetry.sdk.up`. It is currently constructor-only. A gauge that always reports `1`
proves only that some telemetry arrived and cannot measure SDK coverage among clients that failed to
report; its CloudWatch average would remain `1`. The active-installation heartbeat already proves
successful client telemetry by version, release channel, OS, and install method. Monitor KUTS service
health at the service boundary.

### Flush-on-exit drops

Remove `kiro_cli.telemetry.flush_on_exit.dropped_total`. A timed-out process cannot reliably know how
many SDK-buffered records were lost, so it has no trustworthy producer. Preserve whether shutdown was
clean, signal-driven, or panic-driven in bounded local diagnostics.

### Meta-meter up

Remove `kiro_cli.meta_meter.up`. The original design described an independent direct CloudWatch
`PutMetricData` channel, but the current constructor records `1` through the same global OTel meter and
has no live producer. It therefore cannot detect the OTLP/KUTS outage it claims to cover. Do not add a
second client authentication and export path solely for this gauge; use the active-installation
heartbeat, KUTS service health, and persisted export-drop accounting.

### Derived success rate

Remove the generic derived `kiro_cli.slo.success_rate` metric and its `slo_target` dimension. The
dimension only labels `turn`, `session`, or `login`; it does not provide a common success definition,
and those workflows do not share compatible numerators and denominators.

Keep a login-success-rate expression directly in CloudWatch:

```text
SUM(kiro_cli_login_success_total)
/
(SUM(kiro_cli_login_success_total)
 + SUM(kiro_cli_auth_failure_total{auth_operation=login}))
```

Use matching `version_full`, `auth_method`, and `auth_flow` filters when breaking down that expression.
For turns, show failure incidence as
`SUM(kiro_cli_turn_failure_total) / SUM(kiro_cli_user_turns)` and label it accordingly rather than
calling its complement a success rate: `kiro_cli_user_turns` includes user cancellations while the
failure counter excludes them. Do not publish an ordinary chat-session success rate because session
creation has no corresponding terminal success or failure classification.

### Derived client availability

Remove the undefined `kiro_cli.slo.availability` catalog entry and its generic `slo_target` dimension.
Replace it with explicit CloudWatch expressions for three client lifecycle scopes.

Client-turn availability is the share of non-cancelled terminal turns that avoid an unrecovered,
client-observed error:

```text
1 - (
  SUM(kiro_cli_turn_failure_total)
  /
  (SUM(kiro_cli_user_turns) - SUM(kiro_cli_turn_cancelled_total))
)
```

Sum all `turn_failure_reason` values for the overall numerator. Retain `turn_failure_reason` only for
explanatory breakdowns. The shared selectable breakdowns are `version_full`, `session_interface`,
`agent_mode`, and `agent_engine`.

Startup availability is:

```text
1 - (
  SUM(kiro_cli_startup_failure_total)
  /
  SUM(kiro_cli_run_started_total)
)
```

Its shared breakdowns are `version_full`, `session_interface`, `agent_engine`, and `os_type`.

Login availability is the same valid expression as login success rate:

```text
SUM(kiro_cli_login_success_total)
/
(SUM(kiro_cli_login_success_total)
 + SUM(kiro_cli_auth_failure_total{auth_operation=login}))
```

Its shared breakdowns are `version_full`, `auth_method`, and `auth_flow`. Guard every expression against
a zero or negative denominator and display the resulting `0..1` ratio as a percentage.

Do not sum model, TLS, retry, tool, authentication, startup, and turn error counters into one
numerator. One failed user operation can emit several of those diagnostics. Each availability
expression instead uses one terminal failure counter per unit of work, while lower-level counters
explain the cause. Do not blend the three scopes into one weighted percentage because runs, login
attempts, and turns have different denominators.

## CloudWatch derived metric inventory

These are dashboard metric-math expressions or recording rules, not client-emitted instruments:

| Derived signal | Formula or aggregation | Supported breakdowns and limits |
|---|---|---|
| Daily active installation-version count | Daily `SUM(kiro_cli_daily_heartbeat)` | `version_full`, `release_channel`, `os_type`, `install_method`. This counts installation-version days, not unique people. |
| Version adoption percentage | `SUM(kiro_cli_daily_heartbeat{version_full=selected}) / SUM(kiro_cli_daily_heartbeat)` | Use one UTC-day period. The same base metric supports stable/nightly, OS, and install-method shares. Do not infer WAU or MAU by summing days because installations repeat across days. |
| Token cache-hit percentage | `SUM(kiro_cli_tokens_consumed{token_type=input_cache_read}) / (SUM(kiro_cli_tokens_consumed{token_type=input_cache_read}) + SUM(kiro_cli_tokens_consumed{token_type=input_uncached}))` | Match `version_full`, `agent_engine`, and `model`; guard a zero denominator. |
| UI-mode launch share | `SUM(kiro_cli_ui_mode_session_started_total{ui_mode=selected}) / SUM(kiro_cli_ui_mode_session_started_total)` | `version_full`; calculate separately for `tui` and `lite`. |
| Tool outcome rate | `SUM(kiro_cli_tool_call_total{tool_outcome=...}) / SUM(kiro_cli_tool_call_total)` | `version_full`, `agent_engine`, `tool_origin`, `execution_context`, and bounded built-in tool name where applicable. |
| MCP full-availability rate | `SUM(kiro_cli_mcp_server_init_total{mcp_init_outcome=success}) / SUM(kiro_cli_mcp_server_init_total)` | `version_full`, `agent_engine`, `mcp_server_source`. |
| MCP usable-connection rate | `(SUM(kiro_cli_mcp_server_init_total{mcp_init_outcome=success}) + SUM(kiro_cli_mcp_server_init_total{mcp_init_outcome=degraded})) / SUM(kiro_cli_mcp_server_init_total)` | Same breakdowns. Server-specific diagnosis uses non-dimension fields on the metric's EMF records. |
| Model-request failure incidence | `SUM(kiro_cli_model_request_failure_total) / SUM(kiro_cli_model_invocations_total)` | `version_full`, `agent_engine`, `model`; sum `error_kind` for the overall rate. |
| Credits per model invocation | `SUM(kiro_cli_credits_consumed) / SUM(kiro_cli_model_invocations_total)` | `version_full`, `model`; aggregate model invocations across `agent_engine` before division. |
| Credits per user turn | `SUM(kiro_cli_credits_consumed) / SUM(kiro_cli_user_turns)` | `version_full`; aggregate credits across `model` and turns across interface, mode, and engine. |
| Retry recovery or exhaustion share | `SampleCount(kiro_cli_automatic_retries_per_operation{retry_outcome=selected}) / SampleCount(kiro_cli_automatic_retries_per_operation)` | `version_full`, `agent_engine`, `retry_reason`; applies only to operations that retried. |
| Run outcome rate | `SUM(kiro_cli_run_outcome_total{run_outcome=selected}) / SUM(kiro_cli_run_started_total)` | `version_full`, `session_interface`, `agent_engine`, `os_type`. |
| Crash incidence per run | `SUM(kiro_cli_crash_total) / SUM(kiro_cli_run_started_total)` | `version_full`, `agent_engine`, `os_type`; `process_role` may explain the numerator but is not a denominator dimension. |
| Turn failure incidence | `SUM(kiro_cli_turn_failure_total) / SUM(kiro_cli_user_turns)` | `version_full`, `session_interface`, `agent_mode`, `agent_engine`; this includes cancellations only in the denominator and is distinct from availability. |
| Client-turn availability | `1 - SUM(kiro_cli_turn_failure_total) / (SUM(kiro_cli_user_turns) - SUM(kiro_cli_turn_cancelled_total))` | Same turn breakdowns; all failures are terminal and unrecovered. |
| Startup availability | `1 - SUM(kiro_cli_startup_failure_total) / SUM(kiro_cli_run_started_total)` | `version_full`, `session_interface`, `agent_engine`, `os_type`. |
| Login availability | `SUM(kiro_cli_login_success_total) / (SUM(kiro_cli_login_success_total) + SUM(kiro_cli_auth_failure_total{auth_operation=login}))` | `version_full`, `auth_method`, `auth_flow`. |

Do not create derived series for ordinary chat-session success, goal completion percentage, telemetry
SDK availability, PII redaction coverage, or global upstream availability: the reviewed base signals do
not provide trustworthy denominators or independent observations for those questions.

## Fact-log decisions

KUTS currently accepts OTLP metrics and traces but not OTLP logs. The client does not configure an OTLP
log exporter for KUTS endpoints. A catalog `log_event` therefore needs a separate supported
analytics sink, conversion into a bounded metric, or removal; merely retaining its constructor does not
deliver data to KUTS.

Remove `kiro_cli_client_identity`. The catalog entry and constructor have no production producer and
would collect a pseudonymous installation ID, installation date, install method, and internal-Amazon
status for a backend cohort join that does not exist. The retained heartbeat answers the current daily
active-installation question without this fact row. Reintroduce installation identity only through a
reviewed backend analytics design when cross-day deduplication, retention, or first-seen analysis is
actually implemented.

Remove `kiro_cli_feature_first_use`. It has no production producer and was intended for activation
funnels using an anonymous installation ID, feature name, session ID, and trigger. Keep bounded
CloudWatch counters for established command and feature usage, and implement first-use funnels in
Amplitude when needed rather than retaining a dormant high-cardinality fact contract.

Replace `kiro_cli_metering_event` with the additive `kiro_cli_credits_consumed` metric. Record each
finite, non-negative service-reported credit value and use only `version_full` and canonical `model` as
dimensions. Accept `credit` case-insensitively as the service unit; unsupported units produce a bounded
local diagnostic rather than being mixed into the credit counter. Do not attach `request_id`,
`client_application`, `metering_unit`, or `metering_unit_plural`.

The metric covers telemetry-enabled installations and supports trend, release-regression, model-mix,
credits-per-invocation, and credits-per-turn dashboards. It is not an authoritative billing total;
backend metering remains the billing source of truth.

Remove `kiro_cli_user_turn_completed` from the KUTS schema and remove its OTel log translation. Its
request, session, conversation, and message identifiers plus free-form failure descriptions cannot be
delivered through the current KUTS endpoint. The retained turn-count, turn-failure, latency, token, and
model-invocation metrics cover the approved aggregate questions. Do not remove the separate legacy
telemetry event until that channel's cutover is explicitly approved; use Amplitude or a future
supported analytics sink for request-level turn funnels.

Remove `kiro_cli_tool_invoked` from the KUTS schema and remove its OTel log translation. Its raw
tool-use IDs, arbitrary tool and MCP server names, model, result, and duration cannot be delivered
through KUTS and would create a high-volume fact stream. The canonical `kiro_cli_tool_call_total` and
`kiro_cli_tool_execution_duration_ms` metrics retain the approved bounded origin, execution-context,
outcome, and built-in-tool analysis. Preserve any separate legacy event until its channel cutover.

Remove `kiro_cli_subagent_invoked` from the KUTS schema and remove its OTel log translation. Count
delegation attempts through
`kiro_cli_tool_call_total{tool_origin=builtin,builtin_tool_name=use_subagent,execution_context=main}`.
The raw subagent name
is unbounded, cannot be delivered through KUTS logs, and is not required for the approved dashboards.
Preserve any separate legacy event until its channel cutover.

Remove `kiro_cli_conversation_completed` from the KUTS schema and remove its OTel log translation. Its
conversation and session identifiers cannot be delivered through KUTS, and process exit or abandonment
prevents it from observing all ordinary conversation endings. Do not use it for session success or
completion rates. The retained session-start, run-outcome, turn, and explicit goal-outcome metrics cover
the approved aggregate lifecycle questions. Preserve any separate legacy event until its channel
cutover.

Remove the standalone `kiro_cli_mcp_server_init` log event and its OTel log translation because KUTS
does not accept OTLP logs. Preserve the approved raw server diagnosis by attaching
`mcp_server_name`, `mcp_error_kind`, and `mcp_failure_stage` to
`kiro_cli_mcp_server_init_total` as non-dimension EMF fields. Do not remove the separate legacy MCP
initialization event until that channel's cutover is explicitly approved.

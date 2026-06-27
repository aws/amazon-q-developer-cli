/**
 * TUI client-side telemetry observer. The TUI is the only vantage that sees
 * both the Rust/v2 (RustAcpClient) and KAS/v3 (KasAcpClient) agent wire, and
 * emits a client-experience view as metrics via the OTel SDK (`meter.ts`).
 *
 * Two load-bearing contracts: (1) every metric carries `engine` as a
 * first-class v2/v3 discriminator (see {@link Engine}); `kiro.tui` scope is a
 * secondary signal. (2) Only metric + attribute combos the schema catalog
 * (`crates/kiro-telemetry-schema/schema/{metrics,types}.yaml`) allows are
 * emitted — anything else is silently dropped by endpoint validation, and KUTS
 * is metrics-only (no logs), so high-cardinality fields like `tool_name` are
 * never recorded.
 */

import {
  counter as meterCounter,
  gauge as meterGauge,
  histogram as meterHistogram,
  forceFlushMetrics,
  type MetricAttributes,
} from './meter';
import type { ProcessHealthSnapshot } from './process-health-collector';

export const TUI_SCOPE = 'kiro.tui';
/**
 * Default `engine` when a caller omits it. Defaults to v3 so the existing
 * KAS/v3 call sites (which predate the v2 wiring and omit `engine`) stay
 * byte-for-byte unchanged; v2 call sites pass `engine: 'v2'` explicitly.
 */
export const DEFAULT_ENGINE = 'v3';

/**
 * Canonical engine discriminator threaded through every record fn as the
 * `engine` attribute. Serves both the KAS/v3 (KasAcpClient) and Rust/v2
 * (RustAcpClient) client-experience views (telemetry-metric-inventory.md
 * §H.4/§H.6); defaults to {@link DEFAULT_ENGINE} to keep v3 byte-identical.
 */
export type Engine = 'v2' | 'v3';

/**
 * Bucket bounds (seconds) for kiro_cli_user_turn_duration_seconds, matching the
 * V2 buckets so both engines share shape on the same Prometheus series.
 */
const USER_TURN_DURATION_BOUNDS = [1, 2, 5, 10, 30, 60, 120, 300, 600];

/**
 * Resolve the launcher-provided `version_minor_bucket` (§C4). The bucket is
 * relative to the latest known release, which only the Rust launcher knows; it
 * threads it via `KIRO_VERSION_MINOR_BUCKET`. When unset we fall back to
 * `_other_`, NOT the old hardcoded `current` (which silently mislabeled every
 * stale client as up-to-date).
 */
const VERSION_MINOR_BUCKETS = new Set([
  'current',
  'current-1',
  'current-2',
  'older',
  '_other_',
]);
export function versionMinorBucketFromEnv(): string {
  const raw = process.env['KIRO_VERSION_MINOR_BUCKET']?.trim();
  return raw && VERSION_MINOR_BUCKETS.has(raw) ? raw : '_other_';
}

/** Transport seams; default to the real transports. Tests inject spies. */
export interface TuiTelemetryDeps {
  counter?: typeof meterCounter;
  gauge?: typeof meterGauge;
  histogram?: typeof meterHistogram;
}

/**
 * Normalize a mode/agent id into the catalog `mode` allowed-values enum.
 * Mirrors the Rust `Mode::from_name` (crates/kiro-telemetry/src/metric.rs) so a
 * raw TUI/ACP mode id (`default`, `kiro_planner`, `spec`, …) does not leak past
 * the closed enum and collapse to `_other_` on the dashboard. Unrecognized ids
 * (e.g. custom agents) still fall back to `_other_`, matching the Rust default.
 */
export function modeFromId(modeId: string | undefined): string {
  switch (
    (modeId ?? '').trim().replace(/^\/+/, '').toLowerCase().replace(/-/g, '_')
  ) {
    case 'oneshot':
      return 'oneshot';
    case 'agent':
      return 'agent';
    case 'plan':
    case 'quick_plan':
    case 'kiro_planner':
    case 'planner':
      return 'plan';
    case 'review':
      return 'review';
    case 'tangent':
    case 'tangent_mode':
      return 'tangent';
    case 'voice':
      return 'voice';
    case 'acp_external':
      return 'acp_external';
    case 'generate_agent':
    case 'generateagent':
      return 'generate_agent';
    case '':
    case 'default':
    case 'kiro':
    case 'vibe':
    case 'interactive':
      return 'interactive';
    default:
      return '_other_';
  }
}

/**
 * Map a KAS turn-completion status string to the catalog `result`
 * allowed-values ({@link recordTuiUserTurn}). Unknown statuses fall back to
 * `_other_` so they still validate.
 */
export function resultFromStatus(
  status: string | undefined
): 'success' | 'failed' | 'cancelled' | '_other_' {
  switch ((status ?? '').toLowerCase()) {
    case 'completed':
    case 'success':
    case 'succeeded':
      return 'success';
    case 'cancelled':
    case 'canceled':
    case 'interrupted':
      return 'cancelled';
    case 'errored':
    case 'error':
    case 'failed':
      return 'failed';
    default:
      return '_other_';
  }
}

/** Allowed `turn_outcome_reason` enum (mirrors the Rust TurnOutcomeReason). */
export type TurnOutcomeReason =
  | 'interrupted'
  | 'model_error'
  | 'tool_error'
  | 'timeout'
  | 'context_limit'
  | '_other_';

/**
 * Bucket a KAS turn-completion status into the `turn_outcome_reason` enum for
 * `kiro_cli_turn_outcome_total`. Mirrors the Rust `TurnOutcomeReason::from_name`.
 * Returns undefined for a success status — the outcome counter is only emitted
 * for non-success turns.
 */
export function turnOutcomeReasonFromStatus(
  status: string | undefined
): TurnOutcomeReason | undefined {
  const normalized = (status ?? '').toLowerCase();
  // An absent/empty status carries no outcome to report — treat it as a
  // success (the counter only tracks known failure modes), NOT as an `_other_`
  // failure. The V2 path relies on this: a successful `end_turn` maps to an
  // undefined status, and without this guard the failure-only counter would
  // fire spuriously with turn_outcome_reason=_other_ on every success.
  if (normalized === '') return undefined;
  switch (normalized) {
    case 'completed':
    case 'success':
    case 'succeeded':
      return undefined;
    case 'cancelled':
    case 'canceled':
    case 'interrupted':
      return 'interrupted';
    case 'model_error':
    case 'errored':
    case 'error':
    case 'failed':
      return 'model_error';
    case 'tool_error':
      return 'tool_error';
    case 'timeout':
      return 'timeout';
    case 'context_limit':
    case 'context_window_exceeded':
      return 'context_limit';
    default:
      return '_other_';
  }
}

/** Allowed `subagent_name_class` enum (mirrors the Rust SubagentNameClass). */
export type SubagentNameClass =
  | 'code_review'
  | 'general'
  | 'custom'
  | '_other_';

/** Bucket a raw sub-agent name into the bounded `subagent_name_class` enum. */
export function subagentNameClassFromName(
  name: string | undefined
): SubagentNameClass {
  switch ((name ?? '').toLowerCase().replace(/-/g, '_')) {
    case 'code_review':
      return 'code_review';
    case 'general':
      return 'general';
    case 'custom':
      return 'custom';
    default:
      return '_other_';
  }
}

/**
 * Map a `token_type` into the catalog enum. KAS reports four token kinds; the
 * schema closed enum names them `input_uncached / input_cache_read /
 * input_cache_write / output`.
 */
export type TokenType =
  | 'input_uncached'
  | 'input_cache_read'
  | 'input_cache_write'
  | 'output';

/**
 * Under `KIRO_TEST_MODE` with no injected transport, short-circuit so unit
 * tests of unrelated TUI flows don't emit real OTLP traffic.
 */
function suppressedInTest(deps?: TuiTelemetryDeps): boolean {
  return (
    process.env['KIRO_TEST_MODE'] === 'true' &&
    deps?.counter === undefined &&
    deps?.gauge === undefined &&
    deps?.histogram === undefined
  );
}

function counterFn(deps?: TuiTelemetryDeps): typeof meterCounter {
  return deps?.counter ?? meterCounter;
}
function gaugeFn(deps?: TuiTelemetryDeps): typeof meterGauge {
  return deps?.gauge ?? meterGauge;
}
function histogramFn(deps?: TuiTelemetryDeps): typeof meterHistogram {
  return deps?.histogram ?? meterHistogram;
}

/** A session began (`kiro_cli_chat_session_started_total`). One per session (caller dedupes). */
export function recordTuiSessionStarted(
  args: { mode: string; versionMinorBucket: string; engine?: Engine },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_chat_session_started_total',
    1,
    {
      mode: args.mode,
      version_minor_bucket: args.versionMinorBucket,
      engine: args.engine ?? DEFAULT_ENGINE,
    },
    TUI_SCOPE
  );
}

/**
 * A user turn completed: the count (`kiro_cli_user_turns`) + latency histogram
 * (`kiro_cli_user_turn_duration_seconds`). The schema allows `engine` on both
 * (it previously rejected attributes on the histogram), so both carry it.
 */
export function recordTuiUserTurn(
  args: {
    model: string;
    result: 'success' | 'failed' | 'cancelled' | '_other_';
    isSubagent: boolean;
    mode: string;
    chatConversationType: string;
    /**
     * Wall-clock turn duration. Omitted when KAS did not report `elapsedTime`
     * — in that case the latency histogram point is NOT emitted (coercing a
     * missing duration to 0 would pollute the histogram with phantom
     * 0-second turns and bias the percentiles downward). The turn count
     * always fires regardless.
     */
    durationSeconds?: number;
    engine?: Engine;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  const engine = args.engine ?? DEFAULT_ENGINE;

  counterFn(deps)(
    'kiro_cli_user_turns',
    1,
    {
      model: args.model,
      result: args.result,
      is_subagent: String(args.isSubagent),
      mode: args.mode,
      engine,
    },
    TUI_SCOPE
  );

  if (args.durationSeconds !== undefined) {
    histogramFn(deps)(
      'kiro_cli_user_turn_duration_seconds',
      args.durationSeconds,
      {
        model: args.model,
        chat_conversation_type: args.chatConversationType,
        is_subagent: String(args.isSubagent),
        mode: args.mode,
        engine,
      },
      TUI_SCOPE,
      USER_TURN_DURATION_BOUNDS
    );
  }
}

/**
 * A tool call finished: the count (`kiro_cli_tool_call_total`) + latency histogram
 * (`kiro_cli_tool_execution_duration_ms`) when a duration was measured (§C4).
 * `tool_name` is high-cardinality and metric-forbidden, so it is not recorded.
 */
export function recordTuiToolCall(
  args: {
    toolOrigin: 'builtin' | 'mcp' | 'subagent_delegate';
    builtinToolName?: string;
    outcome: 'success' | 'error' | 'cancelled' | 'denied';
    executionDurationMs?: number;
    engine?: Engine;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  const engine = args.engine ?? DEFAULT_ENGINE;

  const isSuccess = args.outcome === 'success';
  const countAttrs: MetricAttributes = {
    tool_origin: args.toolOrigin,
    outcome: args.outcome,
    engine,
  };
  // builtin_tool_name is only meaningful (and only allowed-by-convention)
  // for builtin tools; omit it otherwise to stay within the cardinality cap.
  if (args.toolOrigin === 'builtin' && args.builtinToolName) {
    countAttrs['builtin_tool_name'] = args.builtinToolName;
  }

  counterFn(deps)('kiro_cli_tool_call_total', 1, countAttrs, TUI_SCOPE);

  // Latency only for a finite positive duration (mirrors the Rust
  // tool_execution_duration_ms_for_invocation guard).
  if (
    args.executionDurationMs !== undefined &&
    Number.isFinite(args.executionDurationMs) &&
    args.executionDurationMs > 0
  ) {
    histogramFn(deps)(
      'kiro_cli_tool_execution_duration_ms',
      args.executionDurationMs,
      {
        tool_origin: args.toolOrigin,
        is_success: String(isSuccess),
        engine,
      },
      TUI_SCOPE
    );
  }
}

/**
 * Per-engine token economics (`kiro_cli_tokens_consumed`, §C4 backfill). One
 * point per non-zero token kind; the engine split makes V2-vs-V3 token usage a
 * clean label split.
 */
export function recordTuiTokensConsumed(
  args: {
    model: string;
    isSubagent: boolean;
    tokens: Partial<Record<TokenType, number>>;
    engine?: Engine;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  const engine = args.engine ?? DEFAULT_ENGINE;
  const emit = counterFn(deps);
  for (const [tokenType, value] of Object.entries(args.tokens) as Array<
    [TokenType, number | undefined]
  >) {
    if (value === undefined || !Number.isFinite(value) || value <= 0) continue;
    emit(
      'kiro_cli_tokens_consumed',
      value,
      {
        model: args.model,
        token_type: tokenType,
        is_subagent: String(args.isSubagent),
        engine,
      },
      TUI_SCOPE
    );
  }
}

/** Per-engine model mix (`kiro_cli_model_invocations_total`, §C4). */
export function recordTuiModelInvocation(
  args: { model: string; engine?: Engine },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_model_invocations_total',
    1,
    { model: args.model, engine: args.engine ?? DEFAULT_ENGINE },
    TUI_SCOPE
  );
}

/**
 * Bucketed non-success turn outcome (`kiro_cli_turn_outcome_total`, §C4).
 * No-op for success — the counter only tracks failure modes (see
 * {@link turnOutcomeReasonFromStatus}).
 */
export function recordTuiTurnOutcome(
  args: {
    status: string | undefined;
    model: string;
    mode: string;
    engine?: Engine;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  const reason = turnOutcomeReasonFromStatus(args.status);
  if (reason === undefined) return;
  counterFn(deps)(
    'kiro_cli_turn_outcome_total',
    1,
    {
      turn_outcome_reason: reason,
      model: args.model,
      mode: args.mode,
      engine: args.engine ?? DEFAULT_ENGINE,
    },
    TUI_SCOPE
  );
}

/**
 * Per-engine tool latency (`kiro_cli_tool_execution_duration_ms`, §C4). Standalone
 * entry point for durations outside the {@link recordTuiToolCall} inline path.
 */
export function recordTuiToolExecutionDuration(
  args: {
    toolOrigin: 'builtin' | 'mcp' | 'subagent_delegate';
    isSuccess: boolean;
    durationMs: number;
    engine?: Engine;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  if (!Number.isFinite(args.durationMs) || args.durationMs <= 0) return;
  histogramFn(deps)(
    'kiro_cli_tool_execution_duration_ms',
    args.durationMs,
    {
      tool_origin: args.toolOrigin,
      is_success: String(args.isSuccess),
      engine: args.engine ?? DEFAULT_ENGINE,
    },
    TUI_SCOPE
  );
}

/**
 * Per-engine context pressure (`kiro_cli_context_usage_percentage`, §C4). A
 * GAUGE (last-value); the meter wrapper re-reads it per attribute set at export
 * time, so distinct model/subagent combos report independently.
 */
export function recordTuiContextUsage(
  args: {
    model: string;
    isSubagent: boolean;
    percentage: number;
    engine?: Engine;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  if (!Number.isFinite(args.percentage)) return;
  gaugeFn(deps)(
    'kiro_cli_context_usage_percentage',
    args.percentage,
    {
      model: args.model,
      is_subagent: String(args.isSubagent),
      engine: args.engine ?? DEFAULT_ENGINE,
    },
    TUI_SCOPE
  );
}

/** Per-engine mode usage (`kiro_cli_mode_active_total`, §C4). */
export function recordTuiModeActive(
  args: { mode: string; engine?: Engine },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_mode_active_total',
    1,
    { mode: args.mode, engine: args.engine ?? DEFAULT_ENGINE },
    TUI_SCOPE
  );
}

/**
 * Sub-agent delegation fan-out (`kiro_cli_subagent_delegations_total`, §C4).
 * Raw name is bucketed via {@link subagentNameClassFromName} for the cardinality cap.
 */
export function recordTuiSubagentDelegation(
  args: {
    subagentName: string | undefined;
    model: string;
    engine?: Engine;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_subagent_delegations_total',
    1,
    {
      subagent_name_class: subagentNameClassFromName(args.subagentName),
      model: args.model,
      engine: args.engine ?? DEFAULT_ENGINE,
    },
    TUI_SCOPE
  );
}

/** Origin of a V3 tool call, decided at ToolCall time. */
export type ToolOrigin = 'builtin' | 'mcp' | 'subagent_delegate';

/** Start-time facts a ToolCall carries, captured for the matching finish. */
export interface TuiToolCallStart {
  name: string;
  origin: ToolOrigin;
  mcpServerName?: string;
}

/**
 * Correlates ToolCall → ToolCallFinished events (keyed by toolCallId) into tool
 * telemetry, deriving the start→finish duration. Both ACP clients drive an
 * instance: KasAcpClient with the default `engine='v3'`, RustAcpClient with
 * `engine='v2'` (§H.4); the construction-time `engine` is stamped on every
 * metric it emits.
 */
export class TuiToolCallObserver {
  private readonly inFlight = new Map<
    string,
    TuiToolCallStart & { startMs: number }
  >();
  private readonly deps?: TuiTelemetryDeps;
  private readonly engine: Engine;

  constructor(deps?: TuiTelemetryDeps, engine: Engine = DEFAULT_ENGINE) {
    this.deps = deps;
    this.engine = engine;
  }

  start(toolCallId: string, info: TuiToolCallStart): void {
    this.inFlight.set(toolCallId, { ...info, startMs: performance.now() });
  }

  /**
   * Finish a tool call and emit its metrics. An unmatched id (no prior `start`,
   * e.g. a tool call from before this session) falls back to a builtin/`unknown`
   * shape with no measured duration.
   */
  finish(
    toolCallId: string,
    args: {
      outcome: 'success' | 'error' | 'cancelled' | 'denied';
      model: string;
    }
  ): void {
    const started = this.inFlight.get(toolCallId);
    this.inFlight.delete(toolCallId);
    const toolName = started?.name ?? 'unknown';
    const toolOrigin = started?.origin ?? 'builtin';
    const executionDurationMs =
      started !== undefined
        ? Math.max(0, Math.round(performance.now() - started.startMs))
        : undefined;

    recordTuiToolCall(
      {
        toolOrigin,
        ...(toolOrigin === 'builtin' ? { builtinToolName: toolName } : {}),
        outcome: args.outcome,
        ...(executionDurationMs !== undefined ? { executionDurationMs } : {}),
        engine: this.engine,
      },
      this.deps
    );

    if (toolOrigin === 'subagent_delegate') {
      recordTuiSubagentDelegation(
        {
          subagentName: toolName,
          model: args.model,
          engine: this.engine,
        },
        this.deps
      );
    }
  }

  /** Drop all in-flight state (session change / teardown). */
  reset(): void {
    this.inFlight.clear();
  }
}

/**
 * Process role for TUI-sampled §E perf metrics. The bun TUI samples itself, so
 * its metrics are `process_role = tui` (vs the host's `host`/`kas_subprocess`);
 * tagging it lets one metric name reassemble the process tree downstream.
 */
const PROCESS_ROLE_TUI = 'tui';

/** MiB → bytes. The §E gauges declare unit `By`; the sampler reports MiB. */
const MIB = 1024 * 1024;
/** ms → seconds. The §E TUI histograms declare unit `s`; the sampler reports ms. */
const MS_PER_S = 1000;

/**
 * Histogram bucket bounds (seconds) for the §E TUI latency histograms
 * (event-loop delay, input latency, render duration). Sized sub-ms to
 * hundreds-of-ms so the raw histogram is re-aggregatable downstream, rather
 * than the pre-reduced p99/p95 the log shipped.
 */
const TUI_LATENCY_BOUNDS_S = [
  0.0005, 0.001, 0.002, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1,
];

/**
 * Promote one process-health snapshot to §E SDK metrics. Each metric carries
 * ONLY its catalog-allowed attribute subset, else endpoint validation drops it.
 * Note `kiro_cli.process.cpu.utilization` is a HISTOGRAM, not a gauge — the host
 * declares it so, and a single-observation histogram lets the V2-host and
 * V3-TUI series merge on one Prometheus metric.
 */
export function recordTuiProcessHealth(
  snapshot: ProcessHealthSnapshot,
  engine: Engine = DEFAULT_ENGINE,
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;

  const versionMinorBucket = versionMinorBucketFromEnv();
  // agent_kind is a 1:1 function of engine on the TUI path (v3→kas, v2→v2);
  // catalog allowed_values are [v1, v2, subagent, kas, _other_].
  const agentKind = engine === 'v3' ? 'kas' : 'v2';
  const g = gaugeFn(deps);
  const h = histogramFn(deps);

  // Memory gauges (bytes): rss carries agent_kind, but peak_rss/heap_used do not
  // list it in their catalog attr set, so they omit it.
  g(
    'kiro_cli.process.memory.rss',
    snapshot.rssMb * MIB,
    {
      version_minor_bucket: versionMinorBucket,
      agent_kind: agentKind,
      engine,
      process_role: PROCESS_ROLE_TUI,
    },
    TUI_SCOPE
  );
  g(
    'kiro_cli.process.memory.peak_rss',
    snapshot.peakRssMb * MIB,
    {
      version_minor_bucket: versionMinorBucket,
      engine,
      process_role: PROCESS_ROLE_TUI,
    },
    TUI_SCOPE
  );
  g(
    'kiro_cli.process.memory.heap_used',
    snapshot.heapUsedMb * MIB,
    {
      version_minor_bucket: versionMinorBucket,
      engine,
      process_role: PROCESS_ROLE_TUI,
    },
    TUI_SCOPE
  );

  // CPU: sampler reports user/system as PERCENT of the 60s window; the catalog
  // unit is "1" (a fraction), so combine and divide by 100.
  const cpuRatio = (snapshot.cpuUserPct + snapshot.cpuSystemPct) / 100;
  if (Number.isFinite(cpuRatio) && cpuRatio >= 0) {
    h(
      'kiro_cli.process.cpu.utilization',
      cpuRatio,
      {
        version_minor_bucket: versionMinorBucket,
        agent_kind: agentKind,
        engine,
        process_role: PROCESS_ROLE_TUI,
      },
      TUI_SCOPE
    );
  }

  if (snapshot.eventLoopP99Ms !== null && snapshot.eventLoopP99Ms >= 0) {
    h(
      'kiro_cli.tui.event_loop.delay',
      snapshot.eventLoopP99Ms / MS_PER_S,
      { engine, process_role: PROCESS_ROLE_TUI },
      TUI_SCOPE,
      TUI_LATENCY_BOUNDS_S
    );
  }

  if (snapshot.inputLatencyP95Ms !== null && snapshot.inputLatencyP95Ms >= 0) {
    h(
      'kiro_cli.tui.input.latency',
      snapshot.inputLatencyP95Ms / MS_PER_S,
      { engine, process_role: PROCESS_ROLE_TUI },
      TUI_SCOPE,
      TUI_LATENCY_BOUNDS_S
    );
  }

  // render_kind=full only when full redraws happened this window; otherwise the
  // sampled lastRenderMs is a partial render.
  if (snapshot.lastRenderMs > 0) {
    h(
      'kiro_cli.tui.render.duration',
      snapshot.lastRenderMs / MS_PER_S,
      {
        render_kind: snapshot.fullRedrawsPerMin > 0 ? 'full' : 'partial',
        engine,
        process_role: PROCESS_ROLE_TUI,
      },
      TUI_SCOPE,
      TUI_LATENCY_BOUNDS_S
    );
  }
}

/**
 * Re-exported so the process-health collector's exit path can flush the final
 * (peak_rss-authoritative) window without importing meter.ts directly.
 */
export { forceFlushMetrics };

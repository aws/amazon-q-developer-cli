/**
 * TUI client-side telemetry observer. The TUI is the only vantage that sees
 * both the Rust/v2 (RustAcpClient) and KAS/v3 (KasAcpClient) agent wire, and
 * emits a client-experience view as metrics via the OTel SDK (`meter.ts`).
 *
 * Two load-bearing contracts: (1) every engine-specific metric carries
 * `agent_engine` as a first-class v2/v3 discriminator; `kiro.tui` scope is
 * producer provenance only. (2) Only metric + attribute combos the schema catalog
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
  type MetricLogProperties,
} from './meter';
import type { ProcessHealthSnapshot } from './process-health-collector';
import {
  AgentEventType,
  ContentType,
  type AgentStreamEvent,
} from '../types/agent-events';
import { canonicalSlashCommandName } from './slash-command-telemetry';
import type { WorkflowRestoreSummary } from '../types/workflow.js';
import type {
  WorkflowControlAction,
  WorkflowControlResult,
  WorkflowRestoreMetricResult,
  WorkflowTelemetryObservation,
} from './workflow-telemetry.js';

export const TUI_SCOPE = 'kiro.tui';
/**
 * Default engine when a caller omits it. Defaults to v3 so KAS call sites can
 * stay concise while V2 call sites pass `engine: 'v2'` explicitly.
 */
export const DEFAULT_ENGINE = 'v3';

/** Engine discriminator shared by the KAS and Rust client-experience metrics. */
export type Engine = 'v2' | 'v3';
const SESSION_INTERFACE = 'interactive_cli';

/**
 * Bucket bounds (seconds) for kiro_cli_user_turn_duration_seconds, matching the
 * V2 buckets so both engines share shape on the same Prometheus series.
 */
const USER_TURN_DURATION_BOUNDS = [1, 2, 5, 10, 30, 60, 120, 300, 600];
const FIRST_VISIBLE_RESPONSE_BOUNDS_MS = [
  50, 100, 250, 500, 1000, 2500, 5000, 10_000, 30_000, 60_000,
];

/** Transport seams; default to the real transports. Tests inject spies. */
export interface TuiTelemetryDeps {
  counter?: typeof meterCounter;
  gauge?: typeof meterGauge;
  histogram?: typeof meterHistogram;
}

export type AgentMode = 'default' | 'plan' | 'spec' | 'autonomous' | 'custom';

export function modeFromId(modeId: string | undefined): AgentMode {
  const normalized = (modeId ?? '')
    .trim()
    .replace(/^\/+/, '')
    .toLowerCase()
    .replace(/-/g, '_');
  switch (normalized) {
    case 'plan':
    case 'quick_plan':
    case 'kiro_planner':
    case 'planner':
      return 'plan';
    case 'spec':
    case 'kiro_spec':
      return 'spec';
    case 'autonomous':
      return 'autonomous';
    case '':
    case 'default':
    case 'kiro':
    case 'kiro_default':
    case 'vibe':
    case 'interactive':
      return 'default';
    default:
      return 'custom';
  }
}

/** Map a KAS turn-completion status to the catalog turn result. */
export function resultFromStatus(
  status: string | undefined
): 'success' | 'failed' | 'cancelled' | '_other_' {
  const normalized = (status ?? '').trim().toLowerCase();
  switch (normalized) {
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
      return normalized === '' ? '_other_' : 'failed';
  }
}

/** Bounded failure reasons for a terminal user turn. */
export type TurnFailureReason =
  | 'model_error'
  | 'tool_error'
  | 'timeout'
  | 'context_limit'
  | 'execution_limit'
  | 'internal_error'
  | 'unknown';

/**
 * Bucket a KAS turn-completion status into the `turn_failure_reason` enum for
 * `kiro_cli_turn_failure_total`.
 * Returns undefined for a success status — the outcome counter is only emitted
 * for non-success turns.
 */
export function turnFailureReasonFromStatus(
  status: string | undefined
): TurnFailureReason | undefined {
  const normalized = (status ?? '').trim().toLowerCase();
  if (normalized === '') return undefined;
  switch (normalized) {
    case 'completed':
    case 'success':
    case 'succeeded':
      return undefined;
    case 'cancelled':
    case 'canceled':
    case 'interrupted':
      return undefined;
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
    case 'max_tokens':
    case 'max_turn_requests':
    case 'execution_limit':
      return 'execution_limit';
    case 'internal_error':
      return 'internal_error';
    default:
      return 'unknown';
  }
}

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
  args: {
    mode: string;
    version: string;
    engine?: Engine;
    logProperties?: MetricLogProperties;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_chat_session_started_total',
    1,
    {
      version_full: args.version,
      session_interface: SESSION_INTERFACE,
      agent_mode: modeFromId(args.mode),
      agent_engine: args.engine ?? DEFAULT_ENGINE,
    },
    TUI_SCOPE,
    args.logProperties
  );
}

export function recordTuiUiModeSessionStarted(
  args: { mode: string; version: string },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  const mode =
    args.mode === 'tui' || args.mode === 'lite' ? args.mode : 'unknown';
  counterFn(deps)(
    'kiro_cli_ui_mode_session_started_total',
    1,
    {
      version_full: args.version,
      ui_mode: mode,
    },
    TUI_SCOPE
  );
}

export function recordTuiSlashCommand(
  args: {
    command: string;
    version: string;
    engine?: Engine;
    logProperties?: MetricLogProperties;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_slash_command_invoked_total',
    1,
    {
      version_full: args.version,
      agent_engine: args.engine ?? DEFAULT_ENGINE,
      command: canonicalSlashCommandName(args.command),
    },
    TUI_SCOPE,
    args.logProperties
  );
}

export function recordTuiFirstVisibleResponse(
  args: {
    milliseconds: number;
    mode: string;
    version: string;
    engine?: Engine;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  if (!Number.isFinite(args.milliseconds) || args.milliseconds < 0) return;
  histogramFn(deps)(
    'kiro_cli_time_to_first_visible_response_ms',
    args.milliseconds,
    {
      version_full: args.version,
      session_interface: SESSION_INTERFACE,
      agent_mode: modeFromId(args.mode),
      agent_engine: args.engine ?? DEFAULT_ENGINE,
    },
    TUI_SCOPE,
    FIRST_VISIBLE_RESPONSE_BOUNDS_MS
  );
}

function isVisibleAgentEvent(event: AgentStreamEvent): boolean {
  switch (event.type) {
    case AgentEventType.Content:
    case AgentEventType.Thought:
      return (
        event.content.type !== ContentType.Text ||
        event.content.text.trim().length > 0
      );
    case AgentEventType.ToolCall:
    case AgentEventType.ApprovalRequest:
    case AgentEventType.QuestionRequest:
    case AgentEventType.RateLimitError:
    case AgentEventType.AuthError:
    case AgentEventType.SessionError:
    case AgentEventType.AgentNotFound:
    case AgentEventType.AgentConfigError:
    case AgentEventType.ModelRefusal:
      return true;
    default:
      return false;
  }
}

export class TuiFirstVisibleResponseObserver {
  private pending?: {
    startMs: number;
    mode: string;
    version: string;
    engine: Engine;
  };

  constructor(private readonly deps?: TuiTelemetryDeps) {}

  start(
    args: { mode: string; version: string; engine: Engine },
    startMs = performance.now()
  ): void {
    this.pending = { ...args, startMs };
  }

  observe(event: AgentStreamEvent, nowMs = performance.now()): void {
    const pending = this.pending;
    if (!pending || !isVisibleAgentEvent(event)) return;
    this.pending = undefined;
    recordTuiFirstVisibleResponse(
      {
        milliseconds: Math.max(0, nowMs - pending.startMs),
        mode: pending.mode,
        version: pending.version,
        engine: pending.engine,
      },
      this.deps
    );
  }

  cancel(): void {
    this.pending = undefined;
  }
}

/** Allowed `cloud_event` lifecycle enum (mirrors the schema catalog type). */
export type CloudSessionEvent =
  | 'created'
  | 'create_failed'
  | 'reattached'
  | 'ready'
  | 'provision_failed'
  | 'detached'
  | 'turned_off'
  | 'fell_back_local';

/**
 * A cloud-sandbox session lifecycle event
 * (`kiro_cli_cloud_session_lifecycle_total`): `created` (a cloud-sandbox
 * session was created), `create_failed` (the cloud `session/new` was rejected),
 * `reattached` (the CLI resumed a still-running cloud session), `ready` (the
 * sandbox reached a live status after provisioning; paired with the
 * `kiro_cli_cloud_session_ready_seconds` latency histogram), `provision_failed`
 * (the sandbox reported a failed activity status), `detached` (the CLI
 * disconnected but left it running), `turned_off` (the user stopped it via the
 * /quit prompt), or `fell_back_local` (a cloud sandbox was requested but KAS
 * did not advertise the placement, so the session ran locally instead).
 */
export function recordTuiCloudSession(
  args: { event: CloudSessionEvent; version: string },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_cloud_session_lifecycle_total',
    1,
    { version_full: args.version, cloud_event: args.event },
    TUI_SCOPE
  );
}

/**
 * Bucket bounds (seconds) for `kiro_cli_cloud_session_ready_seconds` — how long
 * a cloud sandbox takes to reach a live status after creation. Wider than the
 * turn buckets: provisioning a sandbox is a slower, cold-start operation.
 */
const CLOUD_READY_DURATION_BOUNDS = [1, 2, 5, 10, 20, 30, 60, 120, 300];

/**
 * A cloud sandbox reached a live status after provisioning. Emits both the
 * `ready` lifecycle counter and the `kiro_cli_cloud_session_ready_seconds`
 * latency histogram (creation to first live status). Dark-safe: only the cloud
 * roster path calls this, which never fires on released builds.
 */
export function recordTuiCloudSessionReady(
  args: { durationSeconds: number; version: string },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_cloud_session_lifecycle_total',
    1,
    { version_full: args.version, cloud_event: 'ready' },
    TUI_SCOPE
  );
  if (args.durationSeconds > 0) {
    histogramFn(deps)(
      'kiro_cli_cloud_session_ready_seconds',
      args.durationSeconds,
      { version_full: args.version },
      TUI_SCOPE,
      CLOUD_READY_DURATION_BOUNDS
    );
  }
}

export type AutonomousEvent =
  | 'enabled'
  | 'disabled'
  | 'switch_failed'
  | 'reverted';

export function recordTuiAutonomousMode(
  args: { event: AutonomousEvent; version: string; engine?: Engine },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_autonomous_mode_total',
    1,
    {
      version_full: args.version,
      autonomous_event: args.event,
      agent_engine: args.engine ?? DEFAULT_ENGINE,
    },
    TUI_SCOPE
  );
}

export type CloudOpLabel =
  | 'session_new'
  | 'session_load'
  | 'turn_stream'
  | 'source_providers_list'
  | 'source_providers_resources'
  | 'list_sessions'
  | 'delete_session';

export type CloudErrorKindLabel =
  | 'throttling'
  | 'auth'
  | 'version_skew'
  | 'not_found'
  | 'network'
  | 'timeout'
  | 'stream_truncated'
  | 'server_error'
  | 'other';

export function recordTuiCloudError(
  args: {
    op: CloudOpLabel;
    kind: CloudErrorKindLabel;
    version: string;
    engine?: Engine;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_cloud_error_total',
    1,
    {
      version_full: args.version,
      cloud_op: args.op,
      cloud_error_kind: args.kind,
      agent_engine: args.engine ?? DEFAULT_ENGINE,
    },
    TUI_SCOPE
  );
}

export type AttachKind = 'image' | 'document' | 'text' | 'binary';
export type AttachSizeBucket =
  | 'under_64k'
  | 'under_1m'
  | 'under_5m'
  | 'over_5m';

export function attachSizeBucket(bytes: number): AttachSizeBucket {
  if (bytes < 64 * 1024) return 'under_64k';
  if (bytes < 1024 * 1024) return 'under_1m';
  if (bytes < 5 * 1024 * 1024) return 'under_5m';
  return 'over_5m';
}

export function recordTuiCloudAttach(
  args: {
    kind: AttachKind;
    sizeBytes: number;
    version: string;
    engine?: Engine;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_cloud_attach_total',
    1,
    {
      version_full: args.version,
      attach_kind: args.kind,
      attach_size_bucket: attachSizeBucket(args.sizeBytes),
      agent_engine: args.engine ?? DEFAULT_ENGINE,
    },
    TUI_SCOPE
  );
}

export type RepoAttachEvent = 'opened' | 'submitted';
export type RepoCountBucket = 'none' | '1' | '2' | '3_5' | '6_plus';

export function repoCountBucket(n: number | undefined): RepoCountBucket {
  if (!n || n <= 0) return 'none';
  if (n === 1) return '1';
  if (n === 2) return '2';
  if (n <= 5) return '3_5';
  return '6_plus';
}

export function recordTuiCloudRepoAttach(
  args: {
    event: RepoAttachEvent;
    repoCount?: number;
    version: string;
    engine?: Engine;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_cloud_repo_attach_total',
    1,
    {
      version_full: args.version,
      repo_attach_event: args.event,
      repo_count_bucket:
        args.event === 'submitted' ? repoCountBucket(args.repoCount) : 'none',
      agent_engine: args.engine ?? DEFAULT_ENGINE,
    },
    TUI_SCOPE
  );
}

/** Categories of the /config panel; 'menu' is the top-level category table. */
export type ConfigPanelCategory =
  | 'menu'
  | 'agents'
  | 'mcp'
  | 'powers'
  | 'steering'
  | 'skills'
  | 'hooks'
  | 'env'
  | 'unknown';

const CONFIG_PANEL_CATEGORIES: ReadonlySet<string> = new Set([
  'menu',
  'agents',
  'mcp',
  'powers',
  'steering',
  'skills',
  'hooks',
  'env',
]);

export function configPanelCategory(
  value: string | undefined
): ConfigPanelCategory {
  const normalized = (value ?? 'menu').trim().toLowerCase();
  return CONFIG_PANEL_CATEGORIES.has(normalized)
    ? (normalized as ConfigPanelCategory)
    : 'unknown';
}

/**
 * A /config view was opened (`kiro_cli_config_panel_total`): the top-level
 * category table ('menu') or one category — whether reached by typing
 * `/config <sub>` or by selecting a row inside the panel.
 */
export function recordTuiConfigPanel(
  args: { category?: string; version: string; engine?: Engine },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_config_panel_total',
    1,
    {
      version_full: args.version,
      config_category: configPanelCategory(args.category),
      agent_engine: args.engine ?? DEFAULT_ENGINE,
    },
    TUI_SCOPE
  );
}

export type DiagnosticSeverity = 'error' | 'warning' | 'info' | 'unknown';

export function diagnosticSeverity(
  value: string | undefined
): DiagnosticSeverity {
  const normalized = (value ?? '').trim().toLowerCase();
  return normalized === 'error' ||
    normalized === 'warning' ||
    normalized === 'info'
    ? normalized
    : 'unknown';
}

/**
 * Cloud-config sync diagnostics arrived (`kiro_cli_cloud_config_diagnostic_total`).
 * One count per diagnostic per push; pushes replace the held set, so the rate
 * reflects how often clients are being shown unhealthy sync state.
 */
export function recordTuiCloudConfigDiagnostics(
  args: {
    severities: readonly string[];
    version: string;
    engine?: Engine;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  const emit = counterFn(deps);
  for (const severity of args.severities) {
    emit(
      'kiro_cli_cloud_config_diagnostic_total',
      1,
      {
        version_full: args.version,
        diagnostic_severity: diagnosticSeverity(severity),
        agent_engine: args.engine ?? DEFAULT_ENGINE,
      },
      TUI_SCOPE
    );
  }
}

/** Config surfaces that carry the ConfigResource descriptor. */
export type ConfigSurface = 'mcp' | 'steering' | 'hooks' | 'powers';

/**
 * A session observed cloud-sourced configuration on one surface
 * (`kiro_cli_cloud_config_source_total`). Callers dedupe per session per
 * surface, so this counts adopting sessions, not descriptor pushes.
 */
export function recordTuiCloudConfigSource(
  args: { surface: ConfigSurface; version: string; engine?: Engine },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_cloud_config_source_total',
    1,
    {
      version_full: args.version,
      config_surface: args.surface,
      agent_engine: args.engine ?? DEFAULT_ENGINE,
    },
    TUI_SCOPE
  );
}

const WORKFLOW_RUN_DURATION_BOUNDS = [
  1, 5, 10, 30, 60, 120, 300, 600, 1800, 3600, 7200,
];
const WORKFLOW_NODE_DURATION_BOUNDS = [
  1, 2, 5, 10, 30, 60, 120, 300, 600, 1800,
];

export function recordTuiWorkflowObservations(
  observations: readonly WorkflowTelemetryObservation[],
  version: string,
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  const emitCounter = counterFn(deps);
  const emitHistogram = histogramFn(deps);
  const emitGauge = gaugeFn(deps);

  for (const observation of observations) {
    switch (observation.type) {
      case 'run':
        emitCounter(
          'kiro_cli_workflow_run_total',
          1,
          {
            version_full: version,
            workflow_run_event: observation.event,
            workflow_topology: observation.topology,
            workflow_step_bucket: observation.stepBucket,
            agent_engine: DEFAULT_ENGINE,
          },
          TUI_SCOPE
        );
        break;
      case 'run_duration':
        emitHistogram(
          'kiro_cli_workflow_run_duration_seconds',
          observation.durationSeconds,
          {
            version_full: version,
            workflow_outcome: observation.outcome,
            workflow_topology: observation.topology,
            workflow_step_bucket: observation.stepBucket,
            agent_engine: DEFAULT_ENGINE,
          },
          TUI_SCOPE,
          WORKFLOW_RUN_DURATION_BOUNDS
        );
        break;
      case 'node':
        emitCounter(
          'kiro_cli_workflow_node_total',
          1,
          {
            version_full: version,
            workflow_node_type: observation.nodeType,
            workflow_node_outcome: observation.outcome,
            agent_engine: DEFAULT_ENGINE,
          },
          TUI_SCOPE
        );
        break;
      case 'node_duration':
        emitHistogram(
          'kiro_cli_workflow_node_duration_seconds',
          observation.durationSeconds,
          {
            version_full: version,
            workflow_node_type: observation.nodeType,
            workflow_node_outcome: observation.outcome,
            agent_engine: DEFAULT_ENGINE,
          },
          TUI_SCOPE,
          WORKFLOW_NODE_DURATION_BOUNDS
        );
        break;
      case 'concurrent':
        emitGauge(
          'kiro_cli_workflow_concurrent_runs',
          observation.activeRuns,
          { version_full: version, agent_engine: DEFAULT_ENGINE },
          TUI_SCOPE
        );
        break;
      default: {
        const exhaustive: never = observation;
        void exhaustive;
      }
    }
  }
}

export function recordTuiWorkflowControl(
  action: WorkflowControlAction,
  result: WorkflowControlResult,
  version: string,
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  counterFn(deps)(
    'kiro_cli_workflow_control_total',
    1,
    {
      version_full: version,
      workflow_control_action: action,
      workflow_control_result: result,
      agent_engine: DEFAULT_ENGINE,
    },
    TUI_SCOPE
  );
}

export function recordTuiWorkflowRestore(
  result: WorkflowRestoreMetricResult,
  version: string,
  count = 1,
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps) || !Number.isFinite(count) || count <= 0) {
    return;
  }
  counterFn(deps)(
    'kiro_cli_workflow_restore_total',
    count,
    {
      version_full: version,
      workflow_restore_result: result,
      agent_engine: DEFAULT_ENGINE,
    },
    TUI_SCOPE
  );
}

export function recordTuiWorkflowRestoreSummary(
  summary: WorkflowRestoreSummary,
  version: string,
  deps?: TuiTelemetryDeps
): void {
  recordTuiWorkflowRestore('restored', version, summary.restored, deps);
  recordTuiWorkflowRestore(
    'discovery_failed',
    version,
    summary.discovery_failed,
    deps
  );
  recordTuiWorkflowRestore('load_failed', version, summary.load_failed, deps);
  recordTuiWorkflowRestore('rejected', version, summary.rejected, deps);
  recordTuiWorkflowRestore('_other_', version, summary._other_, deps);
}

/**
 * A top-level user turn completed. Failures and cancellations use dedicated
 * counters; only successful turns contribute latency observations.
 */
export function recordTuiUserTurn(
  args: {
    result: 'success' | 'failed' | 'cancelled' | '_other_';
    isSubagent: boolean;
    mode: string;
    version: string;
    failureReason?: TurnFailureReason;
    /**
     * Wall-clock turn duration. Omitted when KAS did not report `elapsedTime`
     * — in that case the latency histogram point is NOT emitted (coercing a
     * missing duration to 0 would pollute the histogram with phantom
     * 0-second turns and bias the percentiles downward). The turn count
     * always fires regardless.
     */
    durationSeconds?: number;
    engine?: Engine;
    logProperties?: MetricLogProperties;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  const engine = args.engine ?? DEFAULT_ENGINE;
  if (args.isSubagent) return;
  const attrs = {
    version_full: args.version,
    session_interface: SESSION_INTERFACE,
    agent_mode: modeFromId(args.mode),
    agent_engine: engine,
  };

  counterFn(deps)(
    'kiro_cli_user_turns',
    1,
    attrs,
    TUI_SCOPE,
    args.logProperties
  );

  if (args.result === 'cancelled') {
    counterFn(deps)(
      'kiro_cli_turn_cancelled_total',
      1,
      attrs,
      TUI_SCOPE,
      args.logProperties
    );
  } else if (args.result === 'failed') {
    counterFn(deps)(
      'kiro_cli_turn_failure_total',
      1,
      { ...attrs, turn_failure_reason: args.failureReason ?? 'unknown' },
      TUI_SCOPE,
      args.logProperties
    );
  }

  if (
    args.result === 'success' &&
    args.durationSeconds !== undefined &&
    Number.isFinite(args.durationSeconds) &&
    args.durationSeconds >= 0
  ) {
    histogramFn(deps)(
      'kiro_cli_user_turn_duration_seconds',
      args.durationSeconds,
      attrs,
      TUI_SCOPE,
      USER_TURN_DURATION_BOUNDS,
      args.logProperties
    );
  }
}

export type TokenType =
  | 'input_uncached'
  | 'input_cache_read'
  | 'output'
  | 'reasoning';

export function recordTuiTokensConsumed(
  args: {
    version: string;
    model: string;
    tokens: Partial<Record<TokenType, number>>;
    engine?: Engine;
    logProperties?: MetricLogProperties;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  const emit = counterFn(deps);
  for (const [tokenType, value] of Object.entries(args.tokens) as Array<
    [TokenType, number | undefined]
  >) {
    if (value === undefined || !Number.isFinite(value) || value <= 0) continue;
    emit(
      'kiro_cli_tokens_consumed',
      value,
      {
        version_full: args.version,
        agent_engine: args.engine ?? DEFAULT_ENGINE,
        model: args.model || 'unknown',
        token_type: tokenType,
      },
      TUI_SCOPE,
      args.logProperties
    );
  }
}

export function recordTuiModelInvocations(
  args: {
    version: string;
    model: string;
    count: number;
    engine?: Engine;
    logProperties?: MetricLogProperties;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  if (!Number.isFinite(args.count) || args.count <= 0) return;
  counterFn(deps)(
    'kiro_cli_model_invocations_total',
    Math.floor(args.count),
    {
      version_full: args.version,
      agent_engine: args.engine ?? DEFAULT_ENGINE,
      model: args.model || 'unknown',
    },
    TUI_SCOPE,
    args.logProperties
  );
}

export function recordTuiCreditsConsumed(
  args: {
    version: string;
    model: string;
    credits: number;
    logProperties?: MetricLogProperties;
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  if (!Number.isFinite(args.credits) || args.credits < 0) return;
  counterFn(deps)(
    'kiro_cli_credits_consumed',
    args.credits,
    {
      version_full: args.version,
      model: args.model || 'unknown',
    },
    TUI_SCOPE,
    args.logProperties
  );
}

/**
 * A tool call finished: the count plus a latency observation when measured.
 */
export function recordTuiToolCall(
  args: ToolTelemetryIdentity & {
    outcome: 'success' | 'error' | 'cancelled' | 'denied';
    executionDurationMs?: number;
    engine?: Engine;
    version: string;
    executionContext?: 'main' | 'subagent';
  },
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  const engine = args.engine ?? DEFAULT_ENGINE;

  const countAttrs: MetricAttributes = {
    ...toolAttrs(args, args.version, engine, args.executionContext ?? 'main'),
    tool_outcome: args.outcome,
  };

  counterFn(deps)('kiro_cli_tool_call_total', 1, countAttrs, TUI_SCOPE);

  // Zero or invalid durations would bias latency percentiles.
  if (
    args.executionDurationMs !== undefined &&
    Number.isFinite(args.executionDurationMs) &&
    args.executionDurationMs > 0
  ) {
    histogramFn(deps)(
      'kiro_cli_tool_execution_duration_ms',
      args.executionDurationMs,
      countAttrs,
      TUI_SCOPE
    );
  }
}

export type ToolTelemetryIdentity =
  | {
      toolOrigin: 'builtin';
      builtinToolName: string;
    }
  | {
      toolOrigin: 'mcp';
      mcpServerName: string;
    }
  | {
      toolOrigin: 'unknown';
    };

export type CanonicalBuiltinToolName =
  | 'fs_read'
  | 'fs_write'
  | 'execute_bash'
  | 'summary'
  | 'grep'
  | 'glob'
  | 'use_aws'
  | 'web_fetch'
  | 'web_search'
  | 'code'
  | 'use_subagent'
  | 'session'
  | 'switch_to_execution'
  | 'introspect'
  | 'knowledge'
  | 'tool_search'
  | 'task'
  | 'goal'
  | 'unknown';

export function canonicalBuiltinToolName(
  value: string | undefined
): CanonicalBuiltinToolName {
  const normalized = (value ?? '').trim().toLowerCase().replace(/-/g, '_');
  switch (normalized) {
    case 'fs_read':
    case 'fsread':
    case 'read':
      return 'fs_read';
    case 'fs_write':
    case 'fswrite':
    case 'write':
      return 'fs_write';
    case 'execute_bash':
    case 'execute_cmd':
    case 'executecmd':
    case 'shell':
      return 'execute_bash';
    case 'summary':
    case 'grep':
    case 'glob':
    case 'code':
    case 'introspect':
    case 'knowledge':
    case 'goal':
      return normalized;
    case 'use_aws':
    case 'aws':
      return 'use_aws';
    case 'web_fetch':
    case 'webfetch':
      return 'web_fetch';
    case 'web_search':
    case 'websearch':
      return 'web_search';
    case 'agent_crew':
    case 'subagent':
    case 'use_subagent':
      return 'use_subagent';
    case 'session':
    case 'session_management':
    case 'sessionmanagement':
    case 'sessions':
      return 'session';
    case 'switch_to_execution':
    case 'switchtoexecution':
      return 'switch_to_execution';
    case 'tool_search':
    case 'toolsearch':
      return 'tool_search';
    case 'task':
    case 'todo':
    case 'todo_list':
      return 'task';
    default:
      return 'unknown';
  }
}

function toolAttrs(
  identity: ToolTelemetryIdentity,
  version: string,
  engine: Engine,
  executionContext: 'main' | 'subagent'
): MetricAttributes {
  const attrs: MetricAttributes = {
    version_full: version,
    tool_origin: identity.toolOrigin,
    agent_engine: engine,
    execution_context: executionContext,
  };
  if (identity.toolOrigin === 'builtin') {
    attrs['builtin_tool_name'] = canonicalBuiltinToolName(
      identity.builtinToolName
    );
  }
  return attrs;
}

export type TuiToolCallStart = ToolTelemetryIdentity & {
  name: string;
  executionContext?: 'main' | 'subagent';
};

/**
 * Correlates ToolCall → ToolCallFinished events (keyed by toolCallId) into tool
 * telemetry, deriving the start-to-finish duration.
 */
export class TuiToolCallObserver {
  private readonly inFlight = new Map<
    string,
    TuiToolCallStart & { startMs: number }
  >();
  private readonly deps?: TuiTelemetryDeps;
  private readonly engine: Engine;
  private readonly version: string;

  constructor(
    version: string,
    deps?: TuiTelemetryDeps,
    engine: Engine = DEFAULT_ENGINE
  ) {
    this.version = version;
    this.deps = deps;
    this.engine = engine;
  }

  start(toolCallId: string, info: TuiToolCallStart): void {
    if (this.inFlight.has(toolCallId)) return;
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
    const toolIdentity: ToolTelemetryIdentity = started ?? {
      toolOrigin: 'unknown',
    };
    const executionDurationMs =
      started !== undefined
        ? Math.max(0, Math.round(performance.now() - started.startMs))
        : undefined;

    recordTuiToolCall(
      {
        ...toolIdentity,
        outcome: args.outcome,
        ...(executionDurationMs !== undefined ? { executionDurationMs } : {}),
        engine: this.engine,
        version: this.version,
        executionContext: started?.executionContext ?? 'main',
      },
      this.deps
    );
  }

  /** Drop all in-flight state (session change / teardown). */
  reset(): void {
    this.inFlight.clear();
  }
}

/**
 * Process role for TUI-sampled process metrics.
 */
const PROCESS_ROLE_TUI = 'tui';

/** MiB to bytes. */
const MIB = 1024 * 1024;
/** Milliseconds to seconds. */
const MS_PER_S = 1000;

/**
 * Histogram bucket bounds (seconds) for the TUI latency histograms
 * (event-loop delay, input latency, render duration). Sized sub-ms to
 * hundreds-of-ms so the raw histogram is re-aggregatable downstream, rather
 * than the pre-reduced p99/p95 the log shipped.
 */
const TUI_LATENCY_BOUNDS_S = [
  0.0005, 0.001, 0.002, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1,
];

function osTypeFromPlatform(platform: string): string {
  switch (platform.toLowerCase()) {
    case 'darwin':
    case 'macos':
      return 'macos';
    case 'win32':
    case 'windows':
      return 'windows';
    case 'linux':
      return 'linux';
    default:
      return 'unknown';
  }
}

export function recordTuiRender(
  args: {
    durationMs: number;
    kind: 'full' | 'partial';
    version: string;
    platform: string;
  },
  engine: Engine = DEFAULT_ENGINE,
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;
  if (!Number.isFinite(args.durationMs) || args.durationMs < 0) return;

  histogramFn(deps)(
    'kiro_cli_tui_render_duration_seconds',
    args.durationMs / MS_PER_S,
    {
      version_full: args.version,
      os_type: osTypeFromPlatform(args.platform),
      render_kind: args.kind,
      agent_engine: engine,
    },
    TUI_SCOPE,
    TUI_LATENCY_BOUNDS_S
  );
}

/**
 * Promote one process-health snapshot to SDK metrics. Each metric carries
 * ONLY its catalog-allowed attribute subset, else endpoint validation drops it.
 * CPU utilization is a histogram, not a gauge, so each sample remains
 * re-aggregatable across clients and time windows.
 */
export function recordTuiProcessHealth(
  snapshot: ProcessHealthSnapshot,
  engine: Engine = DEFAULT_ENGINE,
  deps?: TuiTelemetryDeps
): void {
  if (suppressedInTest(deps)) return;

  const version = snapshot.version;
  const osType = osTypeFromPlatform(snapshot.platform);
  const g = gaugeFn(deps);
  const h = histogramFn(deps);

  g(
    'kiro_cli_process_memory_rss_bytes',
    snapshot.rssMb * MIB,
    {
      version_full: version,
      os_type: osType,
      agent_engine: engine,
      process_role: PROCESS_ROLE_TUI,
    },
    TUI_SCOPE
  );
  h(
    'kiro_cli_process_peak_rss_bytes',
    snapshot.peakRssMb * MIB,
    {
      version_full: version,
      os_type: osType,
      agent_engine: engine,
      process_role: PROCESS_ROLE_TUI,
    },
    TUI_SCOPE
  );
  g(
    'kiro_cli_tui_heap_used_bytes',
    snapshot.heapUsedMb * MIB,
    {
      version_full: version,
      os_type: osType,
      agent_engine: engine,
    },
    TUI_SCOPE
  );
  if (typeof snapshot.openFileDescriptorCount === 'number') {
    g(
      'kiro_cli_process_open_file_descriptor_count',
      snapshot.openFileDescriptorCount,
      {
        version_full: version,
        os_type: osType,
        agent_engine: engine,
        process_role: PROCESS_ROLE_TUI,
      },
      TUI_SCOPE
    );
  }
  if (typeof snapshot.handleCount === 'number') {
    g(
      'kiro_cli_process_handle_count',
      snapshot.handleCount,
      {
        version_full: version,
        agent_engine: engine,
        process_role: PROCESS_ROLE_TUI,
      },
      TUI_SCOPE
    );
  }
  if (typeof snapshot.threadCount === 'number') {
    g(
      'kiro_cli_process_thread_count',
      snapshot.threadCount,
      {
        version_full: version,
        os_type: osType,
        agent_engine: engine,
        process_role: PROCESS_ROLE_TUI,
      },
      TUI_SCOPE
    );
  }

  // CPU: sampler reports user/system as percentages of the elapsed window; the catalog
  // unit is "1" (a fraction), so combine and divide by 100.
  const cpuRatio = (snapshot.cpuUserPct + snapshot.cpuSystemPct) / 100;
  if (Number.isFinite(cpuRatio) && cpuRatio >= 0) {
    h(
      'kiro_cli_process_cpu_utilization_ratio',
      cpuRatio,
      {
        version_full: version,
        os_type: osType,
        agent_engine: engine,
        process_role: PROCESS_ROLE_TUI,
      },
      TUI_SCOPE
    );
  }

  if (snapshot.eventLoopP99Ms !== null && snapshot.eventLoopP99Ms >= 0) {
    h(
      'kiro_cli_tui_event_loop_delay_p99_seconds',
      snapshot.eventLoopP99Ms / MS_PER_S,
      { version_full: version, os_type: osType, agent_engine: engine },
      TUI_SCOPE,
      TUI_LATENCY_BOUNDS_S
    );
  }

  if (snapshot.inputLatencyP95Ms !== null && snapshot.inputLatencyP95Ms >= 0) {
    h(
      'kiro_cli_tui_input_to_render_p95_seconds',
      snapshot.inputLatencyP95Ms / MS_PER_S,
      { version_full: version, os_type: osType, agent_engine: engine },
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

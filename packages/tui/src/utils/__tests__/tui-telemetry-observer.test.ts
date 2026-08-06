import { beforeEach, describe, expect, it, mock } from 'bun:test';
import { AgentEventType, ContentType } from '../../types/agent-events';
import type { WorkflowEvent } from '../../types/workflow.js';
import { WorkflowTelemetryTracker } from '../workflow-telemetry.js';

// @ts-expect-error - query-string import avoids process-global mocks in other suites.
const observerMod = await import('../tui-telemetry-observer?unit');
const {
  DEFAULT_ENGINE,
  TUI_SCOPE,
  TuiFirstVisibleResponseObserver,
  TuiToolCallObserver,
  attachSizeBucket,
  canonicalBuiltinToolName,
  modeFromId,
  recordTuiSlashCommand,
  recordTuiAutonomousMode,
  recordTuiCloudAttach,
  recordTuiCloudError,
  recordTuiCloudRepoAttach,
  recordTuiCloudSession,
  recordTuiCloudSessionReady,
  recordTuiCreditsConsumed,
  recordTuiModelInvocations,
  recordTuiProcessHealth,
  recordTuiRender,
  recordTuiSessionStarted,
  recordTuiTokensConsumed,
  recordTuiToolCall,
  recordTuiUiModeSessionStarted,
  recordTuiUserTurn,
  recordTuiWorkflowControl,
  recordTuiWorkflowObservations,
  recordTuiWorkflowRestoreSummary,
  repoCountBucket,
  resultFromStatus,
  turnFailureReasonFromStatus,
} = observerMod as typeof import('../tui-telemetry-observer');

type Attrs = Record<string, string | number | boolean>;
type MetricCall = {
  name: string;
  value: number;
  attrs?: Attrs;
  scope?: string;
  bounds?: number[];
};

let counterCalls: MetricCall[];
let histogramCalls: MetricCall[];
let gaugeCalls: MetricCall[];

const counter = mock(
  (name: string, value: number, attrs?: Attrs, scope?: string) => {
    counterCalls.push({ name, value, attrs, scope });
  }
);
const histogram = mock(
  (
    name: string,
    value: number,
    attrs?: Attrs,
    scope?: string,
    bounds?: number[]
  ) => {
    histogramCalls.push({ name, value, attrs, scope, bounds });
  }
);
const gauge = mock(
  (name: string, value: number, attrs?: Attrs, scope?: string) => {
    gaugeCalls.push({ name, value, attrs, scope });
  }
);
const deps = {
  counter: counter as never,
  histogram: histogram as never,
  gauge: gauge as never,
};

beforeEach(() => {
  counterCalls = [];
  histogramCalls = [];
  gaugeCalls = [];
});

describe('agent mode normalization', () => {
  it('keeps built-in modes bounded and collapses custom ids', () => {
    expect(modeFromId('default')).toBe('default');
    expect(modeFromId('kiro_default')).toBe('default');
    expect(modeFromId('/quick-plan')).toBe('plan');
    expect(modeFromId('kiro-spec')).toBe('spec');
    expect(modeFromId('autonomous')).toBe('autonomous');
    expect(modeFromId('review')).toBe('custom');
    expect(modeFromId('my-agent')).toBe('custom');
  });
});

describe('session and turn metrics', () => {
  it('records one interactive chat session with reviewed dimensions', () => {
    recordTuiSessionStarted(
      { mode: 'my-agent', version: '2.4.0', engine: 'v2' },
      deps
    );

    expect(counterCalls).toEqual([
      {
        name: 'kiro_cli_chat_session_started_total',
        value: 1,
        attrs: {
          version_full: '2.4.0',
          session_interface: 'interactive_cli',
          agent_mode: 'custom',
          agent_engine: 'v2',
        },
        scope: TUI_SCOPE,
      },
    ]);
  });

  it('records successful top-level turns and their duration', () => {
    recordTuiUserTurn(
      {
        result: 'success',
        isSubagent: false,
        mode: 'plan',
        version: '2.4.0',
        durationSeconds: 3.2,
      },
      deps
    );

    expect(counterCalls.map((call) => call.name)).toEqual([
      'kiro_cli_user_turns',
    ]);
    expect(histogramCalls.map((call) => call.name)).toEqual([
      'kiro_cli_user_turn_duration_seconds',
    ]);
    expect(counterCalls[0]?.attrs).toEqual({
      version_full: '2.4.0',
      session_interface: 'interactive_cli',
      agent_mode: 'plan',
      agent_engine: DEFAULT_ENGINE,
    });
  });

  it('splits failures and cancellations without timing either', () => {
    recordTuiUserTurn(
      {
        result: 'failed',
        isSubagent: false,
        mode: 'interactive',
        version: '2.4.0',
        failureReason: 'model_error',
        durationSeconds: 1,
      },
      deps
    );
    recordTuiUserTurn(
      {
        result: 'cancelled',
        isSubagent: false,
        mode: 'interactive',
        version: '2.4.0',
        durationSeconds: 1,
      },
      deps
    );

    expect(counterCalls.map((call) => call.name)).toEqual([
      'kiro_cli_user_turns',
      'kiro_cli_turn_failure_total',
      'kiro_cli_user_turns',
      'kiro_cli_turn_cancelled_total',
    ]);
    expect(counterCalls[1]?.attrs?.['turn_failure_reason']).toBe('model_error');
    expect(histogramCalls).toHaveLength(0);
  });

  it('records unknown non-empty terminal statuses as failures', () => {
    recordTuiUserTurn(
      {
        result: resultFromStatus('backend:new-status'),
        isSubagent: false,
        mode: 'interactive',
        version: '2.4.0',
        failureReason: turnFailureReasonFromStatus('backend:new-status'),
      },
      deps
    );

    expect(counterCalls.map((call) => call.name)).toEqual([
      'kiro_cli_user_turns',
      'kiro_cli_turn_failure_total',
    ]);
    expect(counterCalls[1]?.attrs?.['turn_failure_reason']).toBe('unknown');
  });

  it('maps missing and known KAS terminal statuses without ambiguity', () => {
    expect(resultFromStatus(undefined)).toBe('_other_');
    expect(turnFailureReasonFromStatus(undefined)).toBeUndefined();
    expect(resultFromStatus(' success ')).toBe('success');
    expect(turnFailureReasonFromStatus('success')).toBeUndefined();
    expect(resultFromStatus('cancelled')).toBe('cancelled');
    expect(turnFailureReasonFromStatus('cancelled')).toBeUndefined();
    expect(resultFromStatus('tool_error')).toBe('failed');
    expect(turnFailureReasonFromStatus('tool_error')).toBe('tool_error');
  });

  it('does not count subagent turns as top-level user turns', () => {
    recordTuiUserTurn(
      {
        result: 'success',
        isSubagent: true,
        mode: 'interactive',
        version: '2.4.0',
      },
      deps
    );
    expect(counterCalls).toHaveLength(0);
  });
});

describe('TUI-owned usage metrics', () => {
  it('records slash commands with version and engine', () => {
    recordTuiSlashCommand(
      { command: '/help', version: '2.4.0', engine: 'v3' },
      deps
    );

    expect(counterCalls).toEqual([
      {
        name: 'kiro_cli_slash_command_invoked_total',
        value: 1,
        attrs: {
          version_full: '2.4.0',
          agent_engine: 'v3',
          command: '/help',
        },
        scope: TUI_SCOPE,
      },
    ]);
  });

  it('bounds slash command names at the emission boundary', () => {
    recordTuiSlashCommand(
      {
        command: '/unreviewed-server-command',
        version: '2.4.0',
        engine: 'v3',
      },
      deps
    );

    expect(counterCalls[0]?.attrs?.['command']).toBe('/custom');
  });

  it('records only bounded UI modes', () => {
    recordTuiUiModeSessionStarted({ mode: 'lite', version: '2.4.0' }, deps);
    recordTuiUiModeSessionStarted(
      { mode: 'future-layout', version: '2.4.0' },
      deps
    );

    expect(counterCalls.map((call) => call.attrs?.['ui_mode'])).toEqual([
      'lite',
      'unknown',
    ]);
  });
});

describe('workflow telemetry', () => {
  const version = '9.8.7-test';

  it('emits each lifecycle instrument with catalog dimensions', () => {
    recordTuiWorkflowObservations(
      [
        {
          type: 'run',
          event: 'started',
          topology: 'mixed',
          stepBucket: '6_10',
        },
        {
          type: 'run_duration',
          durationSeconds: 45,
          outcome: 'completed',
          topology: 'mixed',
          stepBucket: '6_10',
        },
        {
          type: 'node',
          nodeType: 'repeat',
          outcome: 'failed',
        },
        {
          type: 'node_duration',
          durationSeconds: 7,
          nodeType: 'repeat',
          outcome: 'failed',
        },
        { type: 'concurrent', activeRuns: 2 },
      ],
      version,
      deps
    );

    expect(counterCalls.map((call) => call.name)).toEqual([
      'kiro_cli_workflow_run_total',
      'kiro_cli_workflow_node_total',
    ]);
    expect(histogramCalls.map((call) => call.name)).toEqual([
      'kiro_cli_workflow_run_duration_seconds',
      'kiro_cli_workflow_node_duration_seconds',
    ]);
    expect(gaugeCalls).toEqual([
      {
        name: 'kiro_cli_workflow_concurrent_runs',
        value: 2,
        attrs: {
          version_full: version,
          agent_engine: 'v3',
        },
        scope: TUI_SCOPE,
      },
    ]);
    expect(counterCalls[0]?.attrs).toEqual({
      version_full: version,
      workflow_run_event: 'started',
      workflow_topology: 'mixed',
      workflow_step_bucket: '6_10',
      agent_engine: 'v3',
    });
    expect(histogramCalls[0]?.attrs).toEqual({
      version_full: version,
      workflow_outcome: 'completed',
      workflow_topology: 'mixed',
      workflow_step_bucket: '6_10',
      agent_engine: 'v3',
    });
    expect(counterCalls[1]?.attrs).toEqual({
      version_full: version,
      workflow_node_type: 'repeat',
      workflow_node_outcome: 'failed',
      agent_engine: 'v3',
    });
  });

  it('records control and restoration outcomes without identifiers', () => {
    recordTuiWorkflowControl('pause', 'failed', version, deps);
    recordTuiWorkflowRestoreSummary(
      {
        restored: 3,
        discovery_failed: 1,
        load_failed: 2,
        rejected: 4,
        _other_: 0,
      },
      version,
      deps
    );

    expect(counterCalls).toEqual([
      {
        name: 'kiro_cli_workflow_control_total',
        value: 1,
        attrs: {
          version_full: version,
          workflow_control_action: 'pause',
          workflow_control_result: 'failed',
          agent_engine: 'v3',
        },
        scope: TUI_SCOPE,
      },
      {
        name: 'kiro_cli_workflow_restore_total',
        value: 3,
        attrs: {
          version_full: version,
          workflow_restore_result: 'restored',
          agent_engine: 'v3',
        },
        scope: TUI_SCOPE,
      },
      {
        name: 'kiro_cli_workflow_restore_total',
        value: 1,
        attrs: {
          version_full: version,
          workflow_restore_result: 'discovery_failed',
          agent_engine: 'v3',
        },
        scope: TUI_SCOPE,
      },
      {
        name: 'kiro_cli_workflow_restore_total',
        value: 2,
        attrs: {
          version_full: version,
          workflow_restore_result: 'load_failed',
          agent_engine: 'v3',
        },
        scope: TUI_SCOPE,
      },
      {
        name: 'kiro_cli_workflow_restore_total',
        value: 4,
        attrs: {
          version_full: version,
          workflow_restore_result: 'rejected',
          agent_engine: 'v3',
        },
        scope: TUI_SCOPE,
      },
    ]);
  });

  it('emits a live lifecycle once and suppresses persisted history', () => {
    const tracker = new WorkflowTelemetryTracker();
    const event = {
      type: 'run_start',
      workflowId: 'private-workflow-id',
      workflowName: 'private-workflow-name',
      inputs: { private: 'value' },
      nodeTree: [{ nodeId: 'private-node-id', type: 'step' }],
    } as const satisfies WorkflowEvent;

    recordTuiWorkflowObservations(tracker.observe(event, false), version, deps);
    expect(counterCalls).toEqual([]);
    expect(gaugeCalls).toEqual([]);

    recordTuiWorkflowObservations(tracker.observe(event, true), version, deps);
    recordTuiWorkflowObservations(tracker.observe(event, true), version, deps);

    expect(counterCalls).toHaveLength(1);
    expect(counterCalls[0]).toMatchObject({
      name: 'kiro_cli_workflow_run_total',
      value: 1,
      attrs: {
        version_full: version,
        workflow_run_event: 'started',
        workflow_topology: 'sequential',
        workflow_step_bucket: '1',
        agent_engine: 'v3',
      },
    });
    expect(gaugeCalls).toHaveLength(1);
  });
});

describe('first visible response', () => {
  it('records only the first meaningful event for a prompt', () => {
    const observer = new TuiFirstVisibleResponseObserver(deps);
    observer.start({ mode: 'plan', version: '2.4.0', engine: 'v2' }, 100);
    observer.observe(
      {
        type: AgentEventType.Content,
        id: 'empty',
        content: { type: ContentType.Text, text: '' },
      },
      125
    );
    observer.observe(
      {
        type: AgentEventType.ToolCall,
        id: 'tool',
        name: 'fs_read',
        args: {},
      } as never,
      350
    );
    observer.observe(
      {
        type: AgentEventType.Content,
        id: 'late',
        content: { type: ContentType.Text, text: 'done' },
      },
      500
    );

    expect(histogramCalls).toHaveLength(1);
    expect(histogramCalls[0]).toMatchObject({
      name: 'kiro_cli_time_to_first_visible_response_ms',
      value: 250,
      attrs: {
        version_full: '2.4.0',
        session_interface: 'interactive_cli',
        agent_mode: 'plan',
        agent_engine: 'v2',
      },
    });
  });
});

describe('KAS backend metrics', () => {
  it('preserves the canonical model id and records canonical token types', () => {
    const model = 'us.anthropic.claude-sonnet-4-20250514-v1:0';
    recordTuiModelInvocations({ version: '2.4.0', model, count: 3 }, deps);
    recordTuiTokensConsumed(
      {
        version: '2.4.0',
        model,
        tokens: {
          input_uncached: 1200,
          input_cache_read: 300,
          output: 128,
        },
      },
      deps
    );

    expect(counterCalls[0]).toMatchObject({
      name: 'kiro_cli_model_invocations_total',
      value: 3,
      attrs: {
        version_full: '2.4.0',
        agent_engine: 'v3',
        model,
      },
    });
    expect(
      counterCalls.slice(1).map((call) => call.attrs?.['token_type'])
    ).toEqual(['input_uncached', 'input_cache_read', 'output']);
  });

  it('records only finite non-negative credits', () => {
    recordTuiCreditsConsumed(
      { version: '2.4.0', model: 'claude-sonnet-4', credits: 1.5 },
      deps
    );
    recordTuiCreditsConsumed(
      { version: '2.4.0', model: 'claude-sonnet-4', credits: -1 },
      deps
    );
    expect(counterCalls).toHaveLength(1);
    expect(counterCalls[0]?.name).toBe('kiro_cli_credits_consumed');
  });
});

describe('tool metrics', () => {
  it('uses reviewed dimensions and excludes MCP server identity', () => {
    recordTuiToolCall(
      {
        toolOrigin: 'mcp',
        mcpServerName: 'private-server',
        outcome: 'error',
        executionDurationMs: 42,
        version: '2.4.0',
        executionContext: 'subagent',
      },
      deps
    );

    expect(counterCalls[0]?.attrs).toEqual({
      version_full: '2.4.0',
      tool_origin: 'mcp',
      agent_engine: 'v3',
      execution_context: 'subagent',
      tool_outcome: 'error',
    });
    expect(histogramCalls[0]?.attrs).toEqual(counterCalls[0]?.attrs);
  });

  it('counts delegation through the canonical tool counter only', () => {
    const observer = new TuiToolCallObserver('2.4.0', deps);
    observer.start('call-1', {
      name: 'reviewer',
      toolOrigin: 'builtin',
      builtinToolName: 'use_subagent',
    });
    observer.finish('call-1', {
      outcome: 'success',
      model: 'claude-sonnet-4',
    });

    expect(counterCalls.map((call) => call.name)).toEqual([
      'kiro_cli_tool_call_total',
    ]);
    expect(counterCalls[0]?.attrs).toMatchObject({
      tool_origin: 'builtin',
      builtin_tool_name: 'use_subagent',
    });
  });

  it('normalizes built-in aliases and bounds unmatched names', () => {
    expect(canonicalBuiltinToolName('execute-cmd')).toBe('execute_bash');
    expect(canonicalBuiltinToolName('agent_crew')).toBe('use_subagent');
    expect(canonicalBuiltinToolName('todo_list')).toBe('task');
    expect(canonicalBuiltinToolName('new_tool_from_server')).toBe('unknown');
  });
});

describe('cloud metrics', () => {
  it('uses lifecycle names and version-only latency dimensions', () => {
    recordTuiCloudSession({ event: 'created', version: '2.4.0' }, deps);
    recordTuiCloudSessionReady(
      { durationSeconds: 12.5, version: '2.4.0' },
      deps
    );

    expect(counterCalls.map((call) => call.name)).toEqual([
      'kiro_cli_cloud_session_lifecycle_total',
      'kiro_cli_cloud_session_lifecycle_total',
    ]);
    expect(histogramCalls[0]).toMatchObject({
      name: 'kiro_cli_cloud_session_ready_seconds',
      attrs: { version_full: '2.4.0' },
    });
  });

  it('records cloud and autonomous events with reviewed dimensions', () => {
    recordTuiAutonomousMode({ event: 'reverted', version: '2.4.0' }, deps);
    recordTuiCloudError(
      { op: 'session_new', kind: 'version_skew', version: '2.4.0' },
      deps
    );
    recordTuiCloudAttach(
      { kind: 'image', sizeBytes: 200 * 1024, version: '2.4.0' },
      deps
    );
    recordTuiCloudRepoAttach(
      { event: 'submitted', repoCount: 3, version: '2.4.0' },
      deps
    );

    expect(counterCalls.map((call) => call.name)).toEqual([
      'kiro_cli_autonomous_mode_total',
      'kiro_cli_cloud_error_total',
      'kiro_cli_cloud_attach_total',
      'kiro_cli_cloud_repo_attach_total',
    ]);
    for (const call of counterCalls) {
      expect(call.attrs?.['version_full']).toBe('2.4.0');
      expect(call.attrs?.['agent_engine']).toBe('v3');
      expect(call.attrs?.['engine']).toBeUndefined();
    }
    expect(counterCalls[0]?.attrs?.['autonomous_event']).toBe('reverted');
    expect(counterCalls[1]?.attrs).toMatchObject({
      cloud_op: 'session_new',
      cloud_error_kind: 'version_skew',
    });
    expect(counterCalls[2]?.attrs).toMatchObject({
      attach_kind: 'image',
      attach_size_bucket: 'under_1m',
    });
    expect(counterCalls[3]?.attrs).toMatchObject({
      repo_attach_event: 'submitted',
      repo_count_bucket: '3_5',
    });
  });

  it('uses bounded attachment-size and repository-count buckets', () => {
    expect(attachSizeBucket(64 * 1024 - 1)).toBe('under_64k');
    expect(attachSizeBucket(64 * 1024)).toBe('under_1m');
    expect(attachSizeBucket(5 * 1024 * 1024)).toBe('over_5m');
    expect(repoCountBucket(undefined)).toBe('none');
    expect(repoCountBucket(2)).toBe('2');
    expect(repoCountBucket(6)).toBe('6_plus');
  });
});

describe('TUI process metrics', () => {
  it('emits the renamed instruments with OS and version dimensions', () => {
    recordTuiProcessHealth(
      {
        rssMb: 10,
        heapUsedMb: 4,
        peakRssMb: 12,
        openFileDescriptorCount: 9,
        handleCount: null,
        threadCount: 7,
        cpuUserPct: 1,
        cpuSystemPct: 2,
        lastRenderMs: 5,
        maxRenderMs: 7,
        rendersPerMin: 1,
        fullRedrawsPerMin: 1,
        yogaNodeCount: 1,
        eventLoopP99Ms: 2,
        inputLatencyP95Ms: 3,
        sessionDurationSec: 30,
        cpuCores: 8,
        totalMemoryMb: 16384,
        terminal: 'unknown',
        sessionId: null,
        version: '9.8.7-test',
        platform: 'darwin',
      },
      'v2',
      deps
    );

    expect(gaugeCalls.map((call) => call.name)).toEqual([
      'kiro_cli_process_memory_rss_bytes',
      'kiro_cli_tui_heap_used_bytes',
      'kiro_cli_process_open_file_descriptor_count',
      'kiro_cli_process_thread_count',
    ]);
    expect(histogramCalls.map((call) => call.name)).toEqual([
      'kiro_cli_process_peak_rss_bytes',
      'kiro_cli_process_cpu_utilization_ratio',
      'kiro_cli_tui_event_loop_delay_p99_seconds',
      'kiro_cli_tui_input_to_render_p95_seconds',
    ]);
    expect(gaugeCalls[0]?.attrs?.['os_type']).toBe('macos');
  });

  it('records each render with its own duration and kind', () => {
    recordTuiRender(
      {
        durationMs: 2.5,
        kind: 'partial',
        version: '9.8.7-test',
        platform: 'darwin',
      },
      'v3',
      deps
    );
    recordTuiRender(
      {
        durationMs: 8,
        kind: 'full',
        version: '9.8.7-test',
        platform: 'darwin',
      },
      'v3',
      deps
    );

    expect(histogramCalls).toEqual([
      expect.objectContaining({
        name: 'kiro_cli_tui_render_duration_seconds',
        value: 0.0025,
        attrs: {
          version_full: '9.8.7-test',
          os_type: 'macos',
          render_kind: 'partial',
          agent_engine: 'v3',
        },
      }),
      expect.objectContaining({
        name: 'kiro_cli_tui_render_duration_seconds',
        value: 0.008,
        attrs: {
          version_full: '9.8.7-test',
          os_type: 'macos',
          render_kind: 'full',
          agent_engine: 'v3',
        },
      }),
    ]);
  });
});

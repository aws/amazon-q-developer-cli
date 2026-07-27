/**
 * Unit tests for the TUI client-side telemetry observer.
 *
 * These lock the catalog contract: each recordTui* helper records the exact
 * metric name(s), attribute keys, scope (`kiro.tui`), and — crucially — a
 * first-class `engine` on EVERY metric (the canonical v2/v3 discriminator,
 * telemetry-metric-inventory.md §C3/§D); these v3-default cases assert
 * engine=v3, and a dedicated block asserts the engine=v2 path. The observer
 * drives the OTel JS SDK via the `meter.ts` wrapper, so the metric transports
 * (counter / gauge / histogram) are injected as spies. TUI telemetry is
 * metrics-only — KUTS has no OTLP logs path.
 */

import { beforeEach, describe, expect, it, mock } from 'bun:test';

// Import via a query string so this suite always gets the REAL module, even
// when another test file (kas-acp-client) registers a process-global
// `mock.module('../utils/tui-telemetry-observer', …)` to spy on the wiring.
// Bun's module mocks are global; the query string sidesteps the override.
// @ts-expect-error - Bun query-string import isolates this module instance.
const observerMod = await import('../tui-telemetry-observer?unit');
const {
  recordTuiSessionStarted,
  recordTuiToolCall,
  recordTuiUserTurn,
  recordTuiTokensConsumed,
  recordTuiModelInvocation,
  recordTuiTurnOutcome,
  recordTuiToolExecutionDuration,
  recordTuiContextUsage,
  recordTuiModeActive,
  recordTuiSubagentDelegation,
  recordTuiCloudSession,
  recordTuiCloudSessionReady,
  recordTuiAutonomousMode,
  recordTuiCloudRepoAttach,
  repoCountBucket,
  recordTuiProcessHealth,
  modeFromId,
  TuiToolCallObserver,
  DEFAULT_ENGINE,
  TUI_SCOPE,
} = observerMod as typeof import('../tui-telemetry-observer');

type Attrs = Record<string, string | number | boolean>;
type CounterCall = {
  name: string;
  value: number;
  attrs?: Attrs;
  scope?: string;
};
type HistogramCall = {
  name: string;
  value: number;
  attrs?: Attrs;
  scope?: string;
  bounds?: number[];
};
type GaugeCall = { name: string; value: number; attrs?: Attrs; scope?: string };

let counterCalls: CounterCall[];
let histogramCalls: HistogramCall[];
let gaugeCalls: GaugeCall[];

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
  counter.mockClear();
  histogram.mockClear();
  gauge.mockClear();
});

/** Every metric the observer emits MUST carry engine=v3 on the v3 scope. */
function expectEngineV3(call: { attrs?: Attrs; scope?: string }): void {
  expect(call.attrs?.['engine']).toBe(DEFAULT_ENGINE);
  expect(call.scope).toBe(TUI_SCOPE);
  // The old hardcoded client_application is gone from the TUI contract.
  expect(call.attrs?.['client_application']).toBeUndefined();
}

/** The observer now also serves the v2 engine: same scope, engine=v2. */
function expectEngineV2(call: { attrs?: Attrs; scope?: string }): void {
  expect(call.attrs?.['engine']).toBe('v2');
  expect(call.scope).toBe(TUI_SCOPE);
  expect(call.attrs?.['client_application']).toBeUndefined();
}

describe('modeFromId', () => {
  it('normalizes known aliases and passes unknown ids through verbatim (mirrors Rust Mode::from_name)', () => {
    // default/kiro/vibe and empty all collapse to interactive.
    expect(modeFromId('default')).toBe('interactive');
    expect(modeFromId('vibe')).toBe('interactive');
    expect(modeFromId(undefined)).toBe('interactive');
    expect(modeFromId('')).toBe('interactive');
    // planner aliases → plan.
    expect(modeFromId('kiro_planner')).toBe('plan');
    expect(modeFromId('quick_plan')).toBe('plan');
    expect(modeFromId('plan')).toBe('plan');
    // case / slash / dash insensitivity.
    expect(modeFromId('/Plan')).toBe('plan');
    expect(modeFromId('generate-agent')).toBe('generate_agent');
    // straight-through enum members.
    expect(modeFromId('oneshot')).toBe('oneshot');
    expect(modeFromId('review')).toBe('review');
    expect(modeFromId('spec')).toBe('spec');
    expect(modeFromId('My-Custom-Agent')).toBe('my_custom_agent');
  });
});

describe('recordTuiSessionStarted', () => {
  it('emits kiro_cli_chat_session_started_total with engine=v3 on the v3 scope', () => {
    recordTuiSessionStarted({ mode: 'interactive', version: '2.4.0' }, deps);
    expect(counterCalls).toHaveLength(1);
    const c = counterCalls[0]!;
    expect(c.name).toBe('kiro_cli_chat_session_started_total');
    expect(c.value).toBe(1);
    expectEngineV3(c);
    expect(c.attrs?.['mode']).toBe('interactive');
    expect(c.attrs?.['version_full']).toBe('2.4.0');
  });
});

describe('recordTuiUserTurn', () => {
  it('emits the engine-split count AND the latency histogram, both with engine=v3', () => {
    recordTuiUserTurn(
      {
        model: 'claude-sonnet-4',
        result: 'success',
        isSubagent: false,
        mode: 'interactive',
        chatConversationType: 'acp',
        durationSeconds: 3.2,
      },
      deps
    );
    expect(counterCalls).toHaveLength(1);
    expect(histogramCalls).toHaveLength(1);

    const turns = counterCalls[0]!;
    expect(turns.name).toBe('kiro_cli_user_turns');
    expectEngineV3(turns);
    expect(turns.attrs?.['result']).toBe('success');
    expect(turns.attrs?.['is_subagent']).toBe('false');

    const dur = histogramCalls[0]!;
    expect(dur.name).toBe('kiro_cli_user_turn_duration_seconds');
    expect(dur.value).toBe(3.2);
    // engine is now allowed on the histogram (schema change) — assert present.
    expectEngineV3(dur);
    expect(dur.attrs?.['model']).toBe('claude-sonnet-4');
    expect(dur.bounds).toBeDefined();
  });

  it('honors a real is_subagent=true (no longer hardcoded false)', () => {
    recordTuiUserTurn(
      {
        model: 'claude-sonnet-4',
        result: 'success',
        isSubagent: true,
        mode: 'interactive',
        chatConversationType: 'subagent',
      },
      deps
    );
    expect(counterCalls[0]!.attrs?.['is_subagent']).toBe('true');
  });

  it('emits the count but NOT the histogram when duration is omitted', () => {
    recordTuiUserTurn(
      {
        model: 'claude-sonnet-4',
        result: 'success',
        isSubagent: false,
        mode: 'interactive',
        chatConversationType: 'acp',
      },
      deps
    );
    expect(counterCalls).toHaveLength(1);
    expect(histogramCalls).toHaveLength(0);
  });
});

describe('recordTuiToolCall', () => {
  it('emits kiro_cli_tool_call_total + latency histogram (engine=v3); no tool_name log (metrics-only)', () => {
    recordTuiToolCall(
      {
        toolOrigin: 'builtin',
        builtinToolName: 'fs_read',
        outcome: 'success',
        executionDurationMs: 42,
      },
      deps
    );

    expect(counterCalls).toHaveLength(1);
    const m = counterCalls[0]!;
    expect(m.name).toBe('kiro_cli_tool_call_total');
    expectEngineV3(m);
    expect(m.attrs?.['tool_origin']).toBe('builtin');
    expect(m.attrs?.['builtin_tool_name']).toBe('fs_read');
    expect(m.attrs?.['outcome']).toBe('success');

    // Latency histogram emitted inline (§C4): engine-split, no tool_name.
    expect(histogramCalls).toHaveLength(1);
    const h = histogramCalls[0]!;
    expect(h.name).toBe('kiro_cli_tool_execution_duration_ms');
    expect(h.value).toBe(42);
    expectEngineV3(h);
    expect(h.attrs?.['is_success']).toBe('true');

    // tool_name is metric-forbidden and KUTS is metrics-only — no log is
    // emitted, and the high-cardinality tool_name never lands on a metric attr.
    expect(m.attrs?.['tool_name']).toBeUndefined();
    expect(h.attrs?.['tool_name']).toBeUndefined();
  });

  it('omits builtin_tool_name + the latency histogram for non-builtin tools without a duration', () => {
    recordTuiToolCall(
      {
        toolOrigin: 'mcp',
        mcpServerName: 'Local-Server',
        outcome: 'error',
      },
      deps
    );
    const m = counterCalls[0]!;
    expect(m.attrs?.['builtin_tool_name']).toBeUndefined();
    expect(m.attrs?.['tool_origin']).toBe('mcp');
    expect(m.attrs?.['mcp_server_name']).toBe('local-server');
    expect(m.attrs?.['outcome']).toBe('error');
    expect(histogramCalls).toHaveLength(0);
  });

  it('normalizes MCP dimensions once for count and duration metrics', () => {
    const mcpServerName = '  My-Postgres Server  ';
    recordTuiToolCall(
      {
        toolOrigin: 'mcp',
        mcpServerName,
        outcome: 'success',
        executionDurationMs: 42,
      },
      deps
    );
    recordTuiToolExecutionDuration(
      {
        toolOrigin: 'mcp',
        mcpServerName,
        isSuccess: true,
        durationMs: 17,
      },
      deps
    );

    expect(counterCalls[0]!.attrs?.['mcp_server_name']).toBe(
      'my-postgres server'
    );
    expect(histogramCalls).toHaveLength(2);
    for (const call of histogramCalls) {
      expect(call.attrs?.['mcp_server_name']).toBe('my-postgres server');
    }
  });
});

describe('recordTuiTokensConsumed', () => {
  it('emits one counter per non-zero token kind, each with engine=v3', () => {
    recordTuiTokensConsumed(
      {
        model: 'claude-sonnet-4',
        isSubagent: false,
        tokens: {
          input_uncached: 1200,
          input_cache_read: 300,
          input_cache_write: 0, // skipped
          output: 128,
        },
      },
      deps
    );
    expect(counterCalls).toHaveLength(3);
    for (const c of counterCalls) {
      expect(c.name).toBe('kiro_cli_tokens_consumed');
      expectEngineV3(c);
      expect(c.attrs?.['model']).toBe('claude-sonnet-4');
      expect(c.attrs?.['is_subagent']).toBe('false');
    }
    const byType = new Map(
      counterCalls.map((c) => [c.attrs?.['token_type'], c.value])
    );
    expect(byType.get('input_uncached')).toBe(1200);
    expect(byType.get('input_cache_read')).toBe(300);
    expect(byType.get('output')).toBe(128);
    expect(byType.has('input_cache_write')).toBe(false);
  });
});

describe('recordTuiModelInvocation', () => {
  it('emits kiro_cli_model_invocations_total with engine=v3', () => {
    recordTuiModelInvocation({ model: 'claude-sonnet-4' }, deps);
    expect(counterCalls).toHaveLength(1);
    const c = counterCalls[0]!;
    expect(c.name).toBe('kiro_cli_model_invocations_total');
    expectEngineV3(c);
    expect(c.attrs?.['model']).toBe('claude-sonnet-4');
  });
});

describe('recordTuiTurnOutcome', () => {
  it('emits kiro_cli_turn_outcome_total only for non-success, bucketing the reason', () => {
    recordTuiTurnOutcome(
      {
        status: 'interrupted',
        model: 'claude-sonnet-4',
        mode: 'interactive',
      },
      deps
    );
    expect(counterCalls).toHaveLength(1);
    const c = counterCalls[0]!;
    expect(c.name).toBe('kiro_cli_turn_outcome_total');
    expectEngineV3(c);
    expect(c.attrs?.['turn_outcome_reason']).toBe('interrupted');
    expect(c.attrs?.['mode']).toBe('interactive');

    // success status → no counter.
    recordTuiTurnOutcome(
      {
        status: 'completed',
        model: 'claude-sonnet-4',
        mode: 'interactive',
      },
      deps
    );
    expect(counterCalls).toHaveLength(1);
  });

  it('suppresses the counter for an undefined status (the v2 end_turn input)', () => {
    // The V2 turn-completion path maps a successful `end_turn` to status
    // undefined; this must be treated as success (no counter), NOT bucketed as
    // an `_other_` failure. Regression guard for the failure-only contract.
    recordTuiTurnOutcome(
      {
        status: undefined,
        model: 'claude-sonnet-4',
        mode: 'interactive',
        engine: 'v2',
      },
      deps
    );
    expect(counterCalls).toHaveLength(0);
  });
});

describe('recordTuiToolExecutionDuration', () => {
  it('emits kiro_cli_tool_execution_duration_ms with engine=v3; skips non-positive', () => {
    recordTuiToolExecutionDuration(
      {
        toolOrigin: 'builtin',
        builtinToolName: 'fs_read',
        isSuccess: true,
        durationMs: 17,
      },
      deps
    );
    expect(histogramCalls).toHaveLength(1);
    const h = histogramCalls[0]!;
    expect(h.name).toBe('kiro_cli_tool_execution_duration_ms');
    expect(h.value).toBe(17);
    expectEngineV3(h);
    expect(h.attrs?.['is_success']).toBe('true');

    recordTuiToolExecutionDuration(
      {
        toolOrigin: 'builtin',
        builtinToolName: 'fs_read',
        isSuccess: true,
        durationMs: 0,
      },
      deps
    );
    expect(histogramCalls).toHaveLength(1); // unchanged
  });
});

describe('recordTuiContextUsage (gauge)', () => {
  it('emits kiro_cli_context_usage_percentage as a gauge with engine=v3', () => {
    recordTuiContextUsage(
      { model: 'claude-sonnet-4', isSubagent: false, percentage: 73.5 },
      deps
    );
    expect(gaugeCalls).toHaveLength(1);
    const g = gaugeCalls[0]!;
    expect(g.name).toBe('kiro_cli_context_usage_percentage');
    expect(g.value).toBe(73.5);
    expectEngineV3(g);
    expect(g.attrs?.['is_subagent']).toBe('false');
  });
});

describe('recordTuiModeActive', () => {
  it('emits kiro_cli_mode_active_total with engine=v3', () => {
    recordTuiModeActive({ mode: 'plan' }, deps);
    expect(counterCalls).toHaveLength(1);
    const c = counterCalls[0]!;
    expect(c.name).toBe('kiro_cli_mode_active_total');
    expectEngineV3(c);
    expect(c.attrs?.['mode']).toBe('plan');
  });
});

describe('recordTuiSubagentDelegation', () => {
  it('emits kiro_cli_subagent_delegations_total with the normalized name, engine=v3', () => {
    recordTuiSubagentDelegation(
      { subagentName: 'code-review', model: 'claude-sonnet-4' },
      deps
    );
    expect(counterCalls).toHaveLength(1);
    const c = counterCalls[0]!;
    expect(c.name).toBe('kiro_cli_subagent_delegations_total');
    expectEngineV3(c);
    expect(c.attrs?.['subagent_name_class']).toBe('code_review');

    recordTuiSubagentDelegation(
      { subagentName: 'some-random-agent', model: 'claude-sonnet-4' },
      deps
    );
    expect(counterCalls[1]!.attrs?.['subagent_name_class']).toBe(
      'some_random_agent'
    );

    recordTuiSubagentDelegation(
      { subagentName: undefined, model: 'claude-sonnet-4' },
      deps
    );
    expect(counterCalls[2]!.attrs?.['subagent_name_class']).toBe('_other_');
  });
});

describe('TuiToolCallObserver', () => {
  it('start→finish emits kiro_cli_tool_call_total for a builtin', () => {
    const obs = new TuiToolCallObserver(deps);
    obs.start('call-1', {
      name: 'fs_read',
      toolOrigin: 'builtin',
      builtinToolName: 'fs_read',
    });
    obs.finish('call-1', {
      outcome: 'success',
      model: 'claude-sonnet-4',
    });

    expect(counterCalls).toHaveLength(1);
    const c = counterCalls[0]!;
    expect(c.name).toBe('kiro_cli_tool_call_total');
    expectEngineV3(c);
    expect(c.attrs?.['tool_origin']).toBe('builtin');
    expect(c.attrs?.['outcome']).toBe('success');
    expect(c.attrs?.['builtin_tool_name']).toBe('fs_read');
  });

  it('keeps the original identity when the same call is started twice', () => {
    const obs = new TuiToolCallObserver(deps);
    obs.start('anchor', {
      name: 'orchestrate_subagent',
      toolOrigin: 'subagent_delegate',
    });
    obs.start('anchor', {
      name: 'replacement',
      toolOrigin: 'builtin',
      builtinToolName: 'replacement',
    });
    obs.finish('anchor', {
      outcome: 'success',
      model: 'claude-sonnet-4',
    });

    expect(
      counterCalls.find((call) => call.name === 'kiro_cli_tool_call_total')
        ?.attrs?.['tool_origin']
    ).toBe('subagent_delegate');
    expect(
      counterCalls.find(
        (call) => call.name === 'kiro_cli_subagent_delegations_total'
      )
    ).toBeDefined();
  });

  it('emits the delegation counter when the origin is subagent_delegate', () => {
    const obs = new TuiToolCallObserver(deps);
    obs.start('call-2', {
      name: 'code-review',
      toolOrigin: 'subagent_delegate',
    });
    obs.finish('call-2', {
      outcome: 'success',
      model: 'claude-sonnet-4',
    });

    const names = counterCalls.map((c) => c.name);
    expect(names).toContain('kiro_cli_tool_call_total');
    expect(names).toContain('kiro_cli_subagent_delegations_total');
    const deleg = counterCalls.find(
      (c) => c.name === 'kiro_cli_subagent_delegations_total'
    )!;
    expect(deleg.attrs?.['subagent_name_class']).toBe('code_review');
  });

  it('requires and emits mcp_server_name for MCP tool starts', () => {
    const obs = new TuiToolCallObserver(deps);
    obs.start('call-mcp', {
      name: 'query_db',
      toolOrigin: 'mcp',
      mcpServerName: 'Local-Server',
    });
    obs.finish('call-mcp', {
      outcome: 'success',
      model: 'claude-sonnet-4',
    });

    expect(counterCalls).toHaveLength(1);
    const c = counterCalls[0]!;
    expect(c.attrs?.['tool_origin']).toBe('mcp');
    expect(c.attrs?.['mcp_server_name']).toBe('local-server');
    expect(c.attrs?.['builtin_tool_name']).toBeUndefined();
  });

  it('falls back to builtin/unknown for a finish with no matching start', () => {
    const obs = new TuiToolCallObserver(deps);
    obs.finish('orphan', { outcome: 'error', model: 'claude-opus-4' });

    expect(counterCalls).toHaveLength(1);
    const c = counterCalls[0]!;
    expect(c.attrs?.['tool_origin']).toBe('builtin');
    expect(c.attrs?.['outcome']).toBe('error');
    expect(c.attrs?.['builtin_tool_name']).toBe('unknown');
  });

  it('reset() drops in-flight starts so a later finish is unmatched', () => {
    const obs = new TuiToolCallObserver(deps);
    obs.start('call-3', {
      name: 'fs_read',
      toolOrigin: 'builtin',
      builtinToolName: 'fs_read',
    });
    obs.reset();
    obs.finish('call-3', {
      outcome: 'success',
      model: 'claude-sonnet-4',
    });
    // Unmatched → unknown (the started fs_read name was dropped by reset).
    expect(counterCalls[0]!.attrs?.['builtin_tool_name']).toBe('unknown');
  });
});

describe('recordTuiProcessHealth', () => {
  it('uses the snapshot version as version_full', () => {
    recordTuiProcessHealth(
      {
        rssMb: 10,
        heapUsedMb: 4,
        peakRssMb: 12,
        cpuUserPct: 1,
        cpuSystemPct: 2,
        lastRenderMs: 0,
        maxRenderMs: 0,
        rendersPerMin: 0,
        fullRedrawsPerMin: 0,
        yogaNodeCount: 0,
        eventLoopP99Ms: null,
        inputLatencyP95Ms: null,
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

    const rss = gaugeCalls.find(
      (call) => call.name === 'kiro_cli.process.memory.rss'
    )!;
    expect(rss.attrs?.['version_full']).toBe('9.8.7-test');
    expect(rss.attrs?.['engine']).toBe('v2');
    const cpu = histogramCalls.find(
      (call) => call.name === 'kiro_cli.process.cpu.utilization'
    )!;
    expect(cpu.attrs?.['version_full']).toBe('9.8.7-test');
  });
});

describe('engine parameterization (v2 path, §H.4)', () => {
  it('stamps engine=v2 when passed explicitly, defaults to v3 otherwise', () => {
    // Explicit v3 still works (the default).
    recordTuiSessionStarted(
      { mode: 'interactive', version: '2.4.0', engine: 'v3' },
      deps
    );
    expectEngineV3(counterCalls[0]!);

    // engine:'v2' flips the discriminator, same metric/scope.
    recordTuiSessionStarted(
      { mode: 'interactive', version: '2.4.0', engine: 'v2' },
      deps
    );
    const v2 = counterCalls[1]!;
    expect(v2.name).toBe('kiro_cli_chat_session_started_total');
    expectEngineV2(v2);
    expect(v2.attrs?.['mode']).toBe('interactive');
  });

  it('threads engine=v2 through recordTuiUserTurn (count + histogram)', () => {
    recordTuiUserTurn(
      {
        model: 'claude-sonnet-4',
        result: 'success',
        isSubagent: false,
        mode: 'interactive',
        chatConversationType: 'acp',
        durationSeconds: 2.5,
        engine: 'v2',
      },
      deps
    );
    expectEngineV2(counterCalls[0]!);
    expectEngineV2(histogramCalls[0]!);
  });

  it('threads engine=v2 through recordTuiModelInvocation, ModeActive, TurnOutcome', () => {
    recordTuiModelInvocation({ model: 'claude-sonnet-4', engine: 'v2' }, deps);
    recordTuiModeActive({ mode: 'plan', engine: 'v2' }, deps);
    recordTuiTurnOutcome(
      {
        status: 'interrupted',
        model: 'claude-sonnet-4',
        mode: 'interactive',
        engine: 'v2',
      },
      deps
    );
    expect(counterCalls).toHaveLength(3);
    for (const c of counterCalls) expectEngineV2(c);
  });

  it('TuiToolCallObserver stamps the engine it was constructed with', () => {
    const obs = new TuiToolCallObserver(deps, 'v2');
    obs.start('call-v2', {
      name: 'fs_read',
      toolOrigin: 'builtin',
      builtinToolName: 'fs_read',
    });
    obs.finish('call-v2', {
      outcome: 'success',
      model: 'claude-sonnet-4',
    });
    const c = counterCalls.find((x) => x.name === 'kiro_cli_tool_call_total')!;
    expectEngineV2(c);
  });
});

describe('KIRO_TEST_MODE suppression', () => {
  it('short-circuits with no transport when KIRO_TEST_MODE=true and no deps injected', () => {
    const prev = process.env['KIRO_TEST_MODE'];
    process.env['KIRO_TEST_MODE'] = 'true';
    try {
      recordTuiSessionStarted({
        mode: 'interactive',
        version: '2.4.0',
      });
      // No injected deps → real transports would have been used, but the
      // guard suppressed them. (counterCalls only tracks the injected spy.)
      expect(counterCalls).toHaveLength(0);
    } finally {
      if (prev === undefined) delete process.env['KIRO_TEST_MODE'];
      else process.env['KIRO_TEST_MODE'] = prev;
    }
  });
});

describe('recordTuiCloudSession', () => {
  it('emits kiro_cli_cloud_session_total with the event + engine=v3', () => {
    recordTuiCloudSession({ event: 'started' }, deps);
    expect(counterCalls).toHaveLength(1);
    const c = counterCalls[0]!;
    expect(c.name).toBe('kiro_cli_cloud_session_total');
    expect(c.value).toBe(1);
    expectEngineV3(c);
    expect(c.attrs?.['cloud_event']).toBe('started');
  });

  it('carries each lifecycle event through cloud_event', () => {
    recordTuiCloudSession({ event: 'start_failed' }, deps);
    recordTuiCloudSession({ event: 'reattached' }, deps);
    recordTuiCloudSession({ event: 'detached' }, deps);
    recordTuiCloudSession({ event: 'turned_off' }, deps);
    recordTuiCloudSession({ event: 'fell_back_local' }, deps);
    expect(counterCalls.map((c) => c.attrs?.['cloud_event'])).toEqual([
      'start_failed',
      'reattached',
      'detached',
      'turned_off',
      'fell_back_local',
    ]);
  });
});

describe('recordTuiCloudSessionReady', () => {
  it('emits the ready counter AND the latency histogram, both engine=v3', () => {
    recordTuiCloudSessionReady({ durationSeconds: 12.5 }, deps);
    expect(counterCalls).toHaveLength(1);
    const c = counterCalls[0]!;
    expect(c.name).toBe('kiro_cli_cloud_session_total');
    expect(c.attrs?.['cloud_event']).toBe('ready');
    expectEngineV3(c);

    expect(histogramCalls).toHaveLength(1);
    const h = histogramCalls[0]!;
    expect(h.name).toBe('kiro_cli_cloud_session_ready_seconds');
    expect(h.value).toBe(12.5);
    expect(h.bounds).toBeDefined();
    expectEngineV3(h);
  });

  it('emits the ready counter but NOT the histogram for a non-positive duration', () => {
    recordTuiCloudSessionReady({ durationSeconds: 0 }, deps);
    expect(counterCalls).toHaveLength(1);
    expect(histogramCalls).toHaveLength(0);
  });
});

describe('recordTuiAutonomousMode', () => {
  it('emits kiro_cli_autonomous_mode_total with the event + engine=v3', () => {
    recordTuiAutonomousMode({ event: 'enabled' }, deps);
    expect(counterCalls).toHaveLength(1);
    const c = counterCalls[0]!;
    expect(c.name).toBe('kiro_cli_autonomous_mode_total');
    expect(c.value).toBe(1);
    expectEngineV3(c);
    expect(c.attrs?.['autonomous_event']).toBe('enabled');
  });

  it('carries each lifecycle event through autonomous_event', () => {
    recordTuiAutonomousMode({ event: 'disabled' }, deps);
    recordTuiAutonomousMode({ event: 'switch_failed' }, deps);
    recordTuiAutonomousMode({ event: 'reverted' }, deps);
    expect(counterCalls.map((c) => c.attrs?.['autonomous_event'])).toEqual([
      'disabled',
      'switch_failed',
      'reverted',
    ]);
  });
});

describe('repoCountBucket', () => {
  it('buckets counts into the bounded enum', () => {
    expect(repoCountBucket(0)).toBe('none');
    expect(repoCountBucket(undefined)).toBe('none');
    expect(repoCountBucket(1)).toBe('1');
    expect(repoCountBucket(2)).toBe('2');
    expect(repoCountBucket(4)).toBe('3_5');
    expect(repoCountBucket(5)).toBe('3_5');
    expect(repoCountBucket(9)).toBe('6_plus');
  });
});

describe('recordTuiCloudRepoAttach', () => {
  it('emits opened with repo_count_bucket=none', () => {
    recordTuiCloudRepoAttach({ event: 'opened' }, deps);
    const c = counterCalls[0]!;
    expect(c.name).toBe('kiro_cli_cloud_repo_attach_total');
    expect(c.attrs?.['repo_attach_event']).toBe('opened');
    expect(c.attrs?.['repo_count_bucket']).toBe('none');
    expectEngineV3(c);
  });

  it('emits submitted with the bucketed repo count', () => {
    recordTuiCloudRepoAttach({ event: 'submitted', repoCount: 3 }, deps);
    const c = counterCalls[0]!;
    expect(c.attrs?.['repo_attach_event']).toBe('submitted');
    expect(c.attrs?.['repo_count_bucket']).toBe('3_5');
    expectEngineV3(c);
  });
});

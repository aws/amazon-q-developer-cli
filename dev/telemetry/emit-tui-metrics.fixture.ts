/**
 * Test fixture for `validate-metrics-e2e.sh`: drives the REAL TUI OTLP emitter
 * modules (no curl-faked payloads) against the local telemetry stack. Records
 * every V3 + §C4 + §E metric (and the engine=v2 set) with realistic args,
 * force-flushes the SDK, and reports ok/FAIL per metric from the shared drop
 * counter (`getEmitFailedTotal`) — a non-zero delta means that record dropped.
 *
 * Driven by the validate script, or run directly with bun:
 * KIRO_TELEMETRY_OTLP_ENDPOINT must point at the local collector
 * (e.g. http://localhost:4318), KIRO_TELEMETRY_ENABLED unset/!=false, and
 * KIRO_TEST_MODE must NOT be 'true' (the observer self-suppresses otherwise).
 */

import {
  recordTuiSessionStarted,
  recordTuiUserTurn,
  recordTuiToolCall,
  recordTuiTokensConsumed,
  recordTuiModelInvocation,
  recordTuiTurnOutcome,
  recordTuiToolExecutionDuration,
  recordTuiContextUsage,
  recordTuiModeActive,
  recordTuiSubagentDelegation,
  recordTuiProcessHealth,
  forceFlushMetrics,
  resultFromStatus,
} from '../../packages/tui/src/utils/tui-telemetry-observer';
import type { ProcessHealthSnapshot } from '../../packages/tui/src/utils/process-health-collector';
import { getEmitFailedTotal } from '../../packages/tui/src/utils/otlp-emit';

/** Record one metric (or group) and decide ok/FAIL from the shared drop counter. */
function step(name: string, fn: () => void): { name: string; ok: boolean; detail: string } {
  const before = getEmitFailedTotal();
  try {
    fn();
  } catch (err) {
    return { name, ok: false, detail: `threw: ${(err as Error).message}` };
  }
  const dropped = getEmitFailedTotal() - before;
  return dropped === 0
    ? { name, ok: true, detail: '' }
    : { name, ok: false, detail: `emit.failed_total +${dropped}` };
}

function sampleSnapshot(): ProcessHealthSnapshot {
  return {
    agentKind: 'kas',
    rssMb: 312,
    heapUsedMb: 96,
    peakRssMb: 401,
    cpuUserPct: 7.5,
    cpuSystemPct: 2.5,
    lastRenderMs: 4.2,
    maxRenderMs: 18,
    rendersPerMin: 120,
    fullRedrawsPerMin: 3,
    yogaNodeCount: 540,
    eventLoopP99Ms: 6.1,
    inputLatencyP95Ms: 12.3,
    sessionDurationSec: 95,
    cpuCores: 10,
    totalMemoryMb: 32768,
    terminal: 'metrics_e2e',
    sessionId: 'e2e-session-1',
    version: '0.0.0-e2e',
    platform: process.platform,
  };
}

async function main(): Promise<void> {
  const endpoint = process.env['KIRO_TELEMETRY_OTLP_ENDPOINT'] ?? '(unset)';
  if (process.env['KIRO_TEST_MODE'] === 'true') {
    throw new Error('KIRO_TEST_MODE=true would suppress real OTLP emits; unset it');
  }
  // eslint-disable-next-line no-console
  console.log(`[emit-tui-metrics] recording chat_cli_v3 metrics for ${endpoint}`);

  const model = 'claude-sonnet-4';

  const results = [
    step('kiro_cli_chat_session_started_total', () =>
      recordTuiSessionStarted({ mode: 'interactive', version: '2.4.0' })
    ),
    step('kiro_cli_user_turns + kiro_cli_user_turn_duration_seconds', () =>
      recordTuiUserTurn({
        model,
        result: resultFromStatus('completed'),
        isSubagent: false,
        mode: 'interactive',
        chatConversationType: 'not_history',
        durationSeconds: 7.5,
      })
    ),
    step('kiro_cli_tool_call_total + kiro_cli_tool_execution_duration_ms', () =>
      recordTuiToolCall({
        toolOrigin: 'builtin',
        builtinToolName: 'fs_read',
        outcome: 'success',
        executionDurationMs: 42,
      })
    ),

    step('kiro_cli_tokens_consumed', () =>
      recordTuiTokensConsumed({
        model,
        isSubagent: false,
        tokens: {
          input_uncached: 1200,
          input_cache_read: 800,
          input_cache_write: 300,
          output: 450,
        },
      })
    ),
    step('kiro_cli_model_invocations_total', () => recordTuiModelInvocation({ model })),
    step('kiro_cli_turn_outcome_total', () =>
      recordTuiTurnOutcome({ status: 'timeout', model, mode: 'interactive' })
    ),
    step('kiro_cli_tool_execution_duration_ms (standalone)', () =>
      recordTuiToolExecutionDuration({ toolOrigin: 'mcp', mcpServerName: 'local-server', isSuccess: true, durationMs: 88 })
    ),
    step('kiro_cli_context_usage_percentage', () =>
      recordTuiContextUsage({ model, isSubagent: false, percentage: 63.4 })
    ),
    step('kiro_cli_mode_active_total', () => recordTuiModeActive({ mode: 'interactive' })),
    step('kiro_cli_subagent_delegations_total', () =>
      recordTuiSubagentDelegation({ subagentName: 'code-review', model })
    ),

    step(
      'kiro_cli.process.memory.{rss,peak_rss,heap_used} + cpu.utilization + tui.{event_loop.delay,input.latency,render.duration}',
      () => recordTuiProcessHealth(sampleSnapshot())
    ),
  ];

  // --- engine=v2 batch: the TUI's client-experience view of the Rust/v2 agent
  // (telemetry-metric-inventory.md §H.6 = FULL-MIRROR MINUS ECONOMICS). The same
  // record fns are reused with engine:'v2' for the client-experience set ONLY.
  // Economics (kiro_cli_tokens_consumed /
  // kiro_cli_context_usage_percentage) are host-authoritative on v2 and are
  // DELIBERATELY OMITTED here to avoid double-counting — they stay v3-only.
  // eslint-disable-next-line no-console
  console.log(`[emit-tui-metrics] recording engine=v2 client-experience metrics for ${endpoint}`);
  const v2Snapshot: ProcessHealthSnapshot = { ...sampleSnapshot(), agentKind: 'v2' };
  results.push(
    step('[v2] kiro_cli_chat_session_started_total', () =>
      recordTuiSessionStarted({ mode: 'interactive', version: '2.4.0', engine: 'v2' })
    ),
    step('[v2] kiro_cli_user_turns + kiro_cli_user_turn_duration_seconds', () =>
      recordTuiUserTurn({
        model,
        result: resultFromStatus('completed'),
        isSubagent: false,
        mode: 'interactive',
        chatConversationType: 'not_history',
        durationSeconds: 7.5,
        engine: 'v2',
      })
    ),
    step('[v2] kiro_cli_tool_call_total + kiro_cli_tool_execution_duration_ms', () =>
      recordTuiToolCall({
        toolOrigin: 'builtin',
        builtinToolName: 'fs_read',
        outcome: 'success',
        executionDurationMs: 42,
        engine: 'v2',
      })
    ),
    step('[v2] kiro_cli_tool_execution_duration_ms (standalone)', () =>
      recordTuiToolExecutionDuration({
        toolOrigin: 'mcp',
        mcpServerName: 'local-server',
        isSuccess: true,
        durationMs: 88,
        engine: 'v2',
      })
    ),
    step('[v2] kiro_cli_model_invocations_total', () =>
      recordTuiModelInvocation({ model, engine: 'v2' })
    ),
    step('[v2] kiro_cli_turn_outcome_total', () =>
      recordTuiTurnOutcome({ status: 'timeout', model, mode: 'interactive', engine: 'v2' })
    ),
    step('[v2] kiro_cli_mode_active_total', () =>
      recordTuiModeActive({ mode: 'interactive', engine: 'v2' })
    ),
    step('[v2] kiro_cli_subagent_delegations_total', () =>
      recordTuiSubagentDelegation({ subagentName: 'code-review', model, engine: 'v2' })
    ),
    step(
      '[v2] kiro_cli.process.{memory,cpu} + tui.{event_loop,input,render} (process health)',
      () => recordTuiProcessHealth(v2Snapshot, 'v2')
    )
  );

  await forceFlushMetrics();

  let anyFailed = false;
  for (const { name, ok, detail } of results) {
    if (ok) {
      // eslint-disable-next-line no-console
      console.log(`  ok    ${name}`);
    } else {
      anyFailed = true;
      // eslint-disable-next-line no-console
      console.error(`  FAIL  ${name}: ${detail}`);
    }
  }

  // eslint-disable-next-line no-console
  console.log(`[emit-tui-metrics] emit.failed_total=${getEmitFailedTotal()}`);
  if (anyFailed) {
    process.exit(1);
  }
}

main().catch((err) => {
  // eslint-disable-next-line no-console
  console.error('[emit-tui-metrics] fatal', err);
  process.exit(1);
});

/**
 * Drives the production TUI metric observer against the local OTLP stack.
 * The validation script filters on `otel_scope_name=kiro.tui`, so catalog
 * smoke records cannot satisfy these producer assertions.
 */

import {
  forceFlushMetrics,
  recordTuiAutonomousMode,
  recordTuiCloudAttach,
  recordTuiCloudConfigDiagnostics,
  recordTuiCloudConfigSource,
  recordTuiCloudError,
  recordTuiCloudRepoAttach,
  recordTuiCloudSession,
  recordTuiCloudSessionReady,
  recordTuiConfigPanel,
  recordTuiCreditsConsumed,
  recordTuiFirstVisibleResponse,
  recordTuiModelInvocations,
  recordTuiProcessHealth,
  recordTuiRender,
  recordTuiSessionStarted,
  recordTuiSlashCommand,
  recordTuiTokensConsumed,
  recordTuiToolCall,
  recordTuiUiModeSessionStarted,
  recordTuiUserTurn,
  recordTuiWorkflowControl,
  recordTuiWorkflowObservations,
  recordTuiWorkflowRestore,
} from '../../packages/tui/src/utils/tui-telemetry-observer';
import type { ProcessHealthSnapshot } from '../../packages/tui/src/utils/process-health-collector';
import { getEmitFailedTotal } from '../../packages/tui/src/utils/otlp-emit';

const VERSION = '0.0.0-e2e';
const MODEL = 'claude-sonnet-4';

function step(
  name: string,
  fn: () => void
): { name: string; ok: boolean; detail: string } {
  const before = getEmitFailedTotal();
  try {
    fn();
  } catch (err) {
    return {
      name,
      ok: false,
      detail: `threw: ${(err as Error).message}`,
    };
  }
  const dropped = getEmitFailedTotal() - before;
  return dropped === 0
    ? { name, ok: true, detail: '' }
    : { name, ok: false, detail: `emit.failed_total +${dropped}` };
}

function sampleSnapshot(agentKind: 'v2' | 'kas'): ProcessHealthSnapshot {
  return {
    agentKind,
    rssMb: 312,
    heapUsedMb: 96,
    peakRssMb: 401,
    openFileDescriptorCount: 24,
    handleCount: 36,
    threadCount: 12,
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
    version: VERSION,
    platform: process.platform,
  };
}

async function main(): Promise<void> {
  const endpoint = process.env['KIRO_TELEMETRY_OTLP_ENDPOINT'] ?? '(unset)';
  if (process.env['KIRO_TEST_MODE'] === 'true') {
    throw new Error('KIRO_TEST_MODE=true would suppress real OTLP emits');
  }
  // eslint-disable-next-line no-console
  console.log(`[emit-tui-metrics] recording TUI metrics for ${endpoint}`);

  const results = [
    step('kiro_cli_chat_session_started_total', () =>
      recordTuiSessionStarted({
        mode: 'interactive',
        version: VERSION,
      })
    ),
    step('kiro_cli_ui_mode_session_started_total', () =>
      recordTuiUiModeSessionStarted({
        mode: 'tui',
        version: VERSION,
      })
    ),
    step('kiro_cli_slash_command_invoked_total{/help,v3}', () =>
      recordTuiSlashCommand({
        command: '/help',
        version: VERSION,
        engine: 'v3',
      })
    ),
    step('kiro_cli_slash_command_invoked_total{/settings,v3}', () =>
      recordTuiSlashCommand({
        command: '/settings',
        version: VERSION,
        engine: 'v3',
      })
    ),
    step('kiro_cli_slash_command_invoked_total{/upgrade-agent,v3}', () =>
      recordTuiSlashCommand({
        command: '/upgrade-agent',
        version: VERSION,
        engine: 'v3',
      })
    ),
    step('kiro_cli_slash_command_invoked_total{/workflow-run,v3}', () =>
      recordTuiSlashCommand({
        command: '/workflow-run',
        version: VERSION,
        engine: 'v3',
      })
    ),
    step('kiro_cli_slash_command_invoked_total{/goal,v3}', () =>
      recordTuiSlashCommand({
        command: '/goal',
        version: VERSION,
        engine: 'v3',
      })
    ),
    step('kiro_cli_slash_command_invoked_total{/config,v3}', () =>
      recordTuiSlashCommand({
        command: '/config',
        version: VERSION,
        engine: 'v3',
      })
    ),
    step('kiro_cli_config_panel_total', () =>
      recordTuiConfigPanel({
        category: 'menu',
        version: VERSION,
        engine: 'v3',
      })
    ),
    step('kiro_cli_cloud_config_diagnostic_total', () =>
      recordTuiCloudConfigDiagnostics({
        severities: ['warning', 'error'],
        version: VERSION,
        engine: 'v3',
      })
    ),
    step('kiro_cli_cloud_config_source_total', () =>
      recordTuiCloudConfigSource({
        surface: 'mcp',
        version: VERSION,
        engine: 'v3',
      })
    ),
    step('kiro_cli_time_to_first_visible_response_ms', () =>
      recordTuiFirstVisibleResponse({
        milliseconds: 180,
        mode: 'interactive',
        version: VERSION,
      })
    ),
    step('kiro_cli_cloud_session_lifecycle_total', () =>
      recordTuiCloudSession({ event: 'created', version: VERSION })
    ),
    step('kiro_cli_cloud_session_ready_seconds + lifecycle ready', () =>
      recordTuiCloudSessionReady({
        durationSeconds: 12,
        version: VERSION,
      })
    ),
    step('kiro_cli_autonomous_mode_total', () =>
      recordTuiAutonomousMode({
        event: 'enabled',
        version: VERSION,
        engine: 'v3',
      })
    ),
    step('kiro_cli_cloud_repo_attach_total', () =>
      recordTuiCloudRepoAttach({
        event: 'submitted',
        repoCount: 2,
        version: VERSION,
        engine: 'v3',
      })
    ),
    step('kiro_cli_cloud_error_total', () =>
      recordTuiCloudError({
        op: 'session_new',
        kind: 'version_skew',
        version: VERSION,
        engine: 'v3',
      })
    ),
    step('kiro_cli_cloud_attach_total', () =>
      recordTuiCloudAttach({
        kind: 'image',
        sizeBytes: 48 * 1024,
        version: VERSION,
        engine: 'v3',
      })
    ),
    step('successful user turn', () =>
      recordTuiUserTurn({
        result: 'success',
        isSubagent: false,
        mode: 'interactive',
        version: VERSION,
        durationSeconds: 7.5,
      })
    ),
    step('failed user turn', () =>
      recordTuiUserTurn({
        result: 'failed',
        isSubagent: false,
        mode: 'plan',
        version: VERSION,
        failureReason: 'model_error',
      })
    ),
    step('cancelled user turn', () =>
      recordTuiUserTurn({
        result: 'cancelled',
        isSubagent: false,
        mode: 'review',
        version: VERSION,
      })
    ),
    step('kiro_cli_tokens_consumed', () =>
      recordTuiTokensConsumed({
        version: VERSION,
        model: MODEL,
        tokens: {
          input_uncached: 1200,
          input_cache_read: 800,
          output: 450,
          reasoning: 50,
        },
      })
    ),
    step('kiro_cli_model_invocations_total', () =>
      recordTuiModelInvocations({
        version: VERSION,
        model: MODEL,
        count: 2,
      })
    ),
    step('kiro_cli_credits_consumed', () =>
      recordTuiCreditsConsumed({
        version: VERSION,
        model: MODEL,
        credits: 1.5,
      })
    ),
    step('kiro_cli_tool_call_total + kiro_cli_tool_execution_duration_ms', () =>
      recordTuiToolCall({
        toolOrigin: 'builtin',
        builtinToolName: 'fs_read',
        outcome: 'success',
        executionDurationMs: 42,
        version: VERSION,
      })
    ),
    step('MCP tool diagnostics stay out of metric dimensions', () =>
      recordTuiToolCall({
        toolOrigin: 'mcp',
        mcpServerName: 'local-server',
        outcome: 'error',
        executionDurationMs: 88,
        version: VERSION,
      })
    ),
    step('TUI process health', () =>
      recordTuiProcessHealth(sampleSnapshot('kas'))
    ),
    step('kiro_cli_tui_render_duration_seconds', () =>
      recordTuiRender({
        durationMs: 4.2,
        kind: 'partial',
        version: VERSION,
        platform: process.platform,
      })
    ),
    step('workflow lifecycle, duration, node, and concurrency metrics', () =>
      recordTuiWorkflowObservations(
        [
          {
            type: 'run',
            event: 'started',
            topology: 'sequential',
            stepBucket: '1',
          },
          {
            type: 'run_duration',
            durationSeconds: 12,
            outcome: 'completed',
            topology: 'sequential',
            stepBucket: '1',
          },
          { type: 'node', nodeType: 'step', outcome: 'completed' },
          {
            type: 'node_duration',
            durationSeconds: 6,
            nodeType: 'step',
            outcome: 'completed',
          },
          { type: 'concurrent', activeRuns: 2 },
        ],
        VERSION
      )
    ),
    step('kiro_cli_workflow_control_total', () =>
      recordTuiWorkflowControl('pause', 'success', VERSION)
    ),
    step('kiro_cli_workflow_restore_total', () =>
      recordTuiWorkflowRestore('restored', VERSION)
    ),
  ];

  results.push(
    step('[v2] kiro_cli_chat_session_started_total', () =>
      recordTuiSessionStarted({
        mode: 'interactive',
        version: VERSION,
        engine: 'v2',
      })
    ),
    step('[v2] kiro_cli_time_to_first_visible_response_ms', () =>
      recordTuiFirstVisibleResponse({
        milliseconds: 220,
        mode: 'interactive',
        version: VERSION,
        engine: 'v2',
      })
    ),
    step('[v2] kiro_cli_slash_command_invoked_total{/settings}', () =>
      recordTuiSlashCommand({
        command: '/settings',
        version: VERSION,
        engine: 'v2',
      })
    ),
    step('[v2] successful user turn', () =>
      recordTuiUserTurn({
        result: 'success',
        isSubagent: false,
        mode: 'interactive',
        version: VERSION,
        durationSeconds: 8.5,
        engine: 'v2',
      })
    ),
    step('[v2] tool call', () =>
      recordTuiToolCall({
        toolOrigin: 'builtin',
        builtinToolName: 'use_subagent',
        outcome: 'success',
        executionDurationMs: 120,
        executionContext: 'main',
        version: VERSION,
        engine: 'v2',
      })
    ),
    step('[v2] TUI process health', () =>
      recordTuiProcessHealth(sampleSnapshot('v2'), 'v2')
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
  if (anyFailed) process.exit(1);
}

main().catch((err) => {
  // eslint-disable-next-line no-console
  console.error('[emit-tui-metrics] fatal', err);
  process.exit(1);
});

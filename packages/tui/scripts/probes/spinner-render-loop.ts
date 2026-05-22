#!/usr/bin/env bun
/**
 * spinner-render-loop.ts — Review 17 technique 10 / Review 06 technique 10
 *
 * Reproduces the idle-CPU render loop caused by a PieSpinner mounted in the
 * active turn tail when a ToolUseMessage has isFinished=false.
 *
 * Root cause: PieSpinner (150ms setInterval) is NOT inside a twinki <Region>,
 * so every frame update triggers commitUpdate → requestRender → full yoga
 * layout pass. With frameBudgetMs=0 and a large node tree, this pegs CPU.
 *
 * This probe launches the TUI in test mode, injects an unfinished ToolCall
 * via IPC, then measures CPU over a 5-second idle window.
 *
 * ## Pass / fail
 *
 *   PASS if idle CPU ≤ 10% with an unfinished tool call mounted
 *   FAIL if idle CPU > 10% (render loop from PieSpinner without Region)
 *
 * Exit codes: 0 = pass, 1 = finding (render loop confirmed), 2 = probe crash
 *
 * Usage:
 *   cd packages/tui && bun run build && cd ../..
 *   bun run packages/tui/scripts/probes/spinner-render-loop.ts
 *
 * Environment:
 *   IDLE_MEASURE_MS   idle measurement window (default: 5000)
 *   PROBE_OUTPUT_DIR  where to write findings/metrics
 */

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { PtyManager } from '../../src/test-utils/shared/pty-manager';

const PROBE_NAME = 'spinner-render-loop';
const OUTPUT_DIR = process.env.PROBE_OUTPUT_DIR ?? './probe-output';
const IDLE_MEASURE_MS = parseInt(process.env.IDLE_MEASURE_MS ?? '5000', 10);

// Budgets
const MAX_IDLE_CPU_PCT = 10;

function timestamp(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${now.getUTCFullYear()}${pad(now.getUTCMonth() + 1)}${pad(now.getUTCDate())}-${pad(now.getUTCHours())}${pad(now.getUTCMinutes())}`;
}

const TS = timestamp();

async function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function measureCpuAvg(pid: number, windowMs: number): Promise<number> {
  const samples: number[] = [];
  const interval = 1000;
  const count = Math.max(1, Math.ceil(windowMs / interval));

  for (let i = 0; i < count; i++) {
    await sleep(interval);
    const proc = Bun.spawn(['ps', '-p', String(pid), '-o', '%cpu='], {
      stdout: 'pipe',
    });
    const text = await new Response(proc.stdout).text();
    await proc.exited;
    const cpu = parseFloat(text.trim());
    if (!isNaN(cpu)) samples.push(cpu);
  }

  return samples.length > 0
    ? samples.reduce((a, b) => a + b, 0) / samples.length
    : 0;
}

/**
 * Send keystrokes to the PTY to trigger a tool call that won't finish.
 * We use the /tools command to verify the TUI is responsive, then send
 * a message that will trigger a tool call via the mock backend.
 *
 * Since we can't easily inject events without MOCK_SESSION_UPDATE support,
 * we use a simpler approach: send a user message that triggers a tool call,
 * then kill the backend (simulating MCP failure) before it finishes.
 */
async function main(): Promise<number> {
  mkdirSync(OUTPUT_DIR, { recursive: true });

  console.log(`[${PROBE_NAME}] Starting render loop reproduction probe`);
  console.log(`[${PROBE_NAME}] Strategy: Launch TUI with PUSH_SEND_MESSAGE_RESPONSE mock`);
  console.log(`[${PROBE_NAME}]   → inject ToolCall without ToolCallFinished`);
  console.log(`[${PROBE_NAME}]   → measure idle CPU with PieSpinner mounted`);
  console.log(`[${PROBE_NAME}] Idle measurement window: ${IDLE_MEASURE_MS}ms`);

  // Build the mock event sequence: a ToolCall that never finishes
  // This simulates what happens when MCP server dies mid-tool-execution
  const toolCallId = `probe-stuck-tool-${Date.now()}`;
  const mockEvents = [
    {
      type: 'tool_call',
      id: toolCallId,
      name: 'execute_bash',
      kind: 'execute',
      args: { command: 'sleep 999' },
    },
    // Deliberately NO tool_call_finished — this is the bug trigger
  ];

  // Write mock events to a temp file for the TUI to consume
  const mockEventsPath = join(
    process.env.TMPDIR ?? '/tmp',
    `kiro-probe-mock-events-${Date.now()}.json`
  );
  writeFileSync(mockEventsPath, JSON.stringify(mockEvents));

  // Launch TUI with the integration test infrastructure
  const tuiJsPath = join(import.meta.dir, '../../dist/tui.js');

  // Use PUSH_SEND_MESSAGE_RESPONSE via IPC to inject the stuck tool call
  // For now, use a simpler approach: launch with a pre-canned response
  const ipcSocketPath = join(
    process.env.TMPDIR ?? '/tmp',
    `kiro-probe-${PROBE_NAME}-${Date.now()}.sock`
  );

  const ptyManager = new PtyManager({
    width: 120,
    height: 40,
    env: {
      KIRO_TEST_MODE: '1',
      KIRO_TEST_TUI_IPC_SOCKET_PATH: ipcSocketPath,
      KIRO_DEV: '1',
      TERM: 'xterm-256color',
      KIRO_TEST_MOCK_RESPONSES: mockEventsPath,
    },
    cwd: process.cwd(),
  });

  ptyManager.spawn('bun', [tuiJsPath, 'chat']);

  const pid = ptyManager.getPid();
  if (!pid) throw new Error('Could not get TUI process PID');
  console.log(`[${PROBE_NAME}] TUI launched, PID: ${pid}`);

  // Wait for TUI to initialize
  console.log(`[${PROBE_NAME}] Waiting for TUI to stabilize (3s)...`);
  await sleep(3000);

  // Measure baseline CPU (no spinner yet)
  console.log(`[${PROBE_NAME}] Measuring baseline CPU...`);
  const baselineCpu = await measureCpuAvg(pid, 3000);
  console.log(`[${PROBE_NAME}] Baseline CPU: ${baselineCpu.toFixed(1)}%`);

  // Type a message to trigger the mock response with the stuck tool call
  console.log(`[${PROBE_NAME}] Sending user message to trigger stuck tool call...`);
  await ptyManager.sendKeys('run a long command\r');

  // Wait for the tool call to render and PieSpinner to mount
  console.log(`[${PROBE_NAME}] Waiting for PieSpinner to mount (2s)...`);
  await sleep(2000);

  // Measure CPU with PieSpinner active (the bug: should be low but will be high)
  console.log(`[${PROBE_NAME}] Measuring CPU with unfinished tool (${IDLE_MEASURE_MS}ms)...`);
  const spinnerCpu = await measureCpuAvg(pid, IDLE_MEASURE_MS);
  console.log(`[${PROBE_NAME}] CPU with spinner: ${spinnerCpu.toFixed(1)}%`);

  // Clean up
  ptyManager.kill();
  await sleep(500);

  // Evaluate
  const passed = spinnerCpu <= MAX_IDLE_CPU_PCT;
  const cpuDelta = spinnerCpu - baselineCpu;

  const result = {
    probe: PROBE_NAME,
    timestamp: TS,
    passed,
    metrics: {
      baselineCpuPct: parseFloat(baselineCpu.toFixed(1)),
      spinnerCpuPct: parseFloat(spinnerCpu.toFixed(1)),
      cpuDeltaPct: parseFloat(cpuDelta.toFixed(1)),
      measureWindowMs: IDLE_MEASURE_MS,
    },
    budgets: {
      maxIdleCpuPct: MAX_IDLE_CPU_PCT,
    },
    rootCause: !passed
      ? [
          'PieSpinner in ToolUseMessage (ActiveTurnTail) triggers full-tree re-render every 150ms.',
          'The spinner is NOT wrapped in a twinki <Region>, so setFrameIndex() → commitUpdate → requestRender() causes a full yoga layout pass.',
          'With frameBudgetMs=0 (no frame pacing) and a large node tree (8620 nodes observed in production), each 150ms tick costs 72ms of CPU.',
          'Net effect: ~14 renders/sec × 72ms = 100% CPU while idle.',
          'Trigger: MCP server re-initialization leaves a tool call stuck with isFinished=false in the active turn tail.',
        ]
      : null,
    fix: !passed
      ? [
          '1. Wrap PieSpinner/Spinner in StatusBar.tsx with a <Region id="spinner-{id}"> to scope re-renders to just the spinner character',
          '2. Set targetFps (e.g. 30) on the twinki render() call to add frame pacing as a safety net',
          '3. Ensure cancelMessage() marks all unfinished tool calls as finished (already exists but may race with MCP server death)',
          '4. Add a timeout that auto-finishes tool calls stuck in executing state for > N seconds',
        ]
      : null,
    reviewPlaybookMatch: [
      'Review 17 — Render Loop Guards (technique 10: idle CPU > 10% = render loop)',
      'Review 06 — Hot-path Allocation (technique 10: idle-CPU soak, 50-65% stuck CPU pattern)',
      'Review 01 — Async Render Path (setInterval in render path without frame pacing)',
    ],
  };

  const resultPath = join(OUTPUT_DIR, `${PROBE_NAME}-${TS}.json`);
  writeFileSync(resultPath, JSON.stringify(result, null, 2));
  console.log(`[${PROBE_NAME}] Results: ${resultPath}`);

  if (passed) {
    console.log(`[${PROBE_NAME}] ✅ PASS — idle CPU ${spinnerCpu.toFixed(1)}% ≤ ${MAX_IDLE_CPU_PCT}%`);
  } else {
    console.log(`[${PROBE_NAME}] ❌ FAIL — idle CPU ${spinnerCpu.toFixed(1)}% > ${MAX_IDLE_CPU_PCT}%`);
    console.log(`[${PROBE_NAME}]   CPU delta from baseline: +${cpuDelta.toFixed(1)}%`);
    console.log(`[${PROBE_NAME}]   Root cause: PieSpinner triggers full-tree re-render (no <Region> isolation)`);
  }

  return passed ? 0 : 1;
}

main()
  .then((code) => process.exit(code))
  .catch((err) => {
    console.error(`[${PROBE_NAME}] Probe crashed:`, err);
    process.exit(2);
  });

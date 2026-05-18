#!/usr/bin/env bun
/**
 * backed-up-pty.ts — Blackbox probe for backed-up PTY memory behavior.
 *
 * Launches the TUI in a PTY, simulates a slow reader (pauses reading stdout),
 * triggers output-generating activity (resize storms), and measures whether
 * memory grows unboundedly when the PTY consumer is backed up.
 *
 * Pass: memory growth < 100MB during backed-up period, process recovers.
 * Fail: memory growth >= 100MB or process does not recover.
 *
 * Exit codes: 0 = pass, 1 = finding, 2 = probe crash.
 */

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { execSync } from 'node:child_process';
import * as pty from 'bun-pty';

const PROBE_NAME = 'backed-up-pty';
const PLATFORM = process.env.KIRO_PROBE_PLATFORM ?? (process.platform === 'darwin' ? 'macos' : process.platform);
const OUTPUT_DIR = process.env.PROBE_OUTPUT_DIR ?? './probe-output';
const MEMORY_BUDGET_MB = 100;
const BACKED_UP_DURATION_MS = 15_000;
const RECOVERY_WAIT_MS = 5_000;

function timestamp(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${now.getUTCFullYear()}${pad(now.getUTCMonth() + 1)}${pad(now.getUTCDate())}-${pad(now.getUTCHours())}${pad(now.getUTCMinutes())}`;
}

function slugify(text: string): string {
  return text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 50);
}

const TS = timestamp();
const PREFIX = `${PROBE_NAME}-${PLATFORM}-${TS}`;

function getRssMb(pid: number): number {
  try {
    const out = execSync(`ps -o rss= -p ${pid}`, { encoding: 'utf-8' }).trim();
    return Math.round(parseInt(out, 10) / 1024);
  } catch {
    return -1;
  }
}

function writeFinding(opts: {
  slug: string;
  title: string;
  severity: 'crash' | 'spiral' | 'regression' | 'slowdown' | 'smell';
  file: string;
  line?: number;
  description: string;
  evidence?: string;
  proposedFix?: string;
}) {
  const slug = slugify(opts.slug);
  const findingId = `${PREFIX}-${slug}`;
  const path = join(OUTPUT_DIR, `${findingId}.md`);
  const frontmatter = [
    '---',
    `id: ${findingId}`,
    `work-item: ${PROBE_NAME}-${PLATFORM}`,
    `review: blackbox-probe`,
    `technique: 1`,
    `class: probe-pty-backpressure`,
    `severity: ${opts.severity}`,
    `file: ${opts.file}`,
    opts.line !== undefined ? `line: ${opts.line}` : '',
    `platforms-affected: [${PLATFORM}]`,
    `discovered-by: blackbox`,
    `discovered-at: ${new Date().toISOString()}`,
    `status: open`,
    '---',
  ].filter(Boolean).join('\n');

  const body = [
    `# ${opts.title}`,
    '',
    opts.description,
    opts.evidence ? `\n## Evidence\n\n${opts.evidence}` : '',
    opts.proposedFix ? `\n## Proposed fix\n\n${opts.proposedFix}` : '',
  ].filter(Boolean).join('\n');

  writeFileSync(path, `${frontmatter}\n\n${body}\n`);
  return path;
}

function writeMetrics(metrics: Record<string, unknown>) {
  const path = join(OUTPUT_DIR, `${PREFIX}-metrics.json`);
  writeFileSync(path, JSON.stringify(metrics, null, 2));
  return path;
}

function writeDoneMarker(summary: { findingsEmitted: number; elapsedMs: number; metrics: Record<string, unknown> }) {
  const path = join(OUTPUT_DIR, `${PREFIX}-done.md`);
  writeFileSync(path, [
    '---',
    `id: ${PREFIX}-done`,
    `work-item: ${PROBE_NAME}-${PLATFORM}`,
    `kind: blackbox`,
    `platform: ${PLATFORM}`,
    `status: done`,
    `findings-emitted: ${summary.findingsEmitted}`,
    `elapsed-ms: ${summary.elapsedMs}`,
    `completed-at: ${new Date().toISOString()}`,
    '---',
    '',
    `# Probe done: ${PROBE_NAME} on ${PLATFORM}`,
    '',
    `Emitted ${summary.findingsEmitted} finding(s) in ${summary.elapsedMs} ms.`,
  ].join('\n'));
  return path;
}

async function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const started = Date.now();

  // Resolve TUI JS path
  const tuiJsPath = process.env.KIRO_TEST_TUI_JS_PATH
    ?? join(process.cwd(), 'packages/tui/dist/tui.js');

  // Spawn the TUI in a PTY
  const ptyProcess = pty.spawn('cargo', ['run', '-p', 'chat_cli', '--', 'chat', '--tui'], {
    name: 'xterm-color',
    cols: 80,
    rows: 24,
    cwd: process.env.KIRO_CLI_ROOT ?? process.cwd(),
    env: {
      ...(process.env as Record<string, string>),
      KIRO_TEST_TUI_JS_PATH: tuiJsPath,
      KIRO_TEST_MODE: '1',
      KIRO_DISABLE_TELEMETRY: '1',
      TERM: 'xterm-256color',
    },
  });

  const pid = ptyProcess.pid;
  console.log(`[${PROBE_NAME}] spawned PID=${pid}`);

  // Collect output normally during startup
  let reading = true;
  const dataChunks: string[] = [];

  ptyProcess.onData((data) => {
    if (reading) {
      dataChunks.push(data);
    }
    // When reading=false, we simply don't consume — simulating a slow reader
    // The PTY buffer will back up in the kernel
  });

  // Wait for TUI to initialize
  console.log(`[${PROBE_NAME}] waiting for TUI startup...`);
  await sleep(5_000);

  const rssBeforeMb = getRssMb(pid);
  console.log(`[${PROBE_NAME}] RSS before throttle: ${rssBeforeMb} MB`);

  // === PHASE: Simulate slow reader ===
  // Stop consuming PTY output — kernel buffer will fill, causing backpressure
  reading = false;
  console.log(`[${PROBE_NAME}] throttle ON — pausing reads for ${BACKED_UP_DURATION_MS}ms`);

  // Generate output by sending resize storms while reader is paused
  const resizeInterval = setInterval(() => {
    try {
      const cols = 60 + Math.floor(Math.random() * 60);
      const rows = 20 + Math.floor(Math.random() * 20);
      ptyProcess.resize(cols, rows);
    } catch {
      // Process may have exited
    }
  }, 100);

  // Also send input that would trigger rendering
  const inputInterval = setInterval(() => {
    try {
      ptyProcess.write('a');
    } catch {
      // Process may have exited
    }
  }, 200);

  // Sample RSS during backed-up period
  const rssSamples: number[] = [];
  const sampleInterval = setInterval(() => {
    const rss = getRssMb(pid);
    if (rss > 0) rssSamples.push(rss);
  }, 1_000);

  await sleep(BACKED_UP_DURATION_MS);

  clearInterval(resizeInterval);
  clearInterval(inputInterval);
  clearInterval(sampleInterval);

  const rssDuringMaxMb = rssSamples.length > 0 ? Math.max(...rssSamples) : getRssMb(pid);
  const memoryGrowthMb = rssDuringMaxMb - rssBeforeMb;
  console.log(`[${PROBE_NAME}] RSS peak during throttle: ${rssDuringMaxMb} MB (growth: ${memoryGrowthMb} MB)`);

  // === PHASE: Resume reading — check recovery ===
  reading = true;
  console.log(`[${PROBE_NAME}] throttle OFF — resuming reads, waiting for recovery...`);

  await sleep(RECOVERY_WAIT_MS);

  const rssAfterRecoveryMb = getRssMb(pid);
  const processAlive = rssAfterRecoveryMb > 0;
  console.log(`[${PROBE_NAME}] RSS after recovery: ${rssAfterRecoveryMb} MB, alive: ${processAlive}`);

  // Cleanup
  try { ptyProcess.kill(); } catch { /* already exited */ }

  // === Evaluate results ===
  const elapsedMs = Date.now() - started;
  let findingsEmitted = 0;

  const metrics = {
    probe: PROBE_NAME,
    platform: PLATFORM,
    rssBeforeMb,
    rssDuringMaxMb,
    rssAfterRecoveryMb,
    memoryGrowthMb,
    processRecovered: processAlive,
    backedUpDurationMs: BACKED_UP_DURATION_MS,
    rssSamples,
    elapsedMs,
  };

  if (memoryGrowthMb >= MEMORY_BUDGET_MB) {
    writeFinding({
      slug: 'memory-growth-during-backpressure',
      title: `RSS grew ${memoryGrowthMb} MB during backed-up PTY (budget: ${MEMORY_BUDGET_MB} MB)`,
      severity: 'spiral',
      file: 'packages/tui/src/renderer/tui.ts',
      description: `When the PTY consumer pauses reads for ${BACKED_UP_DURATION_MS}ms while resize storms generate output, RSS grew from ${rssBeforeMb} MB to ${rssDuringMaxMb} MB (delta: ${memoryGrowthMb} MB), exceeding the ${MEMORY_BUDGET_MB} MB budget.`,
      evidence: `RSS samples (MB): ${rssSamples.join(', ')}`,
      proposedFix: 'Implement backpressure-aware rendering that coalesces or drops frames when the PTY write buffer is full.',
    });
    findingsEmitted++;
  }

  if (!processAlive) {
    writeFinding({
      slug: 'process-did-not-recover',
      title: 'TUI process crashed or hung after PTY backpressure cleared',
      severity: 'crash',
      file: 'packages/tui/src/renderer/tui.ts',
      description: 'After resuming normal PTY reads, the TUI process was no longer running. It either crashed or became unresponsive during the backed-up period.',
      evidence: `PID ${pid} not found after recovery wait of ${RECOVERY_WAIT_MS}ms.`,
    });
    findingsEmitted++;
  }

  writeMetrics(metrics);
  writeDoneMarker({ findingsEmitted, elapsedMs, metrics });

  console.log(`[${PROBE_NAME}] result: ${findingsEmitted > 0 ? 'FAIL' : 'PASS'} (${findingsEmitted} finding(s), ${elapsedMs}ms)`);
  process.exit(findingsEmitted > 0 ? 1 : 0);
}

try {
  await main();
} catch (err) {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const errorPath = join(OUTPUT_DIR, `${PREFIX}-error.log`);
  writeFileSync(errorPath, String(err instanceof Error ? (err.stack ?? err.message) : err));
  console.error(`[${PROBE_NAME}] probe crashed:`, err);
  process.exit(2);
}

#!/usr/bin/env bun
/**
 * resize-storm.ts — blackbox probe: rapid SIGWINCH memory-bounds verification.
 *
 * Sends 1000 rapid PTY resizes to the TUI and verifies:
 *   - RSS memory growth stays under budget (20 MB)
 *   - Process remains responsive after the storm
 *   - No crash occurs
 *
 * Exit codes: 0 = pass, 1 = finding, 2 = probe crash
 *
 * Usage:
 *   bun run packages/tui/scripts/probes/resize-storm.ts
 */

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { execSync } from 'node:child_process';
import { PtyManager } from '../../src/test-utils/shared/pty-manager';
import { resolveChatCliBin } from '../../src/utils/chat-cli-bin';

const PROBE_NAME = 'resize-storm';
const PLATFORM = process.env.KIRO_PROBE_PLATFORM ?? (process.platform === 'darwin' ? 'macos' : process.platform);
const OUTPUT_DIR = process.env.PROBE_OUTPUT_DIR ?? './probe-output';
const RESIZE_COUNT = 1000;
const MEMORY_BUDGET_MB = 20;
const SETTLE_MS = 10000;
const INIT_TIMEOUT_MS = 15000;
const RESPONSIVENESS_TIMEOUT_MS = 15000;

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
    `class: resize-storm`,
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
    '',
  ].join('\n'));
  return path;
}

function getRssKb(pid: number): number {
  try {
    const out = execSync(`ps -o rss= -p ${pid}`, { encoding: 'utf-8' }).trim();
    return parseInt(out, 10);
  } catch {
    return -1;
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const started = Date.now();
  let findingsEmitted = 0;

  // Resolve binary and TUI JS path. Honors CHAT_CLI_BIN as a probe-
  // specific override; otherwise delegates to the shared resolver
  // (KIRO_CHAT_CLI_BIN / CARGO_TARGET_DIR / repo-root target).
  const binaryPath = process.env.CHAT_CLI_BIN ?? resolveChatCliBin();
  const tuiJsPath = process.env.KIRO_TEST_TUI_JS_PATH ?? join(process.cwd(), 'packages/tui/dist/tui.js');

  console.log(`[${PROBE_NAME}] binary=${binaryPath} tui_js=${tuiJsPath}`);

  // Launch TUI in a PTY
  const ptyMgr = new PtyManager({
    width: 120,
    height: 40,
    env: {
      KIRO_TEST_TUI_JS_PATH: tuiJsPath,
      KIRO_TEST_MODE: '1',
      KIRO_DISABLE_TELEMETRY: '1',
      TERM: 'xterm-color',
    },
  });
  ptyMgr.spawn(binaryPath, ['chat', '--tui']);

  const pid = ptyMgr.getPid();
  if (!pid) {
    throw new Error('Failed to get PID from PTY');
  }
  console.log(`[${PROBE_NAME}] PID=${pid}`);

  // Wait for TUI to initialize — look for any output indicating readiness
  console.log(`[${PROBE_NAME}] Waiting for TUI init...`);
  const initStart = Date.now();
  while (Date.now() - initStart < INIT_TIMEOUT_MS) {
    const output = ptyMgr.getOutputCleaned();
    // TUI is ready when we see content rendered (prompt, welcome, or cursor movement)
    if (output.length > 50) break;
    await sleep(100);
  }
  await sleep(2000); // Extra settle time for full initialization

  // Verify process is alive
  if (getRssKb(pid) === -1) {
    throw new Error('Process died during initialization');
  }

  // Measure baseline RSS
  const baselineRssKb = getRssKb(pid);
  console.log(`[${PROBE_NAME}] Baseline RSS: ${baselineRssKb} KB (${Math.round(baselineRssKb / 1024)} MB)`);

  // Execute resize storm: 1000 rapid resizes alternating between two sizes
  console.log(`[${PROBE_NAME}] Starting resize storm (${RESIZE_COUNT} resizes)...`);
  const stormStart = Date.now();
  for (let i = 0; i < RESIZE_COUNT; i++) {
    const cols = (i % 2 === 0) ? 80 : 120;
    const rows = (i % 2 === 0) ? 24 : 40;
    ptyMgr.resize(cols, rows);
  }
  const stormDurationMs = Date.now() - stormStart;
  console.log(`[${PROBE_NAME}] Storm complete in ${stormDurationMs} ms`);

  // Measure mid-point RSS immediately after storm
  const midRssKb = getRssKb(pid);
  console.log(`[${PROBE_NAME}] Post-storm RSS: ${midRssKb} KB (${Math.round(midRssKb / 1024)} MB)`);

  // Allow settling time
  console.log(`[${PROBE_NAME}] Settling for ${SETTLE_MS} ms...`);
  await sleep(SETTLE_MS);

  // Measure final RSS
  const finalRssKb = getRssKb(pid);
  const deltaKb = finalRssKb - baselineRssKb;
  const deltaMb = deltaKb / 1024;
  console.log(`[${PROBE_NAME}] Final RSS: ${finalRssKb} KB (${Math.round(finalRssKb / 1024)} MB)`);
  console.log(`[${PROBE_NAME}] Delta: ${deltaKb} KB (${deltaMb.toFixed(1)} MB)`);

  // Check responsiveness: send a keystroke and see if output changes
  let responsive = false;
  if (getRssKb(pid) !== -1) {
    const outputBefore = ptyMgr.getOutput().length;
    await ptyMgr.sendKeys(' ');
    const respStart = Date.now();
    while (Date.now() - respStart < RESPONSIVENESS_TIMEOUT_MS) {
      if (ptyMgr.getOutput().length > outputBefore) {
        responsive = true;
        break;
      }
      await sleep(50);
    }
  }
  console.log(`[${PROBE_NAME}] Responsive: ${responsive}`);

  // Check if process crashed
  const crashed = getRssKb(pid) === -1;

  // Evaluate pass/fail
  const metrics = {
    probe: PROBE_NAME,
    platform: PLATFORM,
    baselineRssKb,
    midRssKb,
    finalRssKb,
    deltaKb,
    deltaMb: Math.round(deltaMb * 10) / 10,
    resizeCount: RESIZE_COUNT,
    stormDurationMs,
    responsive,
    crashed,
    pid,
  };

  if (crashed) {
    findingsEmitted++;
    writeFinding({
      slug: 'crash-during-resize-storm',
      title: 'TUI crashed during resize storm',
      severity: 'crash',
      file: 'packages/tui/src/renderer/tui.ts',
      description: `Process (PID ${pid}) died during or after ${RESIZE_COUNT} rapid resize events.`,
      evidence: `Baseline RSS: ${baselineRssKb} KB\nMid-storm RSS: ${midRssKb} KB\nProcess not found after settle period.`,
    });
  } else if (deltaMb > MEMORY_BUDGET_MB) {
    findingsEmitted++;
    writeFinding({
      slug: 'memory-growth-exceeds-budget',
      title: `RSS grew ${deltaMb.toFixed(1)} MB (budget: ${MEMORY_BUDGET_MB} MB)`,
      severity: 'spiral',
      file: 'packages/tui/src/renderer/tui.ts',
      description: `After ${RESIZE_COUNT} rapid resizes, RSS grew ${deltaMb.toFixed(1)} MB beyond baseline, exceeding the ${MEMORY_BUDGET_MB} MB budget.`,
      evidence: `Baseline: ${baselineRssKb} KB\nPost-storm: ${midRssKb} KB\nFinal (after ${SETTLE_MS}ms settle): ${finalRssKb} KB\nDelta: ${deltaKb} KB (${deltaMb.toFixed(1)} MB)`,
      proposedFix: 'Investigate resize handler for unbounded allocations or missing cleanup. Check for accumulated layout/render buffers.',
    });
  } else if (!responsive) {
    findingsEmitted++;
    writeFinding({
      slug: 'unresponsive-after-resize-storm',
      title: 'TUI unresponsive after resize storm',
      severity: 'spiral',
      file: 'packages/tui/src/renderer/tui.ts',
      description: `After ${RESIZE_COUNT} rapid resizes, the TUI did not respond to input within ${RESPONSIVENESS_TIMEOUT_MS} ms.`,
      evidence: `Process alive: true\nRSS delta: ${deltaMb.toFixed(1)} MB\nNo output received after sending keystroke.`,
    });
  }

  // Clean up
  ptyMgr.kill();

  const elapsedMs = Date.now() - started;
  writeMetrics({ ...metrics, elapsedMs });
  writeDoneMarker({ findingsEmitted, elapsedMs, metrics });

  const result = findingsEmitted > 0 ? 'FAIL' : 'PASS';
  console.log(`\n[${PROBE_NAME}] ${result} — delta=${deltaMb.toFixed(1)}MB responsive=${responsive} crashed=${crashed}`);
  process.exit(findingsEmitted > 0 ? 1 : 0);
}

try {
  await main();
} catch (err) {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const errorPath = join(OUTPUT_DIR, `${PREFIX}-error.log`);
  writeFileSync(errorPath, String(err instanceof Error ? (err.stack ?? err.message) : err));
  console.error(`[${PROBE_NAME}] probe crashed on ${PLATFORM}:`);
  console.error(err);
  process.exit(2);
}

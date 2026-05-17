#!/usr/bin/env bun
/**
 * subscription-stress.ts — Review 03 technique 9 blackbox probe.
 *
 * Verifies that subscriptions / listeners do not leak across session
 * lifecycle. Runs 100 rapid create/destroy cycles of the TUI session
 * process and measures whether the probe-process's own resource
 * footprint (RSS, open FDs) stays bounded across iterations.
 *
 * ## Scope and caveat
 *
 * The companion runbook (docs/review-playbook/runner/work/
 * 03-subscription-stress-blackbox.md) asks for `.listenerCount()`
 * readings on every EventEmitter after each teardown. The TUI's
 * test-mode IPC (`KIRO_TEST_TUI_IPC_SOCKET_PATH`) does not currently
 * expose a `LISTENER_COUNTS` command, so this probe measures the
 * closest blackbox proxy: process-level resources held by the probe
 * after each cycle's teardown. Cross-process leak detection catches:
 *
 *   - bun-pty FD leaks in the probe harness itself
 *   - Monotonic startup-RSS drift in the child (bundle-level leaks)
 *   - Orphaned child processes not reaped by `kill()`
 *
 * In-process listener leaks within a single child's lifetime are NOT
 * covered — add a `LISTENER_COUNTS` IPC command and upgrade this probe
 * to get that signal.
 *
 * ## Pass / fail
 *
 *   PASS if, across all 100 cycles:
 *     - Probe-process RSS growth < 20 MB total
 *     - Probe-process FD growth < 5 total
 *     - Each child TUI reached steady-state RSS within 10 % of the first child's steady-state RSS
 *     - No iteration failed to spawn or exit cleanly
 *
 *   FAIL otherwise, with the top offending iteration highlighted.
 *
 * Exit codes: 0 = pass, 1 = finding, 2 = probe crash
 *
 * Usage:
 *   bun run packages/tui/scripts/probes/subscription-stress.ts
 *
 * Environment:
 *   STRESS_CYCLES        number of spawn/kill cycles (default: 100; CI uses 20)
 *   STRESS_DWELL_MS      time each child stays alive before being killed (default: 500)
 *   PROBE_OUTPUT_DIR     where to write findings/metrics
 *   KIRO_TEST_TUI_JS_PATH path to built TUI bundle
 *   CHAT_CLI_BIN         path to chat_cli binary (default: cargo-run)
 */

import { execSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { PtyManager } from '../../src/test-utils/shared/pty-manager';

const PROBE_NAME = 'subscription-stress';
const PLATFORM =
  process.env.KIRO_PROBE_PLATFORM ??
  (process.platform === 'darwin' ? 'macos' : process.platform);
const OUTPUT_DIR = process.env.PROBE_OUTPUT_DIR ?? './probe-output';

const DEFAULT_CYCLES = process.env.CI === '1' ? 20 : 100;
const CYCLES = parseInt(process.env.STRESS_CYCLES ?? String(DEFAULT_CYCLES), 10);
const DWELL_MS = parseInt(process.env.STRESS_DWELL_MS ?? '500', 10);

// Budgets
const PROBE_RSS_GROWTH_BUDGET_MB = 20;
const PROBE_FD_GROWTH_BUDGET = 5;
const CHILD_RSS_DRIFT_BUDGET_PCT = 10;

interface Cycle {
  index: number;
  spawnedAtMs: number;
  childPid: number | undefined;
  childPeakRssKb: number;
  childExited: boolean;
  probeRssKb: number;
  probeFdCount: number;
  elapsedMs: number;
}

function timestamp(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${now.getUTCFullYear()}${pad(now.getUTCMonth() + 1)}${pad(now.getUTCDate())}-${pad(now.getUTCHours())}${pad(now.getUTCMinutes())}`;
}

function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 50);
}

const TS = timestamp();
const PREFIX = `${PROBE_NAME}-${PLATFORM}-${TS}`;

function getRssKb(pid: number): number {
  try {
    const out = execSync(`ps -o rss= -p ${pid}`, { encoding: 'utf-8' }).trim();
    return parseInt(out, 10) || 0;
  } catch {
    return -1;
  }
}

function getFdCount(pid: number): number {
  try {
    if (process.platform === 'linux') {
      const { readdirSync } = require('node:fs');
      return readdirSync(`/proc/${pid}/fd`).length;
    }
    const out = execSync(`lsof -p ${pid} 2>/dev/null | wc -l`, {
      encoding: 'utf-8',
    }).trim();
    return Math.max(0, parseInt(out, 10) - 1);
  } catch {
    return 0;
  }
}

function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/** Slope of y over index, in units of y per cycle. */
function slopePerCycle(values: number[]): number {
  if (values.length < 2) return 0;
  const n = values.length;
  const xMean = (n - 1) / 2;
  const yMean = values.reduce((a, b) => a + b, 0) / n;
  let num = 0;
  let den = 0;
  for (let i = 0; i < n; i++) {
    const v = values[i]!;
    num += (i - xMean) * (v - yMean);
    den += (i - xMean) ** 2;
  }
  return den === 0 ? 0 : num / den;
}

function writeFinding(opts: {
  slug: string;
  title: string;
  severity: 'crash' | 'spiral' | 'regression' | 'slowdown' | 'smell';
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
    `work-item: 03-subscription-stress-blackbox`,
    `review: 03-unbounded-growth`,
    `technique: 9`,
    `class: subscription-stress`,
    `severity: ${opts.severity}`,
    `file: packages/tui/src/acp-client.ts`,
    `platforms-affected: [${PLATFORM}]`,
    `discovered-by: blackbox`,
    `discovered-at: ${new Date().toISOString()}`,
    `status: open`,
    '---',
  ].join('\n');

  const body = [
    `# ${opts.title}`,
    '',
    opts.description,
    opts.evidence ? `\n## Evidence\n\n${opts.evidence}` : '',
    opts.proposedFix ? `\n## Proposed fix\n\n${opts.proposedFix}` : '',
  ]
    .filter(Boolean)
    .join('\n');

  writeFileSync(path, `${frontmatter}\n\n${body}\n`);
  return path;
}

async function runOneCycle(
  index: number,
  binaryPath: string,
  binaryArgs: string[],
  tuiJsPath: string
): Promise<Cycle> {
  const cycleStart = Date.now();

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

  let childPeakRssKb = 0;
  let childExited = false;
  let childPid: number | undefined;

  try {
    ptyMgr.spawn(binaryPath, binaryArgs);
    childPid = ptyMgr.getPid();

    // Sample child RSS during the dwell window to capture its peak
    const dwellEnd = Date.now() + DWELL_MS;
    while (Date.now() < dwellEnd) {
      if (childPid) {
        const rss = getRssKb(childPid);
        if (rss > childPeakRssKb) childPeakRssKb = rss;
      }
      await Bun.sleep(50);
    }

    // Tear down. Favor graceful Ctrl+C twice → timeout → hard kill.
    try {
      await ptyMgr.sendKeys([0x03, 0x03]);
    } catch {
      /* ignore */
    }
    await Bun.sleep(200);
    if (childPid && isProcessAlive(childPid)) {
      ptyMgr.kill();
    }

    // Wait for the child to be fully gone (bounded)
    const exitDeadline = Date.now() + 2000;
    while (childPid && Date.now() < exitDeadline) {
      if (!isProcessAlive(childPid)) {
        childExited = true;
        break;
      }
      await Bun.sleep(50);
    }
    if (!childExited && childPid) {
      // Force — SIGKILL
      try {
        process.kill(childPid, 'SIGKILL');
      } catch {
        /* already gone */
      }
      await Bun.sleep(100);
      childExited = !isProcessAlive(childPid);
    }
  } catch {
    // spawn failed or similar — record and continue
    try {
      ptyMgr.kill();
    } catch {
      /* ignore */
    }
  }

  // Measure probe-process resources AFTER teardown
  const probeRssKb = getRssKb(process.pid);
  const probeFdCount = getFdCount(process.pid);

  return {
    index,
    spawnedAtMs: cycleStart,
    childPid,
    childPeakRssKb,
    childExited,
    probeRssKb,
    probeFdCount,
    elapsedMs: Date.now() - cycleStart,
  };
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const started = Date.now();

  // Resolve the chat_cli binary
  const binaryPath = process.env.CHAT_CLI_BIN ?? 'cargo';
  const binaryArgs =
    process.env.CHAT_CLI_BIN !== undefined
      ? ['chat', '--tui']
      : ['run', '-p', 'chat_cli', '--bin', 'chat_cli', '--', 'chat', '--tui'];
  const tuiJsPath =
    process.env.KIRO_TEST_TUI_JS_PATH ??
    join(process.cwd(), 'packages/tui/dist/tui.js');

  // Baseline probe-process resources before any cycles
  const baselineProbeRssKb = getRssKb(process.pid);
  const baselineProbeFdCount = getFdCount(process.pid);

  console.log(
    `[${PROBE_NAME}] baseline probe RSS=${(baselineProbeRssKb / 1024).toFixed(1)} MB, FD=${baselineProbeFdCount}`
  );
  console.log(`[${PROBE_NAME}] running ${CYCLES} cycle(s), ${DWELL_MS} ms dwell each`);

  const cycles: Cycle[] = [];
  for (let i = 0; i < CYCLES; i++) {
    const c = await runOneCycle(i, binaryPath, binaryArgs, tuiJsPath);
    cycles.push(c);
    if ((i + 1) % Math.max(1, Math.floor(CYCLES / 10)) === 0) {
      console.log(
        `[${PROBE_NAME}] cycle ${i + 1}/${CYCLES} — ` +
          `probe RSS=${(c.probeRssKb / 1024).toFixed(1)} MB, FD=${c.probeFdCount}, ` +
          `child peak=${(c.childPeakRssKb / 1024).toFixed(1)} MB`
      );
    }
  }

  // Analysis
  const elapsedMs = Date.now() - started;
  const finalCycle = cycles[cycles.length - 1];
  if (!finalCycle) {
    throw new Error('No cycles executed — cannot analyze');
  }
  const probeRssGrowthMb =
    (finalCycle.probeRssKb - baselineProbeRssKb) / 1024;
  const probeFdGrowth = finalCycle.probeFdCount - baselineProbeFdCount;
  const probeRssSlopeKbPerCycle = slopePerCycle(cycles.map((c) => c.probeRssKb));
  const probeFdSlopePerCycle = slopePerCycle(cycles.map((c) => c.probeFdCount));

  const childPeaks = cycles.map((c) => c.childPeakRssKb).filter((v) => v > 0);
  const firstChildRss = childPeaks[0] ?? 0;
  const maxChildRss = Math.max(...childPeaks, 0);
  const minChildRss = Math.min(...childPeaks, firstChildRss);
  const childRssDriftPct =
    firstChildRss > 0
      ? ((maxChildRss - firstChildRss) / firstChildRss) * 100
      : 0;

  const failedExits = cycles.filter((c) => !c.childExited).length;

  const findings: Array<{ slug: string; title: string; severity: 'spiral' | 'crash' }> = [];

  if (probeRssGrowthMb > PROBE_RSS_GROWTH_BUDGET_MB) {
    findings.push({
      slug: 'probe-rss-growth',
      title:
        `Probe-process RSS grew ${probeRssGrowthMb.toFixed(1)} MB over ${CYCLES} ` +
        `spawn/kill cycles (budget: ${PROBE_RSS_GROWTH_BUDGET_MB} MB)`,
      severity: 'spiral',
    });
  }
  if (probeFdGrowth > PROBE_FD_GROWTH_BUDGET) {
    findings.push({
      slug: 'probe-fd-leak',
      title: `Probe-process FD grew by ${probeFdGrowth} (budget: ${PROBE_FD_GROWTH_BUDGET})`,
      severity: 'spiral',
    });
  }
  if (childRssDriftPct > CHILD_RSS_DRIFT_BUDGET_PCT) {
    findings.push({
      slug: 'child-rss-drift',
      title:
        `Child steady-state RSS drifted ${childRssDriftPct.toFixed(1)} % from first iteration ` +
        `(budget: ${CHILD_RSS_DRIFT_BUDGET_PCT} %)`,
      severity: 'spiral',
    });
  }
  if (failedExits > 0) {
    findings.push({
      slug: 'orphaned-children',
      title: `${failedExits} child process(es) did not exit cleanly and required SIGKILL`,
      severity: 'spiral',
    });
  }

  for (const f of findings) {
    writeFinding({
      slug: f.slug,
      title: f.title,
      severity: f.severity,
      description: f.title,
      evidence: [
        `Cycles: ${CYCLES}`,
        `Dwell per cycle: ${DWELL_MS} ms`,
        `Total elapsed: ${(elapsedMs / 1000).toFixed(1)} s`,
        `Probe baseline RSS: ${(baselineProbeRssKb / 1024).toFixed(1)} MB`,
        `Probe final RSS: ${(finalCycle.probeRssKb / 1024).toFixed(1)} MB`,
        `Probe RSS growth: ${probeRssGrowthMb.toFixed(1)} MB`,
        `Probe RSS slope: ${probeRssSlopeKbPerCycle.toFixed(1)} KB/cycle`,
        `Probe baseline FD: ${baselineProbeFdCount}`,
        `Probe final FD: ${finalCycle.probeFdCount}`,
        `Probe FD growth: ${probeFdGrowth}`,
        `Probe FD slope: ${probeFdSlopePerCycle.toFixed(2)} per cycle`,
        `Child peak RSS — first: ${(firstChildRss / 1024).toFixed(1)} MB, ` +
          `min: ${(minChildRss / 1024).toFixed(1)} MB, ` +
          `max: ${(maxChildRss / 1024).toFixed(1)} MB`,
        `Child RSS drift: ${childRssDriftPct.toFixed(1)} %`,
        `Children requiring SIGKILL: ${failedExits}`,
        '',
        'Note: this probe cannot directly read EventEmitter listener counts',
        'from within each child (the TUI test-mode IPC does not currently',
        'expose a LISTENER_COUNTS command). Monotonic growth in probe-process',
        'resources is a correlated signal but not a substitute for in-process',
        'listener auditing.',
      ].join('\n'),
      proposedFix:
        'If probe RSS/FD grew: look at bun-pty cleanup on ptyMgr.kill(). ' +
        'If child RSS drifted: inspect lazy module loads or cached state in ' +
        'the TUI bundle that might persist via OS caches. ' +
        'To diagnose in-process listener leaks, add a LISTENER_COUNTS IPC ' +
        'command in src/test-utils/TestModeProvider.tsx that reports ' +
        '`(process as any)._getActiveHandles().length` and EventEmitter ' +
        'listener counts, then rerun this probe with per-cycle IPC sampling.',
    });
  }

  const metrics = {
    probe: PROBE_NAME,
    platform: PLATFORM,
    elapsedMs,
    cycles: CYCLES,
    dwellMs: DWELL_MS,
    baselineProbeRssKb,
    baselineProbeFdCount,
    finalProbeRssKb: finalCycle.probeRssKb,
    finalProbeFdCount: finalCycle.probeFdCount,
    probeRssGrowthMb: +probeRssGrowthMb.toFixed(2),
    probeFdGrowth,
    probeRssSlopeKbPerCycle: +probeRssSlopeKbPerCycle.toFixed(2),
    probeFdSlopePerCycle: +probeFdSlopePerCycle.toFixed(3),
    firstChildRssKb: firstChildRss,
    maxChildRssKb: maxChildRss,
    minChildRssKb: minChildRss,
    childRssDriftPct: +childRssDriftPct.toFixed(2),
    failedExits,
    cyclesDetail: cycles,
    budgets: {
      probeRssGrowthMb: PROBE_RSS_GROWTH_BUDGET_MB,
      probeFdGrowth: PROBE_FD_GROWTH_BUDGET,
      childRssDriftPct: CHILD_RSS_DRIFT_BUDGET_PCT,
    },
    pass: findings.length === 0,
  };
  writeFileSync(
    join(OUTPUT_DIR, `${PREFIX}-metrics.json`),
    JSON.stringify(metrics, null, 2)
  );

  writeFileSync(
    join(OUTPUT_DIR, `${PREFIX}-done.md`),
    [
      '---',
      `id: ${PREFIX}-done`,
      `work-item: 03-subscription-stress-blackbox`,
      `kind: blackbox`,
      `platform: ${PLATFORM}`,
      `status: done`,
      `findings-emitted: ${findings.length}`,
      `elapsed-ms: ${elapsedMs}`,
      `completed-at: ${new Date().toISOString()}`,
      '---',
      '',
      `# Probe done: ${PROBE_NAME} on ${PLATFORM}`,
      '',
      `Emitted ${findings.length} finding(s) after ${CYCLES} cycles in ${(elapsedMs / 1000).toFixed(1)} s.`,
      '',
    ].join('\n')
  );

  const pass = findings.length === 0;
  console.log(`\n[${PROBE_NAME}] ${pass ? '✅ PASS' : '❌ FAIL'} on ${PLATFORM}`);
  console.log(`  cycles:             ${CYCLES}`);
  console.log(`  probe RSS growth:   ${probeRssGrowthMb.toFixed(1)} MB`);
  console.log(`  probe FD growth:    ${probeFdGrowth}`);
  console.log(`  child RSS drift:    ${childRssDriftPct.toFixed(1)} %`);
  console.log(`  SIGKILL required:   ${failedExits} children`);
  if (!pass) for (const f of findings) console.log(`  • ${f.title}`);

  process.exit(pass ? 0 : 1);
}

try {
  await main();
} catch (err) {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const errorPath = join(OUTPUT_DIR, `${PREFIX}-error.log`);
  writeFileSync(
    errorPath,
    String(err instanceof Error ? (err.stack ?? err.message) : err)
  );
  console.error(`[${PROBE_NAME}] probe crashed on ${PLATFORM}:`);
  console.error(err);
  process.exit(2);
}

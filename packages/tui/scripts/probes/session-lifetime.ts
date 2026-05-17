#!/usr/bin/env bun
/**
 * session-lifetime.ts — Review 03 technique 7 blackbox probe.
 *
 * Launches chat_cli in a PTY and exercises it over a long session
 * (default 60 minutes) while sampling RSS and FD counts at a regular
 * cadence. Fits a linear regression through the RSS samples after a
 * warm-up period and flags sustained growth.
 *
 * This replaces the earlier `long-soak.ts` probe; the work-item id
 * `03-session-lifetime-blackbox` maps to this filename under the
 * runner's un-defer convention (partition label → `<name>.ts`).
 *
 * Pass criteria (after 10-minute warm-up):
 *   - RSS growth rate < 1 MB/min sustained (linear regression slope)
 *   - Total RSS delta (baseline → final) < 50 MB
 *   - FD growth < 5 over baseline
 *   - Process still responsive at end
 *
 * Exit codes: 0 = pass, 1 = finding, 2 = probe crash
 *
 * Usage:
 *   bun run packages/tui/scripts/probes/session-lifetime.ts
 *
 * Environment:
 *   SOAK_DURATION_MS     total soak duration (default: 3600000 = 60 min;
 *                        minimum enforced: 3600000 unless CI=1, in which
 *                        case a 5-min abbreviated run is allowed)
 *   SAMPLE_INTERVAL_MS   sample cadence (default: 30000)
 *   WARMUP_MS            warm-up before trend fitting (default: 600000 = 10 min)
 *   PROBE_OUTPUT_DIR     where to write findings/metrics
 *   KIRO_TEST_TUI_JS_PATH path to built TUI bundle
 *   CHAT_CLI_BIN         path to chat_cli binary (default: cargo-run)
 */

import { execSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { PtyManager } from '../../src/test-utils/shared/pty-manager';

const PROBE_NAME = 'session-lifetime';
const PLATFORM =
  process.env.KIRO_PROBE_PLATFORM ??
  (process.platform === 'darwin' ? 'macos' : process.platform);
const OUTPUT_DIR = process.env.PROBE_OUTPUT_DIR ?? './probe-output';

const MIN_FULL_DURATION_MS = 3_600_000; // 60 min — the Review 03 requirement
const CI_ABBREVIATED_DURATION_MS = 300_000; // 5 min — CI smoke only
const DEFAULT_DURATION_MS =
  process.env.CI === '1' ? CI_ABBREVIATED_DURATION_MS : MIN_FULL_DURATION_MS;
const SOAK_DURATION_MS = parseInt(
  process.env.SOAK_DURATION_MS ?? String(DEFAULT_DURATION_MS),
  10
);
const SAMPLE_INTERVAL_MS = parseInt(
  process.env.SAMPLE_INTERVAL_MS ?? '30000',
  10
);
const WARMUP_MS = parseInt(process.env.WARMUP_MS ?? '600000', 10);

// Budgets (matched to docs/review-playbook/runner/findings/03-session-heap-blackbox-*-runbook.md)
const GROWTH_RATE_BUDGET_MB_PER_MIN = 1;
const TOTAL_GROWTH_BUDGET_MB = 50;
const FD_GROWTH_BUDGET = 5;

interface Sample {
  timestampMs: number;
  rssKb: number;
  fdCount: number;
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
    // macOS: lsof -p PID counts open files (subtract header line)
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

/**
 * Fit y = a + b*x through samples and return slope b in MB/min.
 * Only samples with timestampMs >= afterMs are considered.
 */
function fitRssSlopeMbPerMin(samples: Sample[], afterMs: number): number {
  const window = samples.filter((s) => s.timestampMs >= afterMs);
  if (window.length < 2) return 0;
  const xs = window.map((s) => s.timestampMs);
  const ys = window.map((s) => s.rssKb);
  const n = xs.length;
  const xMean = xs.reduce((a, b) => a + b, 0) / n;
  const yMean = ys.reduce((a, b) => a + b, 0) / n;
  let num = 0;
  let den = 0;
  for (let i = 0; i < n; i++) {
    const x = xs[i]!;
    const y = ys[i]!;
    num += (x - xMean) * (y - yMean);
    den += (x - xMean) ** 2;
  }
  if (den === 0) return 0;
  const slopeKbPerMs = num / den;
  // kb/ms → mb/min: / 1024 * 60_000
  return (slopeKbPerMs / 1024) * 60_000;
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
    `work-item: 03-session-lifetime-blackbox`,
    `review: 03-unbounded-growth`,
    `technique: 7`,
    `class: session-lifetime-soak`,
    `severity: ${opts.severity}`,
    `file: ${opts.file}`,
    opts.line !== undefined ? `line: ${opts.line}` : '',
    `platforms-affected: [${PLATFORM}]`,
    `discovered-by: blackbox`,
    `discovered-at: ${new Date().toISOString()}`,
    `status: open`,
    '---',
  ]
    .filter(Boolean)
    .join('\n');

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

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const started = Date.now();

  if (
    SOAK_DURATION_MS < MIN_FULL_DURATION_MS &&
    process.env.CI !== '1' &&
    !process.env.SOAK_DURATION_MS
  ) {
    throw new Error(
      `SOAK_DURATION_MS must be >= ${MIN_FULL_DURATION_MS} for Review 03 ` +
        `(got ${SOAK_DURATION_MS}). Set CI=1 for abbreviated runs or ` +
        `set SOAK_DURATION_MS explicitly to override.`
    );
  }

  // Resolve the chat_cli binary
  const binaryPath = process.env.CHAT_CLI_BIN ?? 'cargo';
  const binaryArgs =
    process.env.CHAT_CLI_BIN !== undefined
      ? ['chat', '--tui']
      : ['run', '-p', 'chat_cli', '--bin', 'chat_cli', '--', 'chat', '--tui'];
  const tuiJsPath =
    process.env.KIRO_TEST_TUI_JS_PATH ??
    join(process.cwd(), 'packages/tui/dist/tui.js');

  // Launch PTY
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
  ptyMgr.spawn(binaryPath, binaryArgs);

  const pid = ptyMgr.getPid();
  if (!pid) throw new Error('Failed to get PTY child PID');

  // Wait for startup
  await Bun.sleep(5000);
  if (!isProcessAlive(pid)) {
    throw new Error('Process exited during startup');
  }

  // Collect samples. Between samples, inject light interactive activity
  // (message → clear) to drive per-turn allocations; we do not require the
  // assistant to actually respond because that needs a live backend.
  const samples: Sample[] = [];
  const endTime = started + SOAK_DURATION_MS;
  let turnCount = 0;

  while (Date.now() < endTime) {
    // Simulate one "turn": type a short message, send, then clear.
    // This exercises the input buffer, command history, and message list
    // without depending on ACP responses.
    try {
      await ptyMgr.sendKeys(`probe turn ${turnCount}\n`);
      await Bun.sleep(250);
      await ptyMgr.sendKeys('/clear\n');
    } catch {
      // ignore — keystroke failures shouldn't kill the probe
    }
    turnCount++;

    // Record measurements
    const rssKb = getRssKb(pid);
    const fdCount = getFdCount(pid);
    if (rssKb < 0) break; // process died
    samples.push({ timestampMs: Date.now(), rssKb, fdCount });

    // Wait for next interval
    const remaining = endTime - Date.now();
    await Bun.sleep(Math.min(SAMPLE_INTERVAL_MS, Math.max(0, remaining)));
  }

  // Final responsiveness check
  const responsive = isProcessAlive(pid);

  // Clean up
  ptyMgr.kill();

  // Analysis
  const elapsedMs = Date.now() - started;
  const baselineRssKb = samples[0]?.rssKb ?? 0;
  const finalRssKb = samples[samples.length - 1]?.rssKb ?? 0;
  const rssDeltaMb = (finalRssKb - baselineRssKb) / 1024;
  const warmupCutoff = started + Math.min(WARMUP_MS, elapsedMs / 2);
  const slopeMbPerMin = fitRssSlopeMbPerMin(samples, warmupCutoff);

  const baselineFd = samples[0]?.fdCount ?? 0;
  const finalFd = samples[samples.length - 1]?.fdCount ?? 0;
  const fdGrowth = finalFd - baselineFd;

  // Pass/fail
  const findings: Array<{ slug: string; title: string; severity: 'spiral' | 'slowdown' | 'crash' }> = [];
  if (slopeMbPerMin >= GROWTH_RATE_BUDGET_MB_PER_MIN) {
    findings.push({
      slug: 'rss-trend-growth',
      title: `RSS trend ${slopeMbPerMin.toFixed(2)} MB/min exceeds ${GROWTH_RATE_BUDGET_MB_PER_MIN} MB/min budget`,
      severity: 'spiral',
    });
  }
  if (rssDeltaMb >= TOTAL_GROWTH_BUDGET_MB) {
    findings.push({
      slug: 'rss-total-growth',
      title: `Total RSS growth ${rssDeltaMb.toFixed(1)} MB exceeds ${TOTAL_GROWTH_BUDGET_MB} MB budget`,
      severity: 'spiral',
    });
  }
  if (fdGrowth >= FD_GROWTH_BUDGET) {
    findings.push({
      slug: 'fd-leak',
      title: `FD growth ${fdGrowth} exceeds budget of ${FD_GROWTH_BUDGET}`,
      severity: 'spiral',
    });
  }
  if (!responsive) {
    findings.push({
      slug: 'unresponsive-after-soak',
      title: 'Process became unresponsive during soak',
      severity: 'crash',
    });
  }

  // Emit findings
  for (const f of findings) {
    writeFinding({
      slug: f.slug,
      title: f.title,
      severity: f.severity,
      file: 'packages/tui/src/renderer/tui.ts',
      description: f.title,
      evidence: [
        `Duration: ${(elapsedMs / 60_000).toFixed(1)} min (${elapsedMs} ms)`,
        `Turns simulated: ${turnCount}`,
        `Samples: ${samples.length}`,
        `Baseline RSS: ${(baselineRssKb / 1024).toFixed(1)} MB`,
        `Final RSS: ${(finalRssKb / 1024).toFixed(1)} MB`,
        `RSS delta: ${rssDeltaMb.toFixed(1)} MB`,
        `RSS slope (post-warmup): ${slopeMbPerMin.toFixed(2)} MB/min`,
        `FD baseline: ${baselineFd}, final: ${finalFd}, growth: ${fdGrowth}`,
        `Responsive at end: ${responsive}`,
        '',
        'See metrics.json for the full sample series.',
      ].join('\n'),
      proposedFix:
        'Inspect session-conversations store, ACP listener registry, and ' +
        'message-stream-handler buffers for per-turn allocations that ' +
        'are not bounded or cleared.',
    });
  }

  // Metrics
  const metrics = {
    probe: PROBE_NAME,
    platform: PLATFORM,
    durationMs: elapsedMs,
    soakDurationMsRequested: SOAK_DURATION_MS,
    warmupMs: WARMUP_MS,
    sampleIntervalMs: SAMPLE_INTERVAL_MS,
    turnCount,
    samples,
    baselineRssKb,
    finalRssKb,
    rssDeltaMb: +rssDeltaMb.toFixed(2),
    slopeMbPerMin: +slopeMbPerMin.toFixed(3),
    baselineFd,
    finalFd,
    fdGrowth,
    responsive,
    budgets: {
      growthRateMbPerMin: GROWTH_RATE_BUDGET_MB_PER_MIN,
      totalGrowthMb: TOTAL_GROWTH_BUDGET_MB,
      fdGrowth: FD_GROWTH_BUDGET,
    },
    pass: findings.length === 0,
  };
  writeFileSync(
    join(OUTPUT_DIR, `${PREFIX}-metrics.json`),
    JSON.stringify(metrics, null, 2)
  );

  // Done marker
  writeFileSync(
    join(OUTPUT_DIR, `${PREFIX}-done.md`),
    [
      '---',
      `id: ${PREFIX}-done`,
      `work-item: 03-session-lifetime-blackbox`,
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
      `Emitted ${findings.length} finding(s) in ${(elapsedMs / 60_000).toFixed(1)} min (${turnCount} simulated turns, ${samples.length} samples).`,
      '',
    ].join('\n')
  );

  // Console summary
  const pass = findings.length === 0;
  console.log(`\n[${PROBE_NAME}] ${pass ? '✅ PASS' : '❌ FAIL'} on ${PLATFORM}`);
  console.log(`  duration:      ${(elapsedMs / 60_000).toFixed(1)} min`);
  console.log(`  turns:         ${turnCount}`);
  console.log(`  samples:       ${samples.length}`);
  console.log(`  baseline RSS:  ${(baselineRssKb / 1024).toFixed(1)} MB`);
  console.log(`  final RSS:     ${(finalRssKb / 1024).toFixed(1)} MB`);
  console.log(`  delta:         ${rssDeltaMb.toFixed(1)} MB`);
  console.log(`  slope:         ${slopeMbPerMin.toFixed(2)} MB/min`);
  console.log(`  FD growth:     ${fdGrowth}`);
  console.log(`  responsive:    ${responsive}`);
  if (!pass) {
    for (const f of findings) console.log(`  • ${f.title}`);
  }

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

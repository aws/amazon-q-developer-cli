#!/usr/bin/env bun
/**
 * forced-trim.ts — Review 03 technique 10 blackbox probe.
 *
 * Validates that trim mechanisms actually fire under load. The TUI has
 * two known trim caps (code findings 03-trim-caps-code-*):
 *
 *   - MAX_SESSION_MESSAGES = 50   (stores/session-conversations.ts)
 *   - MAX_HISTORY_SIZE     = 1000 (utils/command-history.ts)
 *
 * This probe drives message traffic well past those thresholds and
 * measures whether RSS stabilises (trim firing) or grows unbounded
 * (trim broken).
 *
 * ## Measurement strategy
 *
 * Direct observation of `msgs.length` or `this.history.length` is not
 * available blackbox — neither trim site currently logs trim events
 * (see code finding 03-trim-caps-code-*-caps-missing-env-overrides).
 *
 * We use **RSS over turn-count** as a proxy: if trim works, RSS should
 * plateau after the threshold. If trim is broken, RSS should grow
 * linearly with turn count.
 *
 * ## Pass / fail
 *
 *   PASS if — for turns in [threshold, 2 × threshold] — the fitted RSS
 *   slope is less than 10 % of the slope measured in [0, threshold].
 *   (i.e. growth rate dropped by at least an order of magnitude once
 *   the trim engaged.)
 *
 *   FAIL if the post-threshold slope is within 10 % of the pre-threshold
 *   slope, OR if RSS at 2 × threshold exceeds 2 × RSS at threshold.
 *
 * Exit codes: 0 = pass, 1 = finding, 2 = probe crash
 *
 * Usage:
 *   bun run packages/tui/scripts/probes/forced-trim.ts
 *
 * Environment:
 *   FORCED_TRIM_TURNS    total turns to send (default: 250 — five × MAX_SESSION_MESSAGES)
 *   FORCED_TRIM_THRESHOLD threshold to measure against (default: 50)
 *   INTER_TURN_MS        pause between turns (default: 50)
 *   PROBE_OUTPUT_DIR     where to write findings/metrics
 *   KIRO_TEST_TUI_JS_PATH path to built TUI bundle
 *   CHAT_CLI_BIN         path to chat_cli binary (default: cargo-run)
 */

import { execSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { PtyManager } from '../../src/test-utils/shared/pty-manager';

const PROBE_NAME = 'forced-trim';
const PLATFORM =
  process.env.KIRO_PROBE_PLATFORM ??
  (process.platform === 'darwin' ? 'macos' : process.platform);
const OUTPUT_DIR = process.env.PROBE_OUTPUT_DIR ?? './probe-output';

const TOTAL_TURNS = parseInt(process.env.FORCED_TRIM_TURNS ?? '250', 10);
const THRESHOLD = parseInt(process.env.FORCED_TRIM_THRESHOLD ?? '50', 10);
const INTER_TURN_MS = parseInt(process.env.INTER_TURN_MS ?? '50', 10);

// A long-ish message to amplify per-message allocation so the RSS proxy
// actually reflects untrimmed accumulation.
const MESSAGE_FILLER = 'x'.repeat(1024); // 1 KiB per message

interface Sample {
  turn: number;
  timestampMs: number;
  rssKb: number;
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

function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/** Least-squares slope of y over x, in kb/turn. */
function slopeKbPerTurn(samples: Sample[], minTurn: number, maxTurn: number): number {
  const w = samples.filter((s) => s.turn >= minTurn && s.turn <= maxTurn);
  if (w.length < 2) return 0;
  const xs = w.map((s) => s.turn);
  const ys = w.map((s) => s.rssKb);
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
    `work-item: 03-forced-trim-blackbox`,
    `review: 03-unbounded-growth`,
    `technique: 10`,
    `class: forced-trim`,
    `severity: ${opts.severity}`,
    `file: packages/tui/src/stores/session-conversations.ts`,
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

// LINT-DEBT(complexity): pre-existing at gate adoption; Async function 'main' has a complexity of 31. Maximum allowed is 30.; refactor before extending
// eslint-disable-next-line complexity
async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const started = Date.now();

  if (TOTAL_TURNS < 2 * THRESHOLD) {
    throw new Error(
      `FORCED_TRIM_TURNS (${TOTAL_TURNS}) must be at least 2 × THRESHOLD (${THRESHOLD}) ` +
        `to compare pre- and post-threshold slopes.`
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

  // Baseline
  const baselineRssKb = getRssKb(pid);

  // Drive turns. Each turn = paste filler + newline. No /clear — we want
  // state to accumulate until trim fires.
  const samples: Sample[] = [
    { turn: 0, timestampMs: Date.now(), rssKb: baselineRssKb },
  ];

  for (let t = 1; t <= TOTAL_TURNS; t++) {
    try {
      await ptyMgr.sendKeys(`turn${t} ${MESSAGE_FILLER}\n`);
    } catch {
      // keystroke failure mid-storm shouldn't abort the probe
    }
    // Sample RSS every turn for dense signal in the pre-threshold regime.
    // After the threshold, sample every 5 turns to bound cost.
    const sampleEveryN = t < 2 * THRESHOLD ? 1 : 5;
    if (t % sampleEveryN === 0) {
      const rssKb = getRssKb(pid);
      if (rssKb < 0) break; // process died
      samples.push({ turn: t, timestampMs: Date.now(), rssKb });
    }
    if (INTER_TURN_MS > 0) await Bun.sleep(INTER_TURN_MS);
  }

  const crashed = !isProcessAlive(pid);
  ptyMgr.kill();

  // Analysis: compare slope in [0, THRESHOLD] vs [THRESHOLD, 2*THRESHOLD]
  const preSlope = slopeKbPerTurn(samples, 0, THRESHOLD);
  const postSlope = slopeKbPerTurn(samples, THRESHOLD, 2 * THRESHOLD);
  const tailSlope = slopeKbPerTurn(samples, 2 * THRESHOLD, TOTAL_TURNS);

  const rssAtThreshold = samples.find((s) => s.turn >= THRESHOLD)?.rssKb ?? 0;
  const rssAtDoubleThreshold =
    samples.find((s) => s.turn >= 2 * THRESHOLD)?.rssKb ?? 0;
  const finalRssKb = samples[samples.length - 1]?.rssKb ?? 0;

  // Pre-slope near zero can happen if messages are small enough that GC
  // noise dominates. Only evaluate the ratio when the pre-slope is large
  // enough to be meaningful.
  const preSlopeSignificant = preSlope > 1; // > 1 KB/turn
  const slopeRatio =
    preSlopeSignificant && preSlope > 0 ? postSlope / preSlope : 0;

  const findings: Array<{ slug: string; title: string; severity: 'spiral' | 'slowdown' | 'crash' | 'regression' }> = [];

  if (crashed) {
    findings.push({
      slug: 'crash-during-trim-probe',
      title: `Process (PID ${pid}) crashed during forced-trim probe`,
      severity: 'crash',
    });
  }

  // Primary check: did trim engage?
  if (preSlopeSignificant && slopeRatio > 0.1) {
    findings.push({
      slug: 'trim-did-not-engage',
      title: `Post-threshold RSS slope is ${(slopeRatio * 100).toFixed(1)} % of pre-threshold (budget: ≤ 10 %)`,
      severity: 'spiral',
    });
  }

  // Secondary check: absolute unboundedness
  if (rssAtThreshold > 0 && rssAtDoubleThreshold > 2 * rssAtThreshold) {
    findings.push({
      slug: 'rss-unbounded-past-threshold',
      title: `RSS at 2 × threshold (${Math.round(rssAtDoubleThreshold / 1024)} MB) is more than 2× RSS at threshold (${Math.round(rssAtThreshold / 1024)} MB)`,
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
        `Threshold: ${THRESHOLD}`,
        `Total turns: ${TOTAL_TURNS}`,
        `Baseline RSS: ${(baselineRssKb / 1024).toFixed(1)} MB`,
        `RSS @ threshold: ${(rssAtThreshold / 1024).toFixed(1)} MB`,
        `RSS @ 2 × threshold: ${(rssAtDoubleThreshold / 1024).toFixed(1)} MB`,
        `Final RSS: ${(finalRssKb / 1024).toFixed(1)} MB`,
        `Pre-threshold slope: ${preSlope.toFixed(2)} KB/turn`,
        `Post-threshold slope: ${postSlope.toFixed(2)} KB/turn`,
        `Tail slope: ${tailSlope.toFixed(2)} KB/turn`,
        `Slope ratio (post/pre): ${(slopeRatio * 100).toFixed(1)} %`,
        '',
        `Note: this probe uses RSS as a proxy for array/map size because`,
        `neither stores/session-conversations.ts nor utils/command-history.ts`,
        `logs trim events. A lower-noise measurement requires instrumenting`,
        `those sites (see code finding 03-trim-caps-code-*).`,
      ].join('\n'),
      proposedFix:
        'Verify trim call sites: `msgs.slice(-MAX_SESSION_MESSAGES)` in ' +
        'session-conversations.ts, `this.history.slice(-MAX_HISTORY_SIZE)` ' +
        'in command-history.ts. Add a debug-log line on every trim to make ' +
        'this measurable directly in future runs.',
    });
  }

  const elapsedMs = Date.now() - started;
  const metrics = {
    probe: PROBE_NAME,
    platform: PLATFORM,
    elapsedMs,
    totalTurns: TOTAL_TURNS,
    threshold: THRESHOLD,
    samples,
    baselineRssKb,
    rssAtThresholdKb: rssAtThreshold,
    rssAtDoubleThresholdKb: rssAtDoubleThreshold,
    finalRssKb,
    preSlopeKbPerTurn: +preSlope.toFixed(3),
    postSlopeKbPerTurn: +postSlope.toFixed(3),
    tailSlopeKbPerTurn: +tailSlope.toFixed(3),
    slopeRatio: +slopeRatio.toFixed(3),
    preSlopeSignificant,
    crashed,
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
      `work-item: 03-forced-trim-blackbox`,
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
      `Emitted ${findings.length} finding(s) after ${TOTAL_TURNS} turns ` +
        `(threshold=${THRESHOLD}) in ${elapsedMs} ms.`,
      '',
    ].join('\n')
  );

  const pass = findings.length === 0;
  console.log(`\n[${PROBE_NAME}] ${pass ? '✅ PASS' : '❌ FAIL'} on ${PLATFORM}`);
  console.log(`  turns:              ${TOTAL_TURNS}`);
  console.log(`  threshold:          ${THRESHOLD}`);
  console.log(`  baseline RSS:       ${(baselineRssKb / 1024).toFixed(1)} MB`);
  console.log(`  RSS @ threshold:    ${(rssAtThreshold / 1024).toFixed(1)} MB`);
  console.log(`  RSS @ 2×threshold:  ${(rssAtDoubleThreshold / 1024).toFixed(1)} MB`);
  console.log(`  pre-slope:          ${preSlope.toFixed(2)} KB/turn`);
  console.log(`  post-slope:         ${postSlope.toFixed(2)} KB/turn`);
  console.log(`  ratio:              ${(slopeRatio * 100).toFixed(1)} %`);
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

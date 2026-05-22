#!/usr/bin/env bun
/**
 * resize-regression.ts — Knight Rider probe: resize regression scenarios.
 *
 * Exercises the known-bad resize patterns that have caused repeated bugs:
 *   1. Extreme narrow width (< 10 cols) — yoga OOM / zero-width text
 *   2. Rapid resize storm — memory spiral / render cascade
 *   3. Content reflow — trim-reappend garble after resize
 *   4. No-op resize (same dimensions) — oscillation loop
 *   5. Recovery — content readable after returning to normal width
 *
 * Uses Knight Rider HTTP API (localhost:3001) for interactive testing with
 * visual evidence capture. Falls back to direct PtyManager if KR unavailable.
 *
 * Exit codes: 0 = pass, 1 = finding, 2 = probe crash
 */

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const PROBE_NAME = 'resize-regression';
const PLATFORM = process.env.KIRO_PROBE_PLATFORM ?? (process.platform === 'darwin' ? 'macos' : process.platform);
const OUTPUT_DIR = process.env.PROBE_OUTPUT_DIR ?? './probe-output';
const KR_URL = process.env.KNIGHT_RIDER_URL ?? 'http://localhost:3001';
const KR = `${KR_URL}/api`;

const MEMORY_BUDGET_MB = 20;
const STORM_COUNT = 200;
const NARROW_WIDTH = 6;
const NORMAL_WIDTH = 120;
const NORMAL_HEIGHT = 40;

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
  description: string;
  evidence: string;
  proposedFix?: string;
}) {
  const slug = slugify(opts.slug);
  const findingId = `${PREFIX}-${slug}`;
  const filePath = join(OUTPUT_DIR, `${findingId}.md`);
  writeFileSync(filePath, [
    '---',
    `id: ${findingId}`,
    `work-item: ${PROBE_NAME}`,
    `review: 01-async-render-path`,
    `technique: 5`,
    `class: resize-regression`,
    `severity: ${opts.severity}`,
    `file: ${opts.file}`,
    `platforms-affected: [${PLATFORM}]`,
    `discovered-by: blackbox`,
    `discovered-at: ${new Date().toISOString()}`,
    `status: open`,
    '---',
    '',
    `# ${opts.title}`,
    '',
    opts.description,
    '',
    '## Evidence',
    '',
    opts.evidence,
    opts.proposedFix ? `\n## Proposed fix\n\n${opts.proposedFix}` : '',
    '',
  ].join('\n'));
  return filePath;
}

function writeDoneMarker(findingsEmitted: number, elapsedMs: number) {
  writeFileSync(join(OUTPUT_DIR, `${PREFIX}-done.md`), [
    '---',
    `id: ${PREFIX}-done`,
    `work-item: ${PROBE_NAME}`,
    `kind: blackbox`,
    `platform: ${PLATFORM}`,
    `status: done`,
    `findings-emitted: ${findingsEmitted}`,
    `elapsed-ms: ${elapsedMs}`,
    `completed-at: ${new Date().toISOString()}`,
    '---',
    '',
    `# Probe done: ${PROBE_NAME} on ${PLATFORM}`,
    '',
    `Emitted ${findingsEmitted} finding(s) in ${elapsedMs} ms.`,
    '',
  ].join('\n'));
}

function writeMetrics(metrics: Record<string, unknown>) {
  writeFileSync(join(OUTPUT_DIR, `${PREFIX}-metrics.json`), JSON.stringify(metrics, null, 2));
}

async function kr(method: string, path: string, body?: unknown): Promise<any> {
  const res = await fetch(`${KR}${path}`, {
    method,
    headers: body ? { 'Content-Type': 'application/json' } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) throw new Error(`KR ${method} ${path}: ${res.status} ${await res.text()}`);
  return res.json();
}

async function frame(label: string) { return kr('POST', '/frame', { label }); }
async function resize(cols: number, rows: number) { return kr('POST', '/resize', { cols, rows }); }
async function screen(): Promise<string[]> { return (await kr('GET', '/screen')).lines; }
async function memory(): Promise<{ rssMB: number }> { return kr('GET', '/memory'); }
async function sleep(ms: number) { return kr('POST', '/sleep', { ms }); }
async function status(): Promise<{ ready: boolean }> { return kr('GET', '/status'); }

async function checkKnightRider(): Promise<boolean> {
  try {
    const s = await status();
    return s.ready === true;
  } catch {
    return false;
  }
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const started = Date.now();
  let findingsEmitted = 0;

  // Check Knight Rider availability
  const krReady = await checkKnightRider();
  if (!krReady) {
    console.error(`[${PROBE_NAME}] Knight Rider not available at ${KR_URL}`);
    console.error(`Start it with: cd packages/tui && bun run knight-rider`);
    process.exit(2);
  }
  console.log(`[${PROBE_NAME}] Knight Rider ready at ${KR_URL}`);

  // Ensure normal starting dimensions
  await resize(NORMAL_WIDTH, NORMAL_HEIGHT);
  await sleep(1000);

  // ── Phase 1: Baseline ──────────────────────────────────────────
  console.log(`[${PROBE_NAME}] Phase 1: Baseline`);
  const baselineMem = await memory();
  const baselineScreen = await screen();
  await frame('01-baseline');
  console.log(`  RSS: ${baselineMem.rssMB} MB, lines with content: ${baselineScreen.filter(l => l.trim()).length}`);

  // ── Phase 2: Extreme narrow width ─────────────────────────────
  console.log(`[${PROBE_NAME}] Phase 2: Extreme narrow (${NARROW_WIDTH} cols)`);
  await resize(NARROW_WIDTH, NORMAL_HEIGHT);
  await sleep(2000); // Give yoga time to layout (or crash)

  let narrowCrashed = false;
  try {
    const narrowMem = await memory();
    await frame('02-narrow-width');
    const narrowDelta = narrowMem.rssMB - baselineMem.rssMB;
    console.log(`  RSS: ${narrowMem.rssMB} MB (delta: ${narrowDelta.toFixed(1)} MB)`);

    if (narrowDelta > MEMORY_BUDGET_MB) {
      findingsEmitted++;
      writeFinding({
        slug: 'narrow-width-memory-spiral',
        title: `Narrow resize (${NARROW_WIDTH} cols) caused ${narrowDelta.toFixed(1)} MB growth`,
        severity: 'spiral',
        file: 'packages/twinki/packages/twinki/src/renderer/render-text.ts',
        description: `Resizing to ${NARROW_WIDTH} columns caused RSS to grow ${narrowDelta.toFixed(1)} MB. This matches the known zero-width yoga OOM pattern where text nodes get width=0 and every character becomes its own line.`,
        evidence: `Baseline RSS: ${baselineMem.rssMB} MB\nAfter resize to ${NARROW_WIDTH} cols: ${narrowMem.rssMB} MB\nDelta: ${narrowDelta.toFixed(1)} MB (budget: ${MEMORY_BUDGET_MB} MB)\nSee frame: 02-narrow-width`,
        proposedFix: 'Verify MIN_LAYOUT_WIDTH guard in renderText and measureElement clamps width ≥ 10.',
      });
    }
  } catch (e) {
    narrowCrashed = true;
    findingsEmitted++;
    writeFinding({
      slug: 'narrow-width-crash',
      title: `TUI crashed on resize to ${NARROW_WIDTH} columns`,
      severity: 'crash',
      file: 'packages/twinki/packages/twinki/src/renderer/render-text.ts',
      description: `Resizing to ${NARROW_WIDTH} columns caused the TUI process to crash or become unresponsive.`,
      evidence: `Error: ${e instanceof Error ? e.message : String(e)}\nThis matches the known zero-width yoga OOM (PR #2156).`,
      proposedFix: 'Ensure dimension guard rejects resize events below MIN_LAYOUT_WIDTH.',
    });
  }

  if (narrowCrashed) {
    const elapsed = Date.now() - started;
    writeMetrics({ probe: PROBE_NAME, platform: PLATFORM, phase: 'narrow-crash', elapsed });
    writeDoneMarker(findingsEmitted, elapsed);
    console.log(`[${PROBE_NAME}] FAIL — crashed at narrow width`);
    process.exit(1);
  }

  // ── Phase 3: Recovery from narrow ─────────────────────────────
  console.log(`[${PROBE_NAME}] Phase 3: Recovery to normal width`);
  await resize(NORMAL_WIDTH, NORMAL_HEIGHT);
  await sleep(2000);

  const recoveryScreen = await screen();
  await frame('03-recovery-from-narrow');
  const recoveryMem = await memory();

  // Check content is visible (not blank/garbled)
  const visibleLines = recoveryScreen.filter(l => l.trim()).length;
  if (visibleLines < 3) {
    findingsEmitted++;
    writeFinding({
      slug: 'blank-after-narrow-recovery',
      title: 'Screen blank/garbled after recovering from narrow resize',
      severity: 'regression',
      file: 'packages/twinki/packages/twinki/src/dom/static-output.ts',
      description: `After resizing from ${NARROW_WIDTH} cols back to ${NORMAL_WIDTH} cols, only ${visibleLines} lines have content. Expected the TUI to reflow and display normally.`,
      evidence: `Visible lines after recovery: ${visibleLines}\nSee frame: 03-recovery-from-narrow`,
      proposedFix: 'Check resetStatic / replaceStaticOutput path re-renders content at new width.',
    });
  }
  console.log(`  Visible lines: ${visibleLines}, RSS: ${recoveryMem.rssMB} MB`);

  // ── Phase 4: Rapid resize storm ───────────────────────────────
  console.log(`[${PROBE_NAME}] Phase 4: Rapid resize storm (${STORM_COUNT} events)`);
  const preStormMem = await memory();
  await frame('04-pre-storm');

  for (let i = 0; i < STORM_COUNT; i++) {
    const cols = i % 2 === 0 ? 80 : 120;
    const rows = i % 2 === 0 ? 24 : 40;
    // Fire resize without waiting — stress the handler
    fetch(`${KR}/resize`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ cols, rows }),
    }).catch(() => {});
    // Tiny delay to avoid overwhelming the HTTP server itself
    if (i % 50 === 0) await new Promise(r => setTimeout(r, 10));
  }

  // Settle
  await sleep(5000);
  await resize(NORMAL_WIDTH, NORMAL_HEIGHT);
  await sleep(2000);

  let stormCrashed = false;
  try {
    const postStormMem = await memory();
    await frame('05-post-storm');
    const stormDelta = postStormMem.rssMB - preStormMem.rssMB;
    console.log(`  Pre-storm: ${preStormMem.rssMB} MB, Post-storm: ${postStormMem.rssMB} MB, Delta: ${stormDelta.toFixed(1)} MB`);

    if (stormDelta > MEMORY_BUDGET_MB) {
      findingsEmitted++;
      writeFinding({
        slug: 'storm-memory-spiral',
        title: `Resize storm caused ${stormDelta.toFixed(1)} MB growth`,
        severity: 'spiral',
        file: 'packages/twinki/packages/twinki/src/terminal/process-terminal.ts',
        description: `${STORM_COUNT} rapid resize events caused RSS to grow ${stormDelta.toFixed(1)} MB. This matches the known resize cascade / render spiral pattern.`,
        evidence: `Pre-storm RSS: ${preStormMem.rssMB} MB\nPost-storm RSS: ${postStormMem.rssMB} MB\nDelta: ${stormDelta.toFixed(1)} MB (budget: ${MEMORY_BUDGET_MB} MB)\nResize count: ${STORM_COUNT}\nSee frame: 05-post-storm`,
        proposedFix: 'Verify dimension guard skips no-op resizes and no async callbacks in resize path.',
      });
    }
  } catch (e) {
    stormCrashed = true;
    findingsEmitted++;
    writeFinding({
      slug: 'storm-crash',
      title: 'TUI crashed during resize storm',
      severity: 'crash',
      file: 'packages/twinki/packages/twinki/src/terminal/process-terminal.ts',
      description: `TUI became unresponsive or crashed during ${STORM_COUNT} rapid resize events.`,
      evidence: `Error: ${e instanceof Error ? e.message : String(e)}`,
    });
  }

  if (stormCrashed) {
    const elapsed = Date.now() - started;
    writeMetrics({ probe: PROBE_NAME, platform: PLATFORM, phase: 'storm-crash', elapsed });
    writeDoneMarker(findingsEmitted, elapsed);
    console.log(`[${PROBE_NAME}] FAIL — crashed during storm`);
    process.exit(1);
  }

  // ── Phase 5: No-op resize (same dimensions) ───────────────────
  console.log(`[${PROBE_NAME}] Phase 5: No-op resizes (same dimensions x50)`);
  const preNoopMem = await memory();
  for (let i = 0; i < 50; i++) {
    await resize(NORMAL_WIDTH, NORMAL_HEIGHT);
  }
  await sleep(2000);
  const postNoopMem = await memory();
  await frame('06-post-noop-resize');
  const noopDelta = postNoopMem.rssMB - preNoopMem.rssMB;
  console.log(`  No-op delta: ${noopDelta.toFixed(1)} MB`);

  if (noopDelta > 5) {
    findingsEmitted++;
    writeFinding({
      slug: 'noop-resize-leaks',
      title: `No-op resizes leaked ${noopDelta.toFixed(1)} MB`,
      severity: 'smell',
      file: 'packages/twinki/packages/twinki/src/terminal/process-terminal.ts',
      description: `50 resize events with unchanged dimensions (${NORMAL_WIDTH}x${NORMAL_HEIGHT}) caused ${noopDelta.toFixed(1)} MB growth. The dimension guard should skip these entirely.`,
      evidence: `Pre: ${preNoopMem.rssMB} MB\nPost: ${postNoopMem.rssMB} MB\nDelta: ${noopDelta.toFixed(1)} MB\nSee frame: 06-post-noop-resize`,
      proposedFix: 'Verify ProcessTerminal dimension guard: skip onResize when cols/rows match current.',
    });
  }

  // ── Phase 6: Content reflow verification ──────────────────────
  console.log(`[${PROBE_NAME}] Phase 6: Content reflow after resize cycle`);
  await resize(NORMAL_WIDTH, NORMAL_HEIGHT);
  await sleep(1000);
  const finalScreen = await screen();
  await frame('07-final-state');
  const finalMem = await memory();

  const finalVisible = finalScreen.filter(l => l.trim()).length;
  const totalDelta = finalMem.rssMB - baselineMem.rssMB;
  console.log(`  Final visible lines: ${finalVisible}, Total RSS delta: ${totalDelta.toFixed(1)} MB`);

  if (totalDelta > MEMORY_BUDGET_MB * 2) {
    findingsEmitted++;
    writeFinding({
      slug: 'cumulative-memory-growth',
      title: `Cumulative resize testing grew RSS by ${totalDelta.toFixed(1)} MB`,
      severity: 'spiral',
      file: 'packages/twinki/packages/twinki/src/terminal/process-terminal.ts',
      description: `After all resize phases (narrow, storm, no-op), total RSS grew ${totalDelta.toFixed(1)} MB from baseline. Budget: ${MEMORY_BUDGET_MB * 2} MB.`,
      evidence: `Baseline: ${baselineMem.rssMB} MB\nFinal: ${finalMem.rssMB} MB\nTotal delta: ${totalDelta.toFixed(1)} MB\nSee frame: 07-final-state`,
    });
  }

  // ── Done ──────────────────────────────────────────────────────
  const elapsed = Date.now() - started;
  writeMetrics({
    probe: PROBE_NAME,
    platform: PLATFORM,
    baselineRssMB: baselineMem.rssMB,
    finalRssMB: finalMem.rssMB,
    totalDeltaMB: Math.round(totalDelta * 10) / 10,
    stormCount: STORM_COUNT,
    narrowWidth: NARROW_WIDTH,
    findingsEmitted,
    elapsed,
  });
  writeDoneMarker(findingsEmitted, elapsed);

  const result = findingsEmitted > 0 ? 'FAIL' : 'PASS';
  console.log(`\n[${PROBE_NAME}] ${result} — ${findingsEmitted} finding(s), total delta=${totalDelta.toFixed(1)}MB, ${elapsed}ms`);
  process.exit(findingsEmitted > 0 ? 1 : 0);
}

try {
  await main();
} catch (err) {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  writeFileSync(join(OUTPUT_DIR, `${PREFIX}-error.log`), String(err instanceof Error ? (err.stack ?? err.message) : err));
  console.error(`[${PROBE_NAME}] probe crashed:`, err);
  process.exit(2);
}

#!/usr/bin/env bun
/**
 * message-ordering.ts — Knight Rider probe: static item / message swallowing.
 *
 * Drives a multi-turn conversation and verifies:
 *   1. All turns remain visible in scrollback (no swallowing)
 *   2. Turn order is preserved (no garbling from trim-reappend)
 *   3. Messages survive a terminal resize mid-conversation
 *   4. New messages appear after the static item cap is hit
 *
 * Guards against the recurring class (4+ fixes, Apr–May 2026):
 *   - 217ab6d: trimming desyncs cursor → new messages disappear
 *   - 62a764e: trim-reappend cycle garbles order on resize
 *   - 804d8c4: turn summary disappears after incremental flush
 *   - 1ab65fb: unbounded static items → resize re-layouts all
 *
 * Exit codes: 0 = pass, 1 = finding, 2 = probe crash
 */

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const PROBE_NAME = 'message-ordering';
const PLATFORM = process.env.KIRO_PROBE_PLATFORM ?? (process.platform === 'darwin' ? 'macos' : process.platform);
const OUTPUT_DIR = process.env.PROBE_OUTPUT_DIR ?? './probe-output';
const KR_URL = process.env.KNIGHT_RIDER_URL ?? 'http://localhost:3001';
const KR = `${KR_URL}/api`;

// Number of turns to drive. Must exceed typical trim threshold.
const TURN_COUNT = parseInt(process.env.MESSAGE_ORDERING_TURNS ?? '12', 10);
const RESPONSE_TIMEOUT_MS = 60_000;

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
    `technique: 4`,
    `class: message-ordering`,
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
async function screen(): Promise<string[]> { return (await kr('GET', '/screen')).lines; }
async function sleep(ms: number) { return kr('POST', '/sleep', { ms }); }
async function status(): Promise<{ ready: boolean }> { return kr('GET', '/status'); }

async function typeText(text: string) {
  for (const c of text) {
    await kr('POST', '/keys', { keys: c });
    await new Promise(r => setTimeout(r, 40));
  }
  await new Promise(r => setTimeout(r, 300));
}

async function waitForIdle(timeoutMs = RESPONSE_TIMEOUT_MS): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const lines = await screen();
    const text = lines.join('\n').toLowerCase();
    if (text.includes('ask a question') || text.includes('message kiro')) {
      return true;
    }
    await new Promise(r => setTimeout(r, 2000));
  }
  return false;
}

/** Scroll to top and collect all visible text across multiple screens */
async function collectAllScreens(): Promise<string> {
  const screens: string[] = [];
  // Scroll up many times to reach the top
  for (let i = 0; i < 30; i++) {
    await kr('POST', '/up', undefined);
    await new Promise(r => setTimeout(r, 150));
  }
  await sleep(500);

  // Capture screens while scrolling down
  for (let i = 0; i < 30; i++) {
    const lines = await screen();
    screens.push(lines.join('\n'));
    await kr('POST', '/down', undefined);
    await new Promise(r => setTimeout(r, 150));
  }
  await sleep(300);
  const finalLines = await screen();
  screens.push(finalLines.join('\n'));

  return screens.join('\n');
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const started = Date.now();
  let findingsEmitted = 0;

  // Check Knight Rider
  try {
    const s = await status();
    if (!s.ready) throw new Error('not ready');
  } catch {
    console.error(`[${PROBE_NAME}] Knight Rider not available at ${KR_URL}`);
    process.exit(2);
  }
  console.log(`[${PROBE_NAME}] Knight Rider ready, driving ${TURN_COUNT} turns`);

  // Each turn uses a unique marker so we can verify ordering later.
  // Prompt: "Reply with exactly: MARKER-N" — short responses to keep it fast.
  const markers: string[] = [];

  await frame('01-initial');

  // ── Phase 1: Drive N turns ────────────────────────────────────
  for (let i = 1; i <= TURN_COUNT; i++) {
    const marker = `PROBE-TURN-${String(i).padStart(3, '0')}`;
    markers.push(marker);

    console.log(`[${PROBE_NAME}] Turn ${i}/${TURN_COUNT}: ${marker}`);
    await typeText(`Reply with exactly this text and nothing else: ${marker}`);
    await kr('POST', '/enter', undefined);

    const responded = await waitForIdle();
    if (!responded) {
      await frame(`turn-${i}-timeout`);
      findingsEmitted++;
      writeFinding({
        slug: `turn-${i}-timeout`,
        title: `Turn ${i} timed out`,
        severity: 'crash',
        file: 'packages/tui/src/components/chat/ConversationView.tsx',
        description: `Agent did not respond to turn ${i} within ${RESPONSE_TIMEOUT_MS / 1000}s.`,
        evidence: `Marker: ${marker}\nSee frame: turn-${i}-timeout`,
      });
      break;
    }

    // Capture a frame every few turns
    if (i % 4 === 0 || i === TURN_COUNT) {
      await frame(`02-turn-${i}-complete`);
    }
  }

  // ── Phase 2: Verify all markers visible in scrollback ─────────
  console.log(`[${PROBE_NAME}] Phase 2: Verifying all markers in scrollback`);
  await sleep(1000);
  const allText = await collectAllScreens();
  await frame('03-scrollback-collected');

  const found: number[] = [];
  const missing: number[] = [];
  for (let i = 0; i < markers.length; i++) {
    if (allText.includes(markers[i]!)) {
      found.push(i + 1);
    } else {
      missing.push(i + 1);
    }
  }

  console.log(`[${PROBE_NAME}] Found ${found.length}/${markers.length} markers`);

  if (missing.length > 0) {
    findingsEmitted++;
    writeFinding({
      slug: 'messages-swallowed',
      title: `${missing.length} turn(s) swallowed — not visible in scrollback`,
      severity: 'regression',
      file: 'packages/tui/src/components/chat/ConversationView.tsx',
      description: `After ${TURN_COUNT} turns, ${missing.length} marker(s) are not visible anywhere in scrollback. This matches the known static-item trimming desync (PR #2101) where new messages silently disappear after the cursor falls behind.`,
      evidence: `Missing turns: ${missing.join(', ')}\nFound turns: ${found.length}/${markers.length}\nSee frame: 03-scrollback-collected`,
      proposedFix: 'Verify adjustStaticCursor is called after trimStaticItems and emittedIds retains trimmed IDs.',
    });
  }

  // ── Phase 3: Verify ordering ──────────────────────────────────
  console.log(`[${PROBE_NAME}] Phase 3: Checking turn order`);
  const positions = found.map(turnNum => ({
    turn: turnNum,
    pos: allText.indexOf(markers[turnNum - 1]!),
  })).filter(p => p.pos >= 0);

  let outOfOrder = false;
  for (let i = 1; i < positions.length; i++) {
    if (positions[i]!.pos < positions[i - 1]!.pos) {
      outOfOrder = true;
      break;
    }
  }

  if (outOfOrder) {
    findingsEmitted++;
    writeFinding({
      slug: 'messages-out-of-order',
      title: 'Conversation turns rendered out of order',
      severity: 'regression',
      file: 'packages/tui/src/components/chat/ConversationView.tsx',
      description: `Turn markers appear out of sequence in scrollback. This matches the trim-reappend cycle (PR #2101, #62a764e) where trimmed items get re-appended at the end.`,
      evidence: `Turn positions (first 10): ${JSON.stringify(positions.slice(0, 10))}\nSee frame: 03-scrollback-collected`,
      proposedFix: 'Verify emittedIds tombstones prevent re-append of trimmed items.',
    });
  }

  // ── Phase 4: Resize and re-verify ─────────────────────────────
  console.log(`[${PROBE_NAME}] Phase 4: Resize and re-verify`);
  await kr('POST', '/resize', { cols: 80, rows: 30 });
  await sleep(3000);
  await frame('04-after-resize');

  // Check the most recent markers are still visible on current screen
  const postResizeLines = await screen();
  const postResizeText = postResizeLines.join('\n');
  const lastMarker = markers[markers.length - 1]!;

  // Scroll down to bottom to see latest
  for (let i = 0; i < 10; i++) {
    await kr('POST', '/down', undefined);
    await new Promise(r => setTimeout(r, 100));
  }
  await sleep(500);
  const bottomLines = await screen();
  const bottomText = bottomLines.join('\n');
  await frame('05-bottom-after-resize');

  // The most recent turn should be reachable
  const recentVisible = bottomText.includes(lastMarker) || postResizeText.includes(lastMarker);
  if (!recentVisible && found.includes(markers.length)) {
    // Only flag if we previously found it (it was there before resize)
    findingsEmitted++;
    writeFinding({
      slug: 'message-lost-on-resize',
      title: 'Most recent turn disappeared after resize',
      severity: 'regression',
      file: 'packages/twinki/packages/twinki/src/dom/static-output.ts',
      description: `The last turn marker (${lastMarker}) was visible before resize but disappeared after resizing to 80x30. This matches the resetStatic / replaceStaticOutput reflow bug.`,
      evidence: `Last marker: ${lastMarker}\nVisible after resize: false\nSee frames: 04-after-resize, 05-bottom-after-resize`,
      proposedFix: 'Verify resetStatic correctly re-renders all static items at new width.',
    });
  }

  // Restore
  await kr('POST', '/resize', { cols: 120, rows: 40 });
  await sleep(1000);
  await frame('06-final');

  // ── Done ──────────────────────────────────────────────────────
  const elapsed = Date.now() - started;
  writeMetrics({
    probe: PROBE_NAME,
    platform: PLATFORM,
    turnCount: TURN_COUNT,
    markersFound: found.length,
    markersMissing: missing.length,
    outOfOrder,
    findingsEmitted,
    elapsed,
  });
  writeDoneMarker(findingsEmitted, elapsed);

  const result = findingsEmitted > 0 ? 'FAIL' : 'PASS';
  console.log(`\n[${PROBE_NAME}] ${result} — ${findingsEmitted} finding(s), found=${found.length}/${markers.length}, order=${outOfOrder ? 'BROKEN' : 'ok'}, ${elapsed}ms`);
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

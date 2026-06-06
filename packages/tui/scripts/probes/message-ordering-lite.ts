#!/usr/bin/env bun
/**
 * message-ordering-lite.ts — Deterministic ordering probe for lite mode.
 *
 * Fires a known sequence of 50 assistant response chunks interleaved with
 * 10 tool calls, then verifies the scrollback snapshot preserves creation
 * order. Re-runs the sequence multiple times and asserts byte-identical
 * rendering (deterministic output).
 *
 * Guards against trim-reappend garbling (62a764e), tool/response interleave
 * misordering (bug-mine 1.2), and non-deterministic render races.
 *
 * Exit codes: 0 = pass, 1 = finding, 2 = probe crash
 */

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { E2ETestCase } from '../../e2e_tests/E2ETestCase';

const PROBE_NAME = 'message-ordering-lite';
const PLATFORM =
  process.env.KIRO_PROBE_PLATFORM ??
  (process.platform === 'darwin' ? 'macos' : process.platform);
const OUTPUT_DIR = process.env.PROBE_OUTPUT_DIR ?? './probe-output';
const RUN_COUNT = parseInt(process.env.ORDERING_RUNS ?? '5', 10);

// 10 turns: each has 5 assistant chunks + 1 tool call = 50 chunks + 10 tools total.
const TURN_COUNT = 10;
const CHUNKS_PER_TURN = 5;

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
  writeFileSync(
    filePath,
    [
      '---',
      `id: ${findingId}`,
      `work-item: ${PROBE_NAME}`,
      `review: 01-async-render-path`,
      `technique: deterministic-replay`,
      `class: message-ordering-lite`,
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
    ].join('\n'),
  );
  return filePath;
}

function writeDoneMarker(findingsEmitted: number, elapsedMs: number) {
  writeFileSync(
    join(OUTPUT_DIR, `${PREFIX}-done.md`),
    [
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
    ].join('\n'),
  );
}

function writeMetrics(metrics: Record<string, unknown>) {
  writeFileSync(join(OUTPUT_DIR, `${PREFIX}-metrics.json`), JSON.stringify(metrics, null, 2));
}

/**
 * Drives the deterministic event sequence in a fresh E2ETestCase instance.
 * Returns the terminal snapshot lines joined as a single string.
 */
async function driveSequence(runIndex: number): Promise<string> {
  const tc = await E2ETestCase.builder()
    .withTestName(`probe-ordering-lite-${Date.now()}-${runIndex}`)
    .withTerminal({ width: 120, height: 80 }) // tall terminal to avoid scrolling
    .withLite()
    .launch();

  try {
    // Wait for full readiness before driving turns
    await tc.waitForText('>', 15000);
    await tc.getSessionId();
    await tc.waitForSlashCommands();

    for (let turn = 0; turn < TURN_COUNT; turn++) {
      // Build event batch: 5 assistant chunks + 1 tool call
      const events: Array<{ kind: 'event'; data: { kind: string; data: Record<string, unknown> } }> = [];

      for (let chunk = 0; chunk < CHUNKS_PER_TURN; chunk++) {
        events.push({
          kind: 'event' as const,
          data: {
            kind: 'AssistantResponseEvent' as const,
            data: { content: `RESP_T${turn}_C${chunk} ` },
          },
        });
      }

      // Tool call for this turn
      events.push({
        kind: 'event' as const,
        data: {
          kind: 'ToolUseEvent' as const,
          data: {
            tool_use_id: `tool_${turn}`,
            name: 'fs_read',
            input: JSON.stringify({ ops: [{ path: `probe_file_${turn}.txt` }] }),
            stop: true,
          },
        },
      });

      // Push events then close stream
      await tc.pushSendMessageResponse(events as any, { silent: true });
      await tc.pushSendMessageResponse(null, { silent: true });

      // Final response after tool — marks the end of this turn
      await tc.pushSendMessageResponse(
        [
          {
            kind: 'event' as const,
            data: {
              kind: 'AssistantResponseEvent' as const,
              data: { content: `END_TURN_${turn}` },
            },
          },
        ] as any,
        { silent: true },
      );
      await tc.pushSendMessageResponse(null, { silent: true });

      // Send user message to trigger the turn
      await tc.sendKeys(`turn${turn}`);
      await tc.sleepMs(100);
      await tc.pressEnter();
      await tc.waitForText(`END_TURN_${turn}`, 30000);
      await tc.waitForIdle(15000);
    }

    // Let rendering settle
    await tc.sleepMs(500);
    const snapshot = tc.getSnapshot();
    return snapshot.join('\n');
  } finally {
    await tc.cleanup();
  }
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const started = Date.now();
  let findingsEmitted = 0;

  console.log(
    `[${PROBE_NAME}] Running ${RUN_COUNT} iterations (${TURN_COUNT} turns x ${CHUNKS_PER_TURN} chunks + ${TURN_COUNT} tools = ${TURN_COUNT * CHUNKS_PER_TURN} chunks + ${TURN_COUNT} tools)`,
  );

  const snapshots: string[] = [];

  for (let i = 0; i < RUN_COUNT; i++) {
    console.log(`[${PROBE_NAME}]   Run ${i + 1}/${RUN_COUNT}...`);
    const snap = await driveSequence(i);
    snapshots.push(snap);
  }

  // ── Phase 1: Verify ordering within first snapshot ────────────
  const first = snapshots[0]!;

  // Build expected marker order: for each turn, chunks come first, then tool, then END marker
  const expectedMarkers: string[] = [];
  for (let turn = 0; turn < TURN_COUNT; turn++) {
    for (let chunk = 0; chunk < CHUNKS_PER_TURN; chunk++) {
      expectedMarkers.push(`RESP_T${turn}_C${chunk}`);
    }
    // tool_N renders as a tool label — look for the file path instead
    expectedMarkers.push(`probe_file_${turn}.txt`);
    expectedMarkers.push(`END_TURN_${turn}`);
  }

  let orderCorrect = true;
  let lastIdx = -1;
  const missingMarkers: string[] = [];
  const outOfOrderMarkers: string[] = [];

  for (const marker of expectedMarkers) {
    const idx = first.indexOf(marker);
    if (idx === -1) {
      missingMarkers.push(marker);
      orderCorrect = false;
    } else if (idx <= lastIdx) {
      outOfOrderMarkers.push(marker);
      orderCorrect = false;
    } else {
      lastIdx = idx;
    }
  }

  if (missingMarkers.length > 0) {
    console.log(`[${PROBE_NAME}] FINDING: ${missingMarkers.length} markers missing from snapshot`);
    console.log(`[${PROBE_NAME}]   Missing: ${missingMarkers.slice(0, 10).join(', ')}${missingMarkers.length > 10 ? '...' : ''}`);
  }

  if (outOfOrderMarkers.length > 0) {
    console.log(`[${PROBE_NAME}] FINDING: ${outOfOrderMarkers.length} markers out of order`);
    console.log(`[${PROBE_NAME}]   Out of order: ${outOfOrderMarkers.slice(0, 10).join(', ')}${outOfOrderMarkers.length > 10 ? '...' : ''}`);
  }

  if (!orderCorrect) {
    findingsEmitted++;
    writeFinding({
      slug: 'ordering-violation',
      title: `Lite mode message ordering violation (${missingMarkers.length} missing, ${outOfOrderMarkers.length} out of order)`,
      severity: 'regression',
      file: 'packages/tui/src/components/lite/LiteScrollback.tsx',
      description: `Deterministic sequence of ${TURN_COUNT * CHUNKS_PER_TURN} assistant chunks + ${TURN_COUNT} tool calls rendered out of creation order. This matches the trim-reappend garbling class (62a764e, bug-mine 1.2).`,
      evidence: [
        `Missing markers (${missingMarkers.length}): ${missingMarkers.slice(0, 5).join(', ')}`,
        `Out of order (${outOfOrderMarkers.length}): ${outOfOrderMarkers.slice(0, 5).join(', ')}`,
        `Total expected markers: ${expectedMarkers.length}`,
      ].join('\n'),
      proposedFix:
        'Verify that lite delta-append walk preserves creation order and tool events are not re-sorted by completion time.',
    });
    writeFileSync(join(OUTPUT_DIR, 'ordering-lite-snapshot-run1.txt'), first);
  }

  // ── Phase 2: Verify determinism across runs ───────────────────
  // Normalize snapshots before comparison: strip variable timing values
  // (e.g. "473ms" → "<TIME>") and trailing whitespace differences.
  function normalizeForComparison(snap: string): string {
    return snap
      .replace(/\(\d+ms\)/g, '(<TIME>)')       // startup timing "(473ms)" → "(<TIME>)"
      .replace(/\d+% ctx/g, '<PCT> ctx')        // context percentage varies
      .replace(/\s+$/gm, '');                   // trailing whitespace per line
  }

  const firstNorm = normalizeForComparison(first);
  let deterministic = true;
  const diffRuns: number[] = [];

  for (let i = 1; i < snapshots.length; i++) {
    const thisNorm = normalizeForComparison(snapshots[i]!);
    if (thisNorm !== firstNorm) {
      deterministic = false;
      diffRuns.push(i + 1);
      writeFileSync(join(OUTPUT_DIR, `ordering-lite-run1.txt`), first);
      writeFileSync(join(OUTPUT_DIR, `ordering-lite-run${i + 1}.txt`), snapshots[i]!);
    }
  }

  if (!deterministic) {
    findingsEmitted++;
    console.log(`[${PROBE_NAME}] FINDING: Non-deterministic rendering — runs ${diffRuns.join(', ')} differ from run 1`);
    writeFinding({
      slug: 'non-deterministic-render',
      title: `Lite mode rendering non-deterministic across ${diffRuns.length} of ${RUN_COUNT} runs`,
      severity: 'regression',
      file: 'packages/tui/src/components/lite/LiteScrollback.tsx',
      description: `Identical event sequences produced different terminal output across multiple runs. This indicates a race condition or non-deterministic render path in lite mode.`,
      evidence: `Runs that differ from run 1: ${diffRuns.join(', ')}\nDiff files written to probe-output/ordering-lite-run*.txt`,
      proposedFix:
        'Check for async render scheduling races, non-stable sort operations, or Map iteration order assumptions in the lite render path.',
    });
  }

  // ── Results ───────────────────────────────────────────────────
  const elapsed = Date.now() - started;

  writeMetrics({
    probe: PROBE_NAME,
    platform: PLATFORM,
    runCount: RUN_COUNT,
    turnCount: TURN_COUNT,
    chunksPerTurn: CHUNKS_PER_TURN,
    totalChunks: TURN_COUNT * CHUNKS_PER_TURN,
    totalTools: TURN_COUNT,
    totalExpectedMarkers: expectedMarkers.length,
    missingMarkers: missingMarkers.length,
    outOfOrderMarkers: outOfOrderMarkers.length,
    orderCorrect,
    deterministic,
    diffRuns,
    findingsEmitted,
    elapsed,
  });
  writeDoneMarker(findingsEmitted, elapsed);

  console.log(`\n[${PROBE_NAME}] === RESULTS ===`);
  console.log(`[${PROBE_NAME}] Order correct: ${orderCorrect}`);
  console.log(`[${PROBE_NAME}] Deterministic across ${RUN_COUNT} runs: ${deterministic}`);
  console.log(`[${PROBE_NAME}] Elapsed: ${elapsed} ms`);

  const result = findingsEmitted > 0 ? 'FAIL (findings)' : 'PASS';
  console.log(`[${PROBE_NAME}] ${result}`);
  process.exit(findingsEmitted > 0 ? 1 : 0);
}

try {
  await main();
} catch (err) {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  writeFileSync(
    join(OUTPUT_DIR, `${PREFIX}-error.log`),
    String(err instanceof Error ? (err.stack ?? err.message) : err),
  );
  console.error(`[${PROBE_NAME}] probe crashed:`, err);
  process.exit(2);
}

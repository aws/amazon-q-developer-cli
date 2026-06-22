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

import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { E2ETestCase } from '../../e2e_tests/E2ETestCase';
import { streamReply } from '../../e2e_tests/lite/helpers/responses';
import {
  createProbeContext,
  writeDoneMarker,
  writeFinding,
  writeMetrics,
  runProbe,
} from './probe-utils';

const ctx = createProbeContext('message-ordering-lite');
const RUN_COUNT = parseInt(process.env.ORDERING_RUNS ?? '5', 10);

// 10 turns: each has 5 assistant chunks + 1 tool call = 50 chunks + 10 tools total.
const TURN_COUNT = 10;
const CHUNKS_PER_TURN = 5;

const FINDING_FILE = 'packages/tui/src/components/lite/LiteScrollback.tsx';

async function driveSequence(runIndex: number): Promise<string> {
  const tc = await E2ETestCase.builder()
    .withTestName(`probe-ordering-lite-${Date.now()}-${runIndex}`)
    .withTerminal({ width: 120, height: 80 }) // tall terminal to avoid scrolling
    .withLite()
    .launch();

  try {
    await tc.waitForText('>', 15000);
    await tc.getSessionId();
    await tc.waitForSlashCommands();

    for (let turn = 0; turn < TURN_COUNT; turn++) {
      const events: Array<{
        kind: 'event';
        data: { kind: string; data: Record<string, unknown> };
      }> = [];

      for (let chunk = 0; chunk < CHUNKS_PER_TURN; chunk++) {
        events.push({
          kind: 'event' as const,
          data: {
            kind: 'AssistantResponseEvent' as const,
            data: { content: `RESP_T${turn}_C${chunk} ` },
          },
        });
      }

      events.push({
        kind: 'event' as const,
        data: {
          kind: 'ToolUseEvent' as const,
          data: {
            tool_use_id: `tool_${turn}`,
            name: 'fs_read',
            input: JSON.stringify({
              ops: [{ path: `probe_file_${turn}.txt` }],
            }),
            stop: true,
          },
        },
      });

      await tc.pushSendMessageResponse(events as any, { silent: true });
      await tc.pushSendMessageResponse(null, { silent: true });

      // Final response after tool — marks the end of this turn
      await streamReply(tc, `END_TURN_${turn}`, { silent: true });

      await tc.sendKeys(`turn${turn}`);
      await tc.sleepMs(100);
      await tc.pressEnter();
      await tc.waitForText(`END_TURN_${turn}`, 30000);
      await tc.waitForIdle(15000);
    }

    await tc.sleepMs(500);
    const snapshot = tc.getSnapshot();
    return snapshot.join('\n');
  } finally {
    await tc.cleanup();
  }
}

async function main() {
  let findingsEmitted = 0;

  console.log(
    `[${ctx.name}] Running ${RUN_COUNT} iterations (${TURN_COUNT} turns x ${CHUNKS_PER_TURN} chunks + ${TURN_COUNT} tools = ${TURN_COUNT * CHUNKS_PER_TURN} chunks + ${TURN_COUNT} tools)`
  );

  const snapshots: string[] = [];

  for (let i = 0; i < RUN_COUNT; i++) {
    console.log(`[${ctx.name}]   Run ${i + 1}/${RUN_COUNT}...`);
    const snap = await driveSequence(i);
    snapshots.push(snap);
  }

  // ── Phase 1: Verify ordering within first snapshot ────────────
  const first = snapshots[0]!;

  // Per turn: chunks first, then tool (matched by file path), then END marker.
  const expectedMarkers: string[] = [];
  for (let turn = 0; turn < TURN_COUNT; turn++) {
    for (let chunk = 0; chunk < CHUNKS_PER_TURN; chunk++) {
      expectedMarkers.push(`RESP_T${turn}_C${chunk}`);
    }
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
    console.log(
      `[${ctx.name}] FINDING: ${missingMarkers.length} markers missing from snapshot`
    );
    console.log(
      `[${ctx.name}]   Missing: ${missingMarkers.slice(0, 10).join(', ')}${missingMarkers.length > 10 ? '...' : ''}`
    );
  }

  if (outOfOrderMarkers.length > 0) {
    console.log(
      `[${ctx.name}] FINDING: ${outOfOrderMarkers.length} markers out of order`
    );
    console.log(
      `[${ctx.name}]   Out of order: ${outOfOrderMarkers.slice(0, 10).join(', ')}${outOfOrderMarkers.length > 10 ? '...' : ''}`
    );
  }

  if (!orderCorrect) {
    findingsEmitted++;
    writeFinding(ctx, {
      slug: 'ordering-violation',
      title: `Lite mode message ordering violation (${missingMarkers.length} missing, ${outOfOrderMarkers.length} out of order)`,
      severity: 'regression',
      description: `Deterministic sequence of ${TURN_COUNT * CHUNKS_PER_TURN} assistant chunks + ${TURN_COUNT} tool calls rendered out of creation order. This matches the trim-reappend garbling class (62a764e, bug-mine 1.2).`,
      evidence: [
        `Missing markers (${missingMarkers.length}): ${missingMarkers.slice(0, 5).join(', ')}`,
        `Out of order (${outOfOrderMarkers.length}): ${outOfOrderMarkers.slice(0, 5).join(', ')}`,
        `Total expected markers: ${expectedMarkers.length}`,
      ].join('\n'),
      proposedFix:
        'Verify that lite delta-append walk preserves creation order and tool events are not re-sorted by completion time.',
      workItem: ctx.name,
      review: '01-async-render-path',
      technique: 'deterministic-replay',
      file: FINDING_FILE,
    });
    writeFileSync(
      join(ctx.outputDir, 'ordering-lite-snapshot-run1.txt'),
      first
    );
  }

  // ── Phase 2: Verify determinism across runs ───────────────────
  // Strip variable timing values before comparison so only structural
  // (ordering/content) differences register as non-determinism.
  function normalizeForComparison(snap: string): string {
    return snap
      .replace(/\(\d+ms\)/g, '(<TIME>)')
      .replace(/\d+% ctx/g, '<PCT> ctx')
      .replace(/\s+$/gm, '');
  }

  const firstNorm = normalizeForComparison(first);
  let deterministic = true;
  const diffRuns: number[] = [];

  for (let i = 1; i < snapshots.length; i++) {
    const thisNorm = normalizeForComparison(snapshots[i]!);
    if (thisNorm !== firstNorm) {
      deterministic = false;
      diffRuns.push(i + 1);
      writeFileSync(join(ctx.outputDir, `ordering-lite-run1.txt`), first);
      writeFileSync(
        join(ctx.outputDir, `ordering-lite-run${i + 1}.txt`),
        snapshots[i]!
      );
    }
  }

  if (!deterministic) {
    findingsEmitted++;
    console.log(
      `[${ctx.name}] FINDING: Non-deterministic rendering — runs ${diffRuns.join(', ')} differ from run 1`
    );
    writeFinding(ctx, {
      slug: 'non-deterministic-render',
      title: `Lite mode rendering non-deterministic across ${diffRuns.length} of ${RUN_COUNT} runs`,
      severity: 'regression',
      description: `Identical event sequences produced different terminal output across multiple runs. This indicates a race condition or non-deterministic render path in lite mode.`,
      evidence: `Runs that differ from run 1: ${diffRuns.join(', ')}\nDiff files written to probe-output/ordering-lite-run*.txt`,
      proposedFix:
        'Check for async render scheduling races, non-stable sort operations, or Map iteration order assumptions in the lite render path.',
      workItem: ctx.name,
      review: '01-async-render-path',
      technique: 'deterministic-replay',
      file: FINDING_FILE,
    });
  }

  // ── Results ───────────────────────────────────────────────────
  const elapsed = Date.now() - ctx.startedAt;

  writeMetrics(ctx, {
    probe: ctx.name,
    platform: ctx.platform,
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
  writeDoneMarker(ctx, {
    workItem: ctx.name,
    findingsEmitted,
    summary: `Emitted ${findingsEmitted} finding(s) in ${elapsed} ms.`,
  });

  console.log(`\n[${ctx.name}] === RESULTS ===`);
  console.log(`[${ctx.name}] Order correct: ${orderCorrect}`);
  console.log(
    `[${ctx.name}] Deterministic across ${RUN_COUNT} runs: ${deterministic}`
  );
  console.log(`[${ctx.name}] Elapsed: ${elapsed} ms`);

  const result = findingsEmitted > 0 ? 'FAIL (findings)' : 'PASS';
  console.log(`[${ctx.name}] ${result}`);
  process.exit(findingsEmitted > 0 ? 1 : 0);
}

await runProbe(ctx, main);

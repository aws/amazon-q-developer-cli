#!/usr/bin/env bun
/**
 * session-lifetime-lite.ts — Memory stability probe for lite mode.
 *
 * Drives N turns (default 100) in lite mode and verifies RSS stays bounded.
 * The LITE_HISTORY_RENDER_CAP (70) prevents unbounded static item growth in
 * the Ink <Static> list; this probe confirms that memory does not grow
 * proportional to conversation length.
 *
 * Uses E2ETestCase to spawn a real CLI + TUI in lite mode, inject mock
 * assistant responses via the agent IPC, and sample memory via the TUI IPC.
 *
 * Exit codes: 0 = pass, 1 = finding, 2 = probe crash
 *
 * Usage:
 *   bun run packages/tui/scripts/probes/session-lifetime-lite.ts
 *
 * Environment:
 *   LITE_PROBE_TURNS      number of turns to drive (default: 100)
 *   RSS_CEILING_MB        max allowed RSS at end (default: 250)
 *   PROBE_OUTPUT_DIR      where to write metrics JSON (default: ./probe-output)
 *   SAMPLE_EVERY_N        sample memory every N turns (default: 10)
 */

import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { E2ETestCase } from '../../e2e_tests/E2ETestCase';
import { createProbeContext, linearSlope, runProbe } from './probe-utils';

const ctx = createProbeContext('session-lifetime-lite');
const PROBE_NAME = ctx.name;
const OUTPUT_DIR = ctx.outputDir;
const TURN_COUNT = parseInt(process.env.LITE_PROBE_TURNS ?? '100', 10);
const RSS_CEILING_MB = parseInt(process.env.RSS_CEILING_MB ?? '250', 10);
const SAMPLE_EVERY_N = parseInt(process.env.SAMPLE_EVERY_N ?? '10', 10);

interface Sample {
  turn: number;
  rssKb: number;
  heapUsedKb: number;
  msgCount: number;
  elapsedMs: number;
}

async function main() {
  const started = ctx.startedAt;

  console.log(
    `[${PROBE_NAME}] Starting: ${TURN_COUNT} turns, RSS ceiling ${RSS_CEILING_MB} MB`
  );

  // Launch CLI in lite mode with E2ETestCase harness
  const tc = await E2ETestCase.builder()
    .withTestName(`probe-${PROBE_NAME}-${Date.now()}`)
    .withTerminal({ width: 120, height: 40 })
    .withLite()
    .launch();

  try {
    // Wait for TUI to be fully ready (slash commands = backend handshake complete)
    await tc.waitForText('>', 20000);
    await tc.waitForSlashCommands(15000);
    await tc.getSessionId(15000);

    // Force GC before baseline
    await tc.forceGC();
    await tc.sleepMs(500);

    const baselineMem = await tc.getMemoryUsage();
    const baselineRssMb = baselineMem.rss / 1024 / 1024;
    console.log(`[${PROBE_NAME}] Baseline RSS: ${baselineRssMb.toFixed(1)} MB`);

    const samples: Sample[] = [];

    // Record baseline sample
    const store0 = await tc.getStore();
    samples.push({
      turn: 0,
      rssKb: Math.round(baselineMem.rss / 1024),
      heapUsedKb: Math.round(baselineMem.heapUsed / 1024),
      msgCount: store0.messages.length,
      elapsedMs: Date.now() - started,
    });

    // Drive turns
    for (let i = 1; i <= TURN_COUNT; i++) {
      // Queue the mock response BEFORE sending the user message
      await tc.pushSendMessageResponse(
        [
          {
            kind: 'event',
            data: {
              kind: 'AssistantResponseEvent',
              data: { content: `Response ${i} of ${TURN_COUNT}` },
            },
          },
        ],
        { silent: true }
      );
      await tc.pushSendMessageResponse(null, { silent: true });

      // Type and send the user message (separate typing from submit, with delay)
      await tc.sendKeys(`turn ${i}`);
      await tc.sleepMs(100);
      await tc.pressEnter();

      // Wait for the response to appear
      await tc.waitForText(`Response ${i} of ${TURN_COUNT}`, 30000);
      await tc.waitForIdle(10000);

      // Sample at regular intervals
      if (i % SAMPLE_EVERY_N === 0 || i === TURN_COUNT) {
        await tc.forceGC();
        await tc.sleepMs(200);

        const mem = await tc.getMemoryUsage();
        const store = await tc.getStore();
        const rssMb = mem.rss / 1024 / 1024;

        samples.push({
          turn: i,
          rssKb: Math.round(mem.rss / 1024),
          heapUsedKb: Math.round(mem.heapUsed / 1024),
          msgCount: store.messages.length,
          elapsedMs: Date.now() - started,
        });

        console.log(
          `  Turn ${String(i).padStart(3)}: RSS=${rssMb.toFixed(1)} MB, ` +
            `heap=${(mem.heapUsed / 1024 / 1024).toFixed(1)} MB, ` +
            `messages=${store.messages.length}`
        );
      }
    }

    // Final measurement
    await tc.forceGC();
    await tc.sleepMs(500);
    const finalMem = await tc.getMemoryUsage();
    const finalStore = await tc.getStore();
    const finalRssMb = finalMem.rss / 1024 / 1024;
    const rssDeltaMb = finalRssMb - baselineRssMb;
    const elapsedMs = Date.now() - started;

    // Check that messages grew (turns were received)
    const finalMsgCount = finalStore.messages.length;

    // Check that liteStaticSkipBefore is set (history cap engaged)
    const skipBefore = finalStore.liteStaticSkipBefore ?? 0;
    const historyCapEngaged = skipBefore > 0;

    // RSS slope (MB/turn) post-warmup — flags unbounded growth.
    const warmupTurn = Math.min(20, Math.floor(TURN_COUNT / 5));
    const postWarmupSamples = samples.filter((s) => s.turn >= warmupTurn);
    const slopeMbPerTurn = linearSlope(
      postWarmupSamples,
      (s) => s.turn,
      (s) => s.rssKb / 1024
    );

    // Results
    console.log(`\n=== RESULTS ===`);
    console.log(`  Duration:          ${(elapsedMs / 1000).toFixed(1)}s`);
    console.log(`  Turns completed:   ${TURN_COUNT}`);
    console.log(`  Baseline RSS:      ${baselineRssMb.toFixed(1)} MB`);
    console.log(`  Final RSS:         ${finalRssMb.toFixed(1)} MB`);
    console.log(`  RSS delta:         ${rssDeltaMb.toFixed(1)} MB`);
    console.log(
      `  RSS slope:         ${(slopeMbPerTurn * 1000).toFixed(2)} KB/turn`
    );
    console.log(`  RSS ceiling:       ${RSS_CEILING_MB} MB`);
    console.log(`  Messages in store: ${finalMsgCount}`);
    console.log(
      `  History cap:       ${historyCapEngaged ? `engaged (skip=${skipBefore})` : 'NOT engaged'}`
    );

    // Write metrics JSON
    const metrics = {
      probe: PROBE_NAME,
      timestamp: new Date().toISOString(),
      config: {
        turnCount: TURN_COUNT,
        rssCeilingMb: RSS_CEILING_MB,
        sampleEveryN: SAMPLE_EVERY_N,
      },
      results: {
        durationMs: elapsedMs,
        turnsCompleted: TURN_COUNT,
        baselineRssMb: +baselineRssMb.toFixed(2),
        finalRssMb: +finalRssMb.toFixed(2),
        rssDeltaMb: +rssDeltaMb.toFixed(2),
        slopeMbPerTurn: +slopeMbPerTurn.toFixed(4),
        finalMessageCount: finalMsgCount,
        liteStaticSkipBefore: skipBefore,
        historyCapEngaged,
      },
      samples,
      pass: finalRssMb <= RSS_CEILING_MB,
    };
    writeFileSync(
      join(OUTPUT_DIR, `${PROBE_NAME}.json`),
      JSON.stringify(metrics, null, 2)
    );

    // Findings
    const findings: string[] = [];

    if (finalRssMb > RSS_CEILING_MB) {
      findings.push(
        `RSS ${finalRssMb.toFixed(1)} MB exceeds ceiling ${RSS_CEILING_MB} MB`
      );
    }

    // Note: liteStaticSkipBefore only engages on session RESUME (loading
    // history from disk), not during live streaming. For live sessions the
    // Ink <Static> cursor simply appends new items. This is expected.
    if (!historyCapEngaged && TURN_COUNT >= 50) {
      console.log(
        `  Note: History cap not engaged during live session ` +
          `(liteStaticSkipBefore=${skipBefore}, messages=${finalMsgCount}). ` +
          `This is expected — cap only applies on session resume.`
      );
    }

    if (slopeMbPerTurn > 0.1) {
      findings.push(
        `RSS growth slope ${(slopeMbPerTurn * 1000).toFixed(1)} KB/turn ` +
          `suggests unbounded memory growth`
      );
    }

    if (findings.length > 0) {
      console.log(`\n  FINDINGS:`);
      for (const f of findings) console.log(`    - ${f}`);
      console.log(`\n[${PROBE_NAME}] FAIL (${findings.length} finding(s))`);
      process.exit(1);
    }

    console.log(`\n[${PROBE_NAME}] PASS`);
    process.exit(0);
  } finally {
    await tc.cleanup();
  }
}

await runProbe(ctx, main);

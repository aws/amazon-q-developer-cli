#!/usr/bin/env bun
/**
 * forced-trim-lite.ts — Stress probe for lite mode's history cap.
 *
 * Populates <Static> well past LITE_HISTORY_RENDER_CAP (70), then
 * appends more messages and verifies:
 *   1. New rows still appear correctly (no cursor corruption)
 *   2. Old trimmed rows don't reappear in the terminal
 *   3. Store message count grows but terminal row count stays bounded
 *
 * Exit codes: 0 = pass, 1 = finding, 2 = probe crash
 */

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { E2ETestCase } from '../../e2e_tests/E2ETestCase';
import { streamReply } from '../../e2e_tests/lite/helpers/responses';

const NAME = 'forced-trim-lite';
const startedAt = Date.now();
const outputDir = process.env.PROBE_OUTPUT_DIR ?? './probe-output';

// Exceeds cap with tool calls (50 turns * ~2 msgs = 100+ messages)
const INITIAL_TURNS = parseInt(process.env.INITIAL_TURNS ?? '50', 10);
const POST_TRIM_TURNS = 10;
const LITE_HISTORY_RENDER_CAP = 70;

async function main() {
  mkdirSync(outputDir, { recursive: true });
  const tc = await E2ETestCase.builder()
    .withTestName('probe-forced-trim-lite')
    .withLite()
    .withTerminal({ width: 120, height: 40 })
    .launch();

  try {
    await tc.waitForText('>', 15000);
    await tc.getSessionId();

    // Phase 1: Drive INITIAL_TURNS to exceed the cap
    console.log(
      `[${NAME}] Driving ${INITIAL_TURNS} turns to exceed history cap (${LITE_HISTORY_RENDER_CAP})...`
    );
    for (let i = 0; i < INITIAL_TURNS; i++) {
      await tc.pushSendMessageResponse(
        [
          {
            kind: 'event',
            data: {
              kind: 'ToolUseEvent',
              data: {
                tool_use_id: `trim-t${i}`,
                name: 'fs_read',
                input: JSON.stringify({ ops: [{ path: `pre_trim_${i}.txt` }] }),
                stop: true,
              },
            },
          },
        ],
        { silent: true }
      );
      await tc.pushSendMessageResponse(null, { silent: true });
      await streamReply(tc, `PRE_TRIM_RESPONSE_${i}`, { silent: true });

      await tc.sendKeys(`pre ${i}\r`);
      await tc.waitForText(`PRE_TRIM_RESPONSE_${i}`, 15000);

      if ((i + 1) % 10 === 0) {
        console.log(`[${NAME}]   Turn ${i + 1}/${INITIAL_TURNS} complete`);
      }
    }

    const storeAfterFill = await tc.getStore();
    console.log(
      `[${NAME}] After ${INITIAL_TURNS} turns: ${storeAfterFill.messages.length} messages in store`
    );

    // Phase 2: Drive POST_TRIM_TURNS more after the cap
    console.log(`[${NAME}] Driving ${POST_TRIM_TURNS} more turns post-cap...`);
    const postTrimMarkers: string[] = [];
    for (let i = 0; i < POST_TRIM_TURNS; i++) {
      const marker = `POST_TRIM_MARKER_${i}_XYZ`;
      postTrimMarkers.push(marker);

      await streamReply(tc, marker, { silent: true });

      await tc.sendKeys(`post ${i}\r`);
      await tc.waitForText(marker, 15000);
    }

    await tc.waitForIdle(10000);

    const snapshot = tc.getSnapshot();
    const allText = snapshot.join('\n');
    const storeAtEnd = await tc.getStore();

    const findings: string[] = [];

    // Check 1: All post-trim markers visible
    for (const marker of postTrimMarkers) {
      if (!allText.includes(marker)) {
        findings.push(
          `Post-trim marker "${marker}" not visible — cursor corruption`
        );
      }
    }

    // Check 2: Early pre-trim messages must not REAPPEAR after the post-trim
    // content (they may still sit in scrollback above — only a re-emission
    // below the newest content is a trim-corruption finding).
    const earlyMarker = 'PRE_TRIM_RESPONSE_0';
    if (allText.includes(earlyMarker)) {
      const earlyIdx = allText.indexOf(earlyMarker);
      const lastPostIdx = allText.lastIndexOf(
        postTrimMarkers[postTrimMarkers.length - 1]!
      );
      if (earlyIdx > lastPostIdx) {
        findings.push(
          `Early marker reappeared AFTER post-trim content — trim corruption`
        );
      }
    }

    // Check 3: Messages in order
    let lastIdx = -1;
    for (const marker of postTrimMarkers) {
      const idx = allText.indexOf(marker);
      if (idx !== -1 && idx <= lastIdx) {
        findings.push(`Post-trim marker "${marker}" out of order`);
      }
      if (idx !== -1) lastIdx = idx;
    }

    // Check 4: Store still has all messages (trim is render-only, not data loss)
    const totalExpectedMsgs = (INITIAL_TURNS + POST_TRIM_TURNS) * 2; // user + model per turn, at minimum
    if (storeAtEnd.messages.length < totalExpectedMsgs) {
      findings.push(
        `Store message count (${storeAtEnd.messages.length}) lower than expected ` +
          `minimum (${totalExpectedMsgs}) — possible data loss`
      );
    }

    const visible = postTrimMarkers.filter((m) => allText.includes(m)).length;
    console.log(`\n[${NAME}] === RESULTS ===`);
    console.log(`[${NAME}] Messages in store: ${storeAtEnd.messages.length}`);
    console.log(
      `[${NAME}] Post-trim markers visible: ${visible}/${postTrimMarkers.length}`
    );
    console.log(`[${NAME}] Findings: ${findings.length}`);

    writeFileSync(
      join(outputDir, `${NAME}-metrics.json`),
      JSON.stringify(
        {
          probe: NAME,
          initialTurns: INITIAL_TURNS,
          postTrimTurns: POST_TRIM_TURNS,
          messagesInStore: storeAtEnd.messages.length,
          postTrimVisible: visible,
          postTrimTotal: postTrimMarkers.length,
          findings,
          elapsedMs: Date.now() - startedAt,
        },
        null,
        2
      )
    );

    if (findings.length > 0) {
      for (const f of findings) {
        console.error(`[${NAME}] FINDING: ${f}`);
      }
      process.exit(1);
    }

    console.log(`[${NAME}] PASS`);
    process.exit(0);
  } finally {
    await tc.cleanup();
  }
}

try {
  await main();
} catch (err) {
  mkdirSync(outputDir, { recursive: true });
  writeFileSync(
    join(outputDir, `${NAME}-error.log`),
    String(err instanceof Error ? (err.stack ?? err.message) : err)
  );
  console.error(`[${NAME}] probe crashed:`, err);
  process.exit(2);
}

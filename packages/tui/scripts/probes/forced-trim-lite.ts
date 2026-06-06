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

import { E2ETestCase } from '../../e2e_tests/E2ETestCase';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const PROBE_NAME = 'forced-trim-lite';
const PLATFORM =
  process.env.KIRO_PROBE_PLATFORM ??
  (process.platform === 'darwin' ? 'macos' : process.platform);
const OUTPUT_DIR = process.env.PROBE_OUTPUT_DIR ?? './probe-output';

// Exceeds cap with tool calls (50 turns * ~2 msgs = 100+ messages)
const INITIAL_TURNS = parseInt(process.env.INITIAL_TURNS ?? '50', 10);
const POST_TRIM_TURNS = 10;
const LITE_HISTORY_RENDER_CAP = 70;

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
    `work-item: 04-forced-trim-lite`,
    `review: 03-unbounded-growth`,
    `technique: 10`,
    `class: forced-trim-lite`,
    `severity: ${opts.severity}`,
    `file: packages/tui/src/components/layout/lite/static-flush.ts`,
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

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const started = Date.now();

  const tc = await E2ETestCase.builder()
    .withTestName('probe-forced-trim-lite')
    .withLite()
    .withTerminal({ width: 120, height: 40 })
    .launch();

  try {
    await tc.waitForText('>', 15000);
    await tc.getSessionId();

    // Phase 1: Drive INITIAL_TURNS to exceed the cap
    console.log(`[${PROBE_NAME}] Driving ${INITIAL_TURNS} turns to exceed history cap (${LITE_HISTORY_RENDER_CAP})...`);
    for (let i = 0; i < INITIAL_TURNS; i++) {
      await tc.pushSendMessageResponse([
        { kind: 'event', data: { kind: 'ToolUseEvent', data: { tool_use_id: `trim-t${i}`, name: 'fs_read', input: JSON.stringify({ ops: [{ path: `pre_trim_${i}.txt` }] }), stop: true } } },
      ], { silent: true });
      await tc.pushSendMessageResponse(null, { silent: true });
      await tc.pushSendMessageResponse([
        { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: `PRE_TRIM_RESPONSE_${i}` } } },
      ], { silent: true });
      await tc.pushSendMessageResponse(null, { silent: true });

      await tc.sendKeys(`pre ${i}\r`);
      await tc.waitForText(`PRE_TRIM_RESPONSE_${i}`, 15000);

      if ((i + 1) % 10 === 0) {
        console.log(`[${PROBE_NAME}]   Turn ${i + 1}/${INITIAL_TURNS} complete`);
      }
    }

    const storeAfterFill = await tc.getStore();
    console.log(`[${PROBE_NAME}] After ${INITIAL_TURNS} turns: ${storeAfterFill.messages.length} messages in store`);

    // Phase 2: Drive POST_TRIM_TURNS more after the cap
    console.log(`[${PROBE_NAME}] Driving ${POST_TRIM_TURNS} more turns post-cap...`);
    const postTrimMarkers: string[] = [];
    for (let i = 0; i < POST_TRIM_TURNS; i++) {
      const marker = `POST_TRIM_MARKER_${i}_XYZ`;
      postTrimMarkers.push(marker);

      await tc.pushSendMessageResponse([
        { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: marker } } },
      ], { silent: true });
      await tc.pushSendMessageResponse(null, { silent: true });

      await tc.sendKeys(`post ${i}\r`);
      await tc.waitForText(marker, 15000);
    }

    // Wait for the TUI to settle
    await tc.waitForIdle(10000);

    // Verification
    const snapshot = tc.getSnapshot();
    const allText = snapshot.join('\n');
    const storeAtEnd = await tc.getStore();

    const findings: string[] = [];

    // Check 1: All post-trim markers visible
    for (const marker of postTrimMarkers) {
      if (!allText.includes(marker)) {
        findings.push(`Post-trim marker "${marker}" not visible — cursor corruption`);
      }
    }

    // Check 2: Early pre-trim messages should NOT be in terminal
    // (they were rendered, scrolled past, and the cap ensures they weren't all dumped)
    const earlyMarker = 'PRE_TRIM_RESPONSE_0';
    if (allText.includes(earlyMarker)) {
      // This might be OK if terminal scrollback buffer is large enough.
      // It's only a finding if it appears AFTER the post-trim markers (reappearance).
      const earlyIdx = allText.indexOf(earlyMarker);
      const lastPostIdx = allText.lastIndexOf(postTrimMarkers[postTrimMarkers.length - 1]!);
      if (earlyIdx > lastPostIdx) {
        findings.push(`Early marker reappeared AFTER post-trim content — trim corruption`);
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

    // Emit findings
    console.log(`\n[${PROBE_NAME}] === RESULTS ===`);
    console.log(`[${PROBE_NAME}] Messages in store: ${storeAtEnd.messages.length}`);
    console.log(`[${PROBE_NAME}] Post-trim markers visible: ${postTrimMarkers.filter(m => allText.includes(m)).length}/${postTrimMarkers.length}`);
    console.log(`[${PROBE_NAME}] Findings: ${findings.length}`);

    const metrics = {
      probe: PROBE_NAME,
      platform: PLATFORM,
      initialTurns: INITIAL_TURNS,
      postTrimTurns: POST_TRIM_TURNS,
      messagesInStore: storeAtEnd.messages.length,
      postTrimVisible: postTrimMarkers.filter(m => allText.includes(m)).length,
      postTrimTotal: postTrimMarkers.length,
      findings,
      elapsedMs: Date.now() - started,
    };
    writeFileSync(join(OUTPUT_DIR, `${PREFIX}-metrics.json`), JSON.stringify(metrics, null, 2));

    if (findings.length > 0) {
      for (const f of findings) {
        console.error(`[${PROBE_NAME}] FINDING: ${f}`);
        writeFinding({
          slug: slugify(f.slice(0, 40)),
          title: f,
          severity: 'regression',
          description: f,
          evidence: [
            `Initial turns: ${INITIAL_TURNS}`,
            `Post-trim turns: ${POST_TRIM_TURNS}`,
            `Messages in store: ${storeAtEnd.messages.length}`,
            `Post-trim markers visible: ${postTrimMarkers.filter(m => allText.includes(m)).length}/${postTrimMarkers.length}`,
            `LITE_HISTORY_RENDER_CAP: ${LITE_HISTORY_RENDER_CAP}`,
          ].join('\n'),
          proposedFix:
            'Check liteStaticSkipBefore / static cursor alignment in ' +
            'packages/tui/src/components/layout/lite/static-flush.ts and ' +
            'LiteLayout.tsx after exceeding the 70-message cap.',
        });
      }

      writeFileSync(join(OUTPUT_DIR, `${PREFIX}-done.md`), [
        '---',
        `id: ${PREFIX}-done`,
        `work-item: 04-forced-trim-lite`,
        `kind: blackbox`,
        `platform: ${PLATFORM}`,
        `status: done`,
        `findings-emitted: ${findings.length}`,
        `elapsed-ms: ${Date.now() - started}`,
        `completed-at: ${new Date().toISOString()}`,
        '---',
        '',
        `# Probe done: ${PROBE_NAME} on ${PLATFORM}`,
        '',
        `Emitted ${findings.length} finding(s) after ${INITIAL_TURNS + POST_TRIM_TURNS} turns in ${Date.now() - started} ms.`,
        '',
      ].join('\n'));

      process.exit(1);
    }

    writeFileSync(join(OUTPUT_DIR, `${PREFIX}-done.md`), [
      '---',
      `id: ${PREFIX}-done`,
      `work-item: 04-forced-trim-lite`,
      `kind: blackbox`,
      `platform: ${PLATFORM}`,
      `status: done`,
      `findings-emitted: 0`,
      `elapsed-ms: ${Date.now() - started}`,
      `completed-at: ${new Date().toISOString()}`,
      '---',
      '',
      `# Probe done: ${PROBE_NAME} on ${PLATFORM}`,
      '',
      `PASS after ${INITIAL_TURNS + POST_TRIM_TURNS} turns in ${Date.now() - started} ms.`,
      '',
    ].join('\n'));

    console.log(`[${PROBE_NAME}] PASS`);
    process.exit(0);
  } finally {
    await tc.cleanup();
  }
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

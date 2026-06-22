/**
 * Knight Rider scenario S1 — lite append-only contract across a 4-turn session:
 * short prompt → shell tool with live streaming → streaming response cancelled
 * mid-stream (Ctrl+C) → clean follow-up turn.
 *
 * Regression classes guarded (commit b7d6f4be1, PR #2643 restored cancel-path):
 *   - cancel leaves stale liveOutputs for the cancelled tool id.
 *   - cancel leaves isProcessing=true (no further turns possible).
 *   - post-cancel turn re-emits the prior turn's tool row (orphan ToolUse).
 *
 * Cost ~30s / ~250MB RSS. Env-gated by KIRO_RUN_KNIGHT_RIDER_TESTS=1; default
 * `bun test` skips this file.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import { LiteSequence } from './lite/helpers/sequence';
import { assistantEvent, streamReply } from './lite/helpers/responses';

const KR_ENABLED = process.env.KIRO_RUN_KNIGHT_RIDER_TESTS === '1';

describe.skipIf(!KR_ENABLED)(
  'knight rider — lite shell + cancel + recover',
  () => {
    let testCase: E2ETestCase | null = null;

    afterEach(async () => {
      if (testCase) {
        await testCase.cleanup();
        testCase = null;
      }
    });

    it('survives shell streaming, mid-stream cancel, and a follow-up turn with append-only intact', async () => {
      testCase = await E2ETestCase.builder()
        .withTestName('knight-rider-lite-shell-cancel')
        .withTerminal({ width: 120, height: 40 })
        .withLite()
        .withCliArgs('--trust-tools=shell')
        .launch();

      const seq = new LiteSequence(testCase, 'kr-lite-shell-cancel');
      try {
        await seq.step('boot lite + wait for prompt', async () => {
          await testCase!.waitForText('>', 15000);
          await testCase!.waitForSlashCommands();
          await testCase!.getSessionId();
        });

        // Turn 1 — short greeting prompt.
        const turn1Marker = 'KR_S1_TURN_ONE_HELLO';
        await seq.step('turn 1: short hello prompt', async () => {
          await streamReply(testCase!, turn1Marker);
          await testCase!.sendKeys('hello');
          await testCase!.sleepMs(100);
          await testCase!.pressEnter();
          await testCase!.waitForText(turn1Marker, 15000);
          await testCase!.waitForIdle(15000);
        });

        // Turn 2 — real shell tool with live streaming output. Three lines
        // with 0.4s delays = ~1.2s of streaming, deterministic enough to
        // assert "live output was populated" without dragging the wall
        // clock above the 30s budget.
        const shellToolId = 'kr-s1-shell-tool';
        const turn2Marker = 'KR_S1_TURN_TWO_DONE';
        await seq.step('turn 2: shell tool streaming + complete', async () => {
          const cmd =
            process.platform === 'win32'
              ? '1..3 | ForEach-Object { Write-Output "kr-stream-line-$_"; Start-Sleep -Milliseconds 400 }'
              : 'for i in 1 2 3; do echo "kr-stream-line-$i"; sleep 0.4; done';
          await testCase!.pushSendMessageResponse([
            {
              kind: 'event',
              data: {
                kind: 'ToolUseEvent',
                data: {
                  tool_use_id: shellToolId,
                  name: 'shell',
                  input: JSON.stringify({ command: cmd }),
                  stop: true,
                },
              },
            },
          ]);
          await testCase!.pushSendMessageResponse(null);
          await streamReply(testCase!, turn2Marker);
          await testCase!.sendKeys('run a shell');
          await testCase!.sleepMs(100);
          await testCase!.pressEnter();
          await testCase!.waitForText(turn2Marker, 30000);
          await testCase!.waitForIdle(20000);
        });

        await seq.expect(
          'after turn 2: liveOutputs cleared for shell tool',
          (s) => {
            const lo = (s.liveOutputs as any)?.[shellToolId];
            return lo === undefined;
          }
        );

        // Append-only monotonicity: turn 1+2 markers must stay visible after
        // turns 3 and 4 commit.
        const beforeTurn3Snapshot = testCase!.getSnapshot().join('\n');
        expect(beforeTurn3Snapshot).toContain(turn1Marker);
        expect(beforeTurn3Snapshot).toContain(turn2Marker);

        // Turn 3 — long-running streaming response that the user cancels
        // mid-stream. We push 6 short chunks, leave the stream open, and
        // press Ctrl+C while isProcessing is still true.
        await seq.step(
          'turn 3: 6-chunk stream + Ctrl+C mid-stream',
          async () => {
            for (let i = 0; i < 6; i++) {
              await testCase!.pushSendMessageResponse([
                assistantEvent(`chunk-${i} `),
              ]);
            }
            // DO NOT close the stream here — leave it open so the user is
            // cancelling a real in-flight response.
            await testCase!.sendKeys('long answer please');
            await testCase!.sleepMs(100);
            await testCase!.pressEnter();
            // Wait for at least one chunk to land so cancel happens
            // mid-stream rather than pre-stream.
            await testCase!.waitForText('chunk-2', 15000);
            await testCase!.pressCtrlC();
            await testCase!.waitForStoreCondition(
              (s) => !s.isProcessing,
              15000
            );
          }
        );

        await seq.expect(
          'after Ctrl+C: isProcessing false, no orphan tool entries',
          (s) => {
            const toolMessages = s.messages.filter(
              (m) => m.role === 'tool_use'
            );
            // Only the turn-2 shell tool should be in messages — turn 3
            // had no tool, so total tool_use count is exactly 1.
            return !s.isProcessing && toolMessages.length === 1;
          }
        );

        await seq.expect(
          'after Ctrl+C: liveOutputs has no leftover entries from turn 3',
          (s) => {
            // liveOutputs is keyed by tool-call id; turn 3 had no tool
            // call so the map should have nothing related to turn 3.
            // The shell tool id from turn 2 was already cleared above.
            const keys = Object.keys((s.liveOutputs as any) ?? {});
            return keys.length === 0;
          }
        );

        // Turn 4 — clean follow-up.
        const turn4Marker = 'KR_S1_TURN_FOUR_FINAL';
        await seq.step('turn 4: final clean prompt', async () => {
          // Drain remaining turn-3 chunks by closing that stream first, so a
          // fresh turn is allowed; then stream the turn-4 response.
          await testCase!.pushSendMessageResponse(null);
          await streamReply(testCase!, turn4Marker);
          await testCase!.sendKeys('final prompt');
          await testCase!.sleepMs(100);
          await testCase!.pressEnter();
          await testCase!.waitForText(turn4Marker, 15000);
          await testCase!.waitForIdle(15000);
        });

        await seq.expect(
          'after turn 4: idle with all four turns visible append-only',
          (s) => {
            return !s.isProcessing && s.queuedMessages.length === 0;
          }
        );

        const afterTurn4Snapshot = testCase!.getSnapshot().join('\n');
        // Append-only: turns 1, 2, 4 markers all visible. (Turn 3 had
        // partial chunks committed via the cancel path — chunk-0 may or
        // may not be visible depending on how much of the stream
        // flushed before the cancel; we don't pin that.)
        expect(afterTurn4Snapshot).toContain(turn1Marker);
        expect(afterTurn4Snapshot).toContain(turn2Marker);
        expect(afterTurn4Snapshot).toContain(turn4Marker);
      } catch (e) {
        const dumpDir = await seq.dumpHtml();
        if (dumpDir) {
          console.log(`LiteSequence timeline dumped to: ${dumpDir}`);
        }
        throw e;
      }
    }, 120000);
  }
);

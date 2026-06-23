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

import { describe, expect, it } from 'bun:test';
import { trackCleanup } from './lite/helpers/integ-lifecycle';
import { E2ETestCase } from './E2ETestCase';
import { assistantEvent, streamReply } from './lite/helpers/responses';

const KR_ENABLED = process.env.KIRO_RUN_KNIGHT_RIDER_TESTS === '1';

describe.skipIf(!KR_ENABLED)(
  'knight rider — lite shell + cancel + recover',
  () => {
    let testCase: E2ETestCase | null = null;
    trackCleanup(() => testCase);

    it('survives shell streaming, mid-stream cancel, and a follow-up turn with append-only intact', async () => {
      testCase = await E2ETestCase.builder()
        .withTestName('knight-rider-lite-shell-cancel')
        .withTerminal({ width: 120, height: 40 })
        .withLite()
        .withCliArgs('--trust-tools=shell')
        .launch();

      await testCase.waitForText('>', 15000);
      await testCase.waitForSlashCommands();
      await testCase.getSessionId();

      // Turn 1 — short greeting prompt.
      const turn1Marker = 'KR_S1_TURN_ONE_HELLO';
      await streamReply(testCase, turn1Marker);
      await testCase.sendKeys('hello');
      await testCase.sleepMs(100);
      await testCase.pressEnter();
      await testCase.waitForText(turn1Marker, 15000);
      await testCase.waitForIdle(15000);

      // Turn 2 — real shell tool with live streaming output. Three lines with
      // 0.4s delays = ~1.2s of streaming, deterministic enough to assert "live
      // output was populated" without blowing the 30s budget.
      const shellToolId = 'kr-s1-shell-tool';
      const turn2Marker = 'KR_S1_TURN_TWO_DONE';
      const cmd =
        process.platform === 'win32'
          ? '1..3 | ForEach-Object { Write-Output "kr-stream-line-$_"; Start-Sleep -Milliseconds 400 }'
          : 'for i in 1 2 3; do echo "kr-stream-line-$i"; sleep 0.4; done';
      await testCase.pushSendMessageResponse([
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
      await testCase.pushSendMessageResponse(null);
      await streamReply(testCase, turn2Marker);
      await testCase.sendKeys('run a shell');
      await testCase.sleepMs(100);
      await testCase.pressEnter();
      await testCase.waitForText(turn2Marker, 30000);
      await testCase.waitForIdle(20000);

      // After turn 2: liveOutputs cleared for the shell tool.
      let s = await testCase.getStore();
      expect((s.liveOutputs as any)?.[shellToolId]).toBeUndefined();

      // Append-only monotonicity: turn 1+2 markers must stay visible after
      // turns 3 and 4 commit.
      const beforeTurn3Snapshot = testCase.getSnapshot().join('\n');
      expect(beforeTurn3Snapshot).toContain(turn1Marker);
      expect(beforeTurn3Snapshot).toContain(turn2Marker);

      // Turn 3 — long-running streaming response cancelled mid-stream: push 6
      // chunks, leave the stream OPEN (so the user cancels a real in-flight
      // response), then Ctrl+C while isProcessing is still true.
      for (let i = 0; i < 6; i++) {
        await testCase.pushSendMessageResponse([assistantEvent(`chunk-${i} `)]);
      }
      await testCase.sendKeys('long answer please');
      await testCase.sleepMs(100);
      await testCase.pressEnter();
      // Wait for at least one chunk to land so cancel happens mid-stream.
      await testCase.waitForText('chunk-2', 15000);
      await testCase.pressCtrlC();
      await testCase.waitForStoreCondition((s) => !s.isProcessing, 15000);

      // After Ctrl+C: isProcessing false and only the turn-2 shell tool remains
      // in messages (turn 3 had no tool), so liveOutputs is empty too.
      s = await testCase.getStore();
      expect(s.isProcessing).toBe(false);
      expect(s.messages.filter((m) => m.role === 'tool_use').length).toBe(1);
      expect(Object.keys((s.liveOutputs as any) ?? {}).length).toBe(0);

      // Turn 4 — clean follow-up. Drain remaining turn-3 chunks by closing that
      // stream first so a fresh turn is allowed, then stream the turn-4 reply.
      const turn4Marker = 'KR_S1_TURN_FOUR_FINAL';
      await testCase.pushSendMessageResponse(null);
      await streamReply(testCase, turn4Marker);
      await testCase.sendKeys('final prompt');
      await testCase.sleepMs(100);
      await testCase.pressEnter();
      await testCase.waitForText(turn4Marker, 15000);
      await testCase.waitForIdle(15000);

      s = await testCase.getStore();
      expect(s.isProcessing).toBe(false);
      expect(s.queuedMessages.length).toBe(0);

      const afterTurn4Snapshot = testCase!.getSnapshot().join('\n');
      // Append-only: turns 1, 2, 4 markers all visible. (Turn 3 had partial
      // chunks committed via the cancel path — chunk-0 may or may not be
      // visible depending on how much of the stream flushed before the
      // cancel; we don't pin that.)
      expect(afterTurn4Snapshot).toContain(turn1Marker);
      expect(afterTurn4Snapshot).toContain(turn2Marker);
      expect(afterTurn4Snapshot).toContain(turn4Marker);
    }, 120000);
  }
);

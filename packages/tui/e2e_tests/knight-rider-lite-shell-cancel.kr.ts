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
import { launchLiteE2E, sendUserMessage } from './lite/helpers/commands';
import { assistantEvent, streamReply } from './lite/helpers/responses';

const KR_ENABLED = process.env.KIRO_RUN_KNIGHT_RIDER_TESTS === '1';

/** Push a real shell ToolUseEvent (+ terminating null) over the e2e agent IPC. */
async function pushShellTool(
  tc: E2ETestCase,
  toolUseId: string,
  command: string
): Promise<void> {
  await tc.pushSendMessageResponse([
    {
      kind: 'event',
      data: {
        kind: 'ToolUseEvent',
        data: {
          tool_use_id: toolUseId,
          name: 'shell',
          input: JSON.stringify({ command }),
          stop: true,
        },
      },
    },
  ]);
  await tc.pushSendMessageResponse(null);
}

describe.skipIf(!KR_ENABLED)(
  'knight rider — lite shell + cancel + recover',
  () => {
    let testCase: E2ETestCase | null = null;
    trackCleanup(() => testCase);

    it('survives shell streaming, mid-stream cancel, and a follow-up turn with append-only intact', async () => {
      testCase = await launchLiteE2E('knight-rider-lite-shell-cancel', {
        cliArgs: '--trust-tools=shell',
      });

      const turn1Marker = 'KR_S1_TURN_ONE_HELLO';
      await streamReply(testCase, turn1Marker);
      await sendUserMessage(testCase, 'hello');
      await testCase.waitForText(turn1Marker, 15000);
      await testCase.waitForIdle(15000);

      // Turn 2 — real shell tool streaming ~1.2s of output (3 lines, 0.4s gaps).
      const shellToolId = 'kr-s1-shell-tool';
      const turn2Marker = 'KR_S1_TURN_TWO_DONE';
      const cmd =
        process.platform === 'win32'
          ? '1..3 | ForEach-Object { Write-Output "kr-stream-line-$_"; Start-Sleep -Milliseconds 400 }'
          : 'for i in 1 2 3; do echo "kr-stream-line-$i"; sleep 0.4; done';
      await pushShellTool(testCase, shellToolId, cmd);
      await streamReply(testCase, turn2Marker);
      await sendUserMessage(testCase, 'run a shell');
      await testCase.waitForText(turn2Marker, 30000);
      await testCase.waitForIdle(20000);

      let s = await testCase.getStore();
      expect((s.liveOutputs as any)?.[shellToolId]).toBeUndefined();

      const beforeTurn3Snapshot = testCase.getSnapshot().join('\n');
      expect(beforeTurn3Snapshot).toContain(turn1Marker);
      expect(beforeTurn3Snapshot).toContain(turn2Marker);

      // Turn 3 — cancel a real in-flight response: push 6 chunks but leave the
      // stream OPEN, then Ctrl+C while isProcessing is still true. Regression:
      // cancel must not leave stale liveOutputs, stuck isProcessing, or an
      // orphan ToolUse row from the prior turn.
      for (let i = 0; i < 6; i++) {
        await testCase.pushSendMessageResponse([assistantEvent(`chunk-${i} `)]);
      }
      await sendUserMessage(testCase, 'long answer please');
      await testCase.waitForText('chunk-2', 15000);
      await testCase.pressCtrlC();
      await testCase.waitForStoreCondition((s) => !s.isProcessing, 15000);

      // Only the turn-2 shell tool remains (turn 3 had no tool) → no stale state.
      s = await testCase.getStore();
      expect(s.isProcessing).toBe(false);
      expect(s.messages.filter((m) => m.role === 'tool_use').length).toBe(1);
      expect(Object.keys((s.liveOutputs as any) ?? {}).length).toBe(0);

      // Turn 4 — drain the open turn-3 stream first, then a clean follow-up.
      const turn4Marker = 'KR_S1_TURN_FOUR_FINAL';
      await testCase.pushSendMessageResponse(null);
      await streamReply(testCase, turn4Marker);
      await sendUserMessage(testCase, 'final prompt');
      await testCase.waitForText(turn4Marker, 15000);
      await testCase.waitForIdle(15000);

      s = await testCase.getStore();
      expect(s.isProcessing).toBe(false);
      expect(s.queuedMessages.length).toBe(0);

      // Append-only: turns 1, 2, 4 markers all visible. Turn 3's partial chunks
      // may or may not have flushed before the cancel, so we don't pin them.
      const afterTurn4Snapshot = testCase!.getSnapshot().join('\n');
      expect(afterTurn4Snapshot).toContain(turn1Marker);
      expect(afterTurn4Snapshot).toContain(turn2Marker);
      expect(afterTurn4Snapshot).toContain(turn4Marker);
    }, 120000);
  }
);

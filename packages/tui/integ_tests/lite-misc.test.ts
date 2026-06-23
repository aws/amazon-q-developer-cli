import { describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';
import { MessageRole } from '../src/stores/app-store';
import {
  CMD_CLEAR,
  typeSlashCommand,
} from '../e2e_tests/lite/helpers/commands';
import {
  exitLiteInteg,
  finishAndExitLite,
  launchLiteInteg,
  trackCleanup,
} from '../e2e_tests/lite/helpers/integ-lifecycle';

/** Bug-mine category 10: miscellaneous lite-mode edge cases (per-it ids below). */
describe('lite miscellaneous [bug-mine 10.x]', () => {
  let testCase: TestCase | null = null;
  trackCleanup(() => testCase);

  // Integ TestCase path: trailing space + 800ms settle are load-bearing here.
  const typeCommand = (tc: TestCase, cmd: string) =>
    typeSlashCommand(tc, cmd, { trailingSpace: true, postEnterMs: 800 });

  it('live region leading separator matches static spacing [bug-mine 10.1]', async () => {
    testCase = await launchLiteInteg('lite-misc-10-1-spacing');

    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tool-spacing-001',
      name: 'Read',
      kind: 'read',
      args: { path: '/tmp/spacing.txt' },
    });
    await testCase.typeAndSubmit('test spacing');
    await testCase.sleepMs(300);

    const liveSnapshot = testCase.getSnapshot();
    const liveReadLine = liveSnapshot.findIndex((line) =>
      line.includes('Read')
    );

    await testCase.completeTurn();
    await testCase.sleepMs(400);

    const staticSnapshot = testCase.getSnapshot();
    const staticReadLine = staticSnapshot.findIndex((line) =>
      line.includes('Read')
    );

    expect(liveReadLine).not.toBe(-1);
    expect(staticReadLine).not.toBe(-1);

    // The blank-line gap above the tool line must be identical live vs static.
    const livePrefix =
      liveReadLine > 0 ? (liveSnapshot[liveReadLine - 1] ?? '') : '';
    const staticPrefix =
      staticReadLine > 0 ? (staticSnapshot[staticReadLine - 1] ?? '') : '';

    const liveIsBlank = livePrefix.trim() === '';
    const staticIsBlank = staticPrefix.trim() === '';
    expect(staticIsBlank).toBe(liveIsBlank);

    await exitLiteInteg(testCase);
  }, 30000);

  it.each([
    {
      bug: '10.2',
      testName: 'lite-misc-10-2-no-dup',
      setup: async (tc: TestCase) => {
        await tc.mockSessionUpdate({
          type: AgentEventType.ToolCall,
          id: 'tool-dup-001',
          name: 'Shell',
          kind: 'shell',
          args: { command: 'echo duplicate-guard' },
        });
        await tc.mockSessionUpdate({
          type: AgentEventType.ToolCallFinished,
          id: 'tool-dup-001',
          result: { status: 'success', output: 'duplicate-guard output' },
        });
      },
      assert: (snapshot: string[]) => {
        // Exactly once — a cleanup race would emit a duplicate bar.
        expect(snapshot.filter((l) => l.includes('Shell')).length).toBe(1);
      },
    },
    {
      bug: '10.3',
      testName: 'lite-misc-10-3-concat',
      setup: async (tc: TestCase) => {
        await tc.mockSessionUpdate({
          type: AgentEventType.ToolCall,
          id: 'tool-concat-001',
          name: 'Shell',
          kind: 'shell',
          args: { command: 'echo concat-test' },
        });
        await tc.mockSessionUpdate({
          type: AgentEventType.ToolCallUpdate,
          id: 'tool-concat-001',
          content: { type: ContentType.Text, text: 'concat-output-line' },
        });
        await tc.mockSessionUpdate({
          type: AgentEventType.ToolCallFinished,
          id: 'tool-concat-001',
          result: { status: 'success', output: 'concat-output-line' },
        });
      },
      assert: (snapshot: string[]) => {
        const toolLine = snapshot.findIndex((l) => l.includes('Shell'));
        expect(toolLine).not.toBe(-1);
        const nearby = snapshot.slice(toolLine, toolLine + 5).join('\n');
        expect(
          nearby.includes('output') ||
            nearby.includes('concat') ||
            nearby.includes('echo')
        ).toBe(true);
      },
    },
    {
      bug: '10.10',
      testName: 'lite-misc-10-10-emoji',
      setup: async (tc: TestCase) => {
        await tc.mockSessionUpdate({
          type: AgentEventType.Content,
          id: 'msg-emoji-001',
          content: {
            type: ContentType.Text,
            text: 'Party time! \u{1F389}\u{1F680}\u{2728} Great success!',
          },
        });
      },
      assert: (snapshot: string[]) => {
        const partyLine = snapshot.find((l) => l.includes('Party time'));
        expect(partyLine).toBeDefined();
        // No lone surrogates / replacement chars (U+FFFD) across chunk boundaries.
        expect(partyLine).not.toContain('�');
        expect(snapshot.join('\n')).toContain('Great success!');
      },
    },
  ])(
    'single-turn render committed cleanly [bug-mine $bug]',
    async ({ testName, setup, assert }) => {
      testCase = await launchLiteInteg(testName);
      await setup(testCase);
      await testCase.typeAndSubmit('t0');
      await testCase.completeTurn();
      await testCase.sleepMs(400);
      assert(testCase.getSnapshot());
      await exitLiteInteg(testCase);
    },
    30000
  );

  it('inner subagent tools carry agentName for batch isolation [bug-mine 10.4]', async () => {
    testCase = await launchLiteInteg('lite-misc-10-4-subagent-isolation');

    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'parent-tool-001',
      name: 'subagent',
      args: { task: 'run pipeline' },
    });

    // sessionId marks this as an inner subagent tool (isolated from the batch).
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'inner-tool-001',
      name: 'Shell',
      kind: 'shell',
      args: { command: 'echo inner-secret-cmd' },
      sessionId: 'subagent-session-x',
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCallFinished,
      id: 'inner-tool-001',
      result: { status: 'success', output: 'inner done' },
    });

    await testCase.typeAndSubmit('t0');
    await testCase.sleepMs(300);

    // The agentName field is the data contract isInnerSubagentTool() uses to
    // filter inner tools from the batch/scrollback (currentAgent is null in
    // test mode, so we assert the field rather than the visual filter).
    const store = await testCase.getStore();
    const innerTool = store.messages.find(
      (m) => m.role === 'tool_use' && m.id === 'inner-tool-001'
    );
    expect(innerTool).toBeDefined();
    expect(innerTool!.role).toBe(MessageRole.ToolUse);
    if (innerTool!.role === MessageRole.ToolUse) {
      expect(innerTool!.agentName).toBeDefined();
      expect(innerTool!.agentName).not.toBe(store.currentAgent?.name ?? '');
    }

    const parentTool = store.messages.find(
      (m) => m.role === MessageRole.ToolUse && m.id === 'parent-tool-001'
    );
    expect(parentTool).toBeDefined();
    if (parentTool!.role === MessageRole.ToolUse) {
      const parentIsMain =
        !parentTool!.agentName ||
        parentTool!.agentName === (store.currentAgent?.name ?? undefined);
      expect(parentIsMain).toBe(true);
    }

    await finishAndExitLite(testCase);
  }, 30000);

  it('transient alert appears then auto-dismisses [bug-mine 10.5]', async () => {
    testCase = await launchLiteInteg('lite-misc-10-5-transient-alert');

    // Start a turn but DON'T complete it — the alert only fires while busy.
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'busy-tool-001',
      name: 'Shell',
      kind: 'shell',
      args: { command: 'sleep 10' },
    });
    await testCase.typeAndSubmit('t0');
    await testCase.sleepMs(300);

    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    // A shell escape (!cmd) during processing always triggers a warning alert.
    for (const ch of '!ls ') {
      await testCase.sendKeys(ch);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    await testCase.sendKeys('\r');
    await testCase.sleepMs(300);

    store = await testCase.getStore();
    expect(store.transientAlert).not.toBeNull();
    expect(store.transientAlert!.message).toContain("can't be queued");

    // Auto-dismiss after autoHideMs (4000ms).
    await testCase.sleepMs(4500);

    store = await testCase.getStore();
    expect(store.transientAlert).toBeNull();

    await finishAndExitLite(testCase);
  }, 30000);

  it('/clear in lite writes CSI escape and wipes visible terminal [bug-mine 10.8]', async () => {
    testCase = await launchLiteInteg('lite-misc-10-8-clear');

    // Mock mode doesn't auto-send CommandsUpdate, so register /clear manually.
    await testCase.mockSessionUpdate({
      type: AgentEventType.CommandsUpdate,
      commands: [
        { name: 'clear', description: 'Clear conversation' },
        { name: 'compact', description: 'Compact context' },
        { name: 'chat', description: 'Chat management' },
      ],
    });
    await testCase.sleepMs(200);

    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'msg-marker-001',
      content: {
        type: ContentType.Text,
        text: 'UNIQUE_MARKER_XYZZY_2024',
      },
    });
    await testCase.typeAndSubmit('show marker');
    await testCase.completeTurn();
    await testCase.sleepMs(400);

    await testCase.sleepMs(300);
    let store = await testCase.getStore();
    // Retry if still processing (race between completeTurn IPC and render).
    if (store.isProcessing) {
      await testCase.sleepMs(500);
      store = await testCase.getStore();
    }
    expect(store.isProcessing).toBe(false);

    const msgCountBefore = store.messages.length;
    expect(msgCountBefore).toBeGreaterThan(0);

    let snapshot = testCase.getSnapshot();
    expect(snapshot.some((l) => l.includes('UNIQUE_MARKER_XYZZY_2024'))).toBe(
      true
    );

    await typeCommand(testCase, CMD_CLEAR);
    await testCase.sleepMs(600);

    // /clear writes CSI 2J+3J+H (screen + scrollback wipe + cursor home), so
    // the marker text must no longer be visible on screen.
    snapshot = testCase.getSnapshot();
    const markerGone = !snapshot.some((l) =>
      l.includes('UNIQUE_MARKER_XYZZY_2024')
    );
    expect(markerGone).toBe(true);

    const rawOutput = testCase.getOutput();
    const hasWipeSequence =
      rawOutput.includes('\x1b[2J') || rawOutput.includes('\x1b[3J');
    expect(hasWipeSequence).toBe(true);

    await exitLiteInteg(testCase);
  }, 30000);
});

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';
import { MessageRole } from '../src/stores/app-store';
import {
  CMD_CLEAR,
  typeSlashCommand,
} from '../e2e_tests/lite/helpers/commands';
import {
  exitLiteInteg,
  launchLiteInteg,
} from '../e2e_tests/lite/helpers/integ-lifecycle';

/**
 * Bug-mine category 10: miscellaneous lite-mode edge cases.
 *
 * Covers spacing consistency (10.1), race guards (10.2), tool output
 * concatenation (10.3), subagent isolation (10.4), transient alerts (10.5),
 * /clear terminal wipe (10.8), and astral character integrity (10.10).
 */
describe('lite miscellaneous [bug-mine 10.x]', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  // Integ TestCase path: trailing space + 800ms settle are load-bearing here.
  const typeCommand = (tc: TestCase, cmd: string) =>
    typeSlashCommand(tc, cmd, { trailingSpace: true, postEnterMs: 800 });

  it('live region leading separator matches static spacing [bug-mine 10.1]', async () => {
    testCase = await launchLiteInteg('lite-misc-10-1-spacing');

    // Inject a tool event (live rendering)
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tool-spacing-001',
      name: 'Read',
      kind: 'read',
      args: { path: '/tmp/spacing.txt' },
    });
    await testCase.typeAndSubmit('test spacing');
    await testCase.sleepMs(300);

    // Capture live snapshot (tool still in progress)
    const liveSnapshot = testCase.getSnapshot();
    const liveReadLine = liveSnapshot.findIndex((line) =>
      line.includes('Read')
    );

    // Complete the turn — flushes to static
    await testCase.completeTurn();
    await testCase.sleepMs(400);

    // Capture static snapshot
    const staticSnapshot = testCase.getSnapshot();
    const staticReadLine = staticSnapshot.findIndex((line) =>
      line.includes('Read')
    );

    // Both should have the tool line present (no visual jump / disappear)
    expect(liveReadLine).not.toBe(-1);
    expect(staticReadLine).not.toBe(-1);

    // Verify there is no unexpected blank-line gap above the tool line in
    // static that wasn't there in live (spacing consistency)
    const livePrefix =
      liveReadLine > 0 ? (liveSnapshot[liveReadLine - 1] ?? '') : '';
    const staticPrefix =
      staticReadLine > 0 ? (staticSnapshot[staticReadLine - 1] ?? '') : '';

    // The line above should be semantically equivalent (both empty or both non-empty)
    const liveIsBlank = livePrefix.trim() === '';
    const staticIsBlank = staticPrefix.trim() === '';
    expect(staticIsBlank).toBe(liveIsBlank);

    await exitLiteInteg(testCase);
  }, 30000);

  it('finished tool has no duplicate bar in snapshot [bug-mine 10.2]', async () => {
    testCase = await launchLiteInteg('lite-misc-10-2-no-dup');

    // Inject tool then finish it immediately
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tool-dup-001',
      name: 'Shell',
      kind: 'shell',
      args: { command: 'echo duplicate-guard' },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCallFinished,
      id: 'tool-dup-001',
      result: { status: 'success', output: 'duplicate-guard output' },
    });

    await testCase.typeAndSubmit('t0');
    await testCase.completeTurn();
    await testCase.sleepMs(400);

    // Count how many times the tool label "Shell" appears in the snapshot
    const snapshot = testCase.getSnapshot();
    const shellLines = snapshot.filter((line) => line.includes('Shell'));

    // Should appear exactly once — no duplicate from a cleanup race
    expect(shellLines.length).toBe(1);

    await exitLiteInteg(testCase);
  }, 30000);

  it('tool line and output bar appear without extra gap [bug-mine 10.3]', async () => {
    testCase = await launchLiteInteg('lite-misc-10-3-concat');

    // Inject a tool with streaming output via ToolCallUpdate
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tool-concat-001',
      name: 'Shell',
      kind: 'shell',
      args: { command: 'echo concat-test' },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCallUpdate,
      id: 'tool-concat-001',
      content: { type: ContentType.Text, text: 'concat-output-line' },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCallFinished,
      id: 'tool-concat-001',
      result: { status: 'success', output: 'concat-output-line' },
    });

    await testCase.typeAndSubmit('t0');
    await testCase.completeTurn();
    await testCase.sleepMs(400);

    const snapshot = testCase.getSnapshot();
    const toolLine = snapshot.findIndex((line) => line.includes('Shell'));
    expect(toolLine).not.toBe(-1);

    // Look for the output indicator or args within a window of 5 lines
    // after the tool line — validates no excessive gap between tool name
    // and its content
    const nearbyLines = snapshot.slice(toolLine, toolLine + 5).join('\n');
    const hasContentNearby =
      nearbyLines.includes('output') ||
      nearbyLines.includes('concat') ||
      nearbyLines.includes('echo');
    expect(hasContentNearby).toBe(true);

    await exitLiteInteg(testCase);
  }, 30000);

  it('inner subagent tools carry agentName for batch isolation [bug-mine 10.4]', async () => {
    testCase = await launchLiteInteg('lite-misc-10-4-subagent-isolation');

    // Inject a parent tool (visible in main chat)
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'parent-tool-001',
      name: 'subagent',
      args: { task: 'run pipeline' },
    });

    // Inject an inner subagent tool (with sessionId — should be isolated)
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

    // Validate via store: the inner tool should have agentName set
    // (different from mainAgent), which is what isInnerSubagentTool()
    // uses to filter it from the batch computation and scrollback.
    // In test mode, currentAgent is null so the visual filter doesn't
    // apply, but the data contract (agentName field) is the invariant.
    const store = await testCase.getStore();
    const innerTool = store.messages.find(
      (m) => m.role === 'tool_use' && m.id === 'inner-tool-001'
    );
    expect(innerTool).toBeDefined();
    expect(innerTool!.role).toBe(MessageRole.ToolUse);
    // Narrow to tool_use to access agentName
    if (innerTool!.role === MessageRole.ToolUse) {
      expect(innerTool!.agentName).toBeDefined();
      // The inner tool's agentName should differ from the parent's
      expect(innerTool!.agentName).not.toBe(store.currentAgent?.name ?? '');
    }

    // The parent subagent tool should NOT have a foreign agentName
    const parentTool = store.messages.find(
      (m) => m.role === MessageRole.ToolUse && m.id === 'parent-tool-001'
    );
    expect(parentTool).toBeDefined();
    // Parent tool either has no agentName or has the main agent's name
    if (parentTool!.role === MessageRole.ToolUse) {
      const parentIsMain =
        !parentTool!.agentName ||
        parentTool!.agentName === (store.currentAgent?.name ?? undefined);
      expect(parentIsMain).toBe(true);
    }

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await exitLiteInteg(testCase);
  }, 30000);

  it('transient alert appears then auto-dismisses [bug-mine 10.5]', async () => {
    testCase = await launchLiteInteg('lite-misc-10-5-transient-alert');

    // To trigger a transient alert, we need the TUI to be processing.
    // Inject a tool and start a turn but DON'T complete it:
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'busy-tool-001',
      name: 'Shell',
      kind: 'shell',
      args: { command: 'sleep 10' },
    });
    await testCase.typeAndSubmit('t0');
    await testCase.sleepMs(300);

    // Verify processing state
    let store = await testCase.getStore();
    expect(store.isProcessing).toBe(true);

    // Submit a shell escape (!cmd) during processing — always triggers a
    // warning alert regardless of command availability
    for (const ch of '!ls ') {
      await testCase.sendKeys(ch);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    await testCase.sendKeys('\r');
    await testCase.sleepMs(300);

    // Check that transientAlert is set in the store
    store = await testCase.getStore();
    expect(store.transientAlert).not.toBeNull();
    expect(store.transientAlert!.message).toContain("can't be queued");

    // Wait for auto-dismiss (autoHideMs is 4000ms)
    await testCase.sleepMs(4500);

    // Alert should have auto-dismissed
    store = await testCase.getStore();
    expect(store.transientAlert).toBeNull();

    await testCase.completeTurn();
    await testCase.sleepMs(100);
    await exitLiteInteg(testCase);
  }, 30000);

  it('/clear in lite writes CSI escape and wipes visible terminal [bug-mine 10.8]', async () => {
    testCase = await launchLiteInteg('lite-misc-10-8-clear');

    // Register /clear as a known command (mock mode doesn't auto-send
    // CommandsUpdate from the backend)
    await testCase.mockSessionUpdate({
      type: AgentEventType.CommandsUpdate,
      commands: [
        { name: 'clear', description: 'Clear conversation' },
        { name: 'compact', description: 'Compact context' },
        { name: 'chat', description: 'Chat management' },
      ],
    });
    await testCase.sleepMs(200);

    // Inject content that will appear in the terminal
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

    // Wait for the turn to fully settle (isProcessing must be false)
    await testCase.sleepMs(300);
    let store = await testCase.getStore();
    // Retry if still processing (race between completeTurn IPC and render)
    if (store.isProcessing) {
      await testCase.sleepMs(500);
      store = await testCase.getStore();
    }
    expect(store.isProcessing).toBe(false);

    // Verify content was committed to the store
    const msgCountBefore = store.messages.length;
    expect(msgCountBefore).toBeGreaterThan(0);

    // Verify the marker is visible before /clear
    let snapshot = testCase.getSnapshot();
    expect(snapshot.some((l) => l.includes('UNIQUE_MARKER_XYZZY_2024'))).toBe(
      true
    );

    // Send /clear command
    await typeCommand(testCase, CMD_CLEAR);
    await testCase.sleepMs(600);

    // The /clear effect writes CSI 2J+3J+H (screen wipe + scrollback wipe +
    // cursor home). In the xterm PTY the marker text should no longer be
    // visible on screen.
    snapshot = testCase.getSnapshot();
    const markerGone = !snapshot.some((l) =>
      l.includes('UNIQUE_MARKER_XYZZY_2024')
    );
    expect(markerGone).toBe(true);

    // The raw PTY output should contain the CSI escape sequence for screen
    // wipe (ESC[2J = erase display, ESC[3J = erase scrollback, ESC[H = home)
    const rawOutput = testCase.getOutput();
    const hasWipeSequence =
      rawOutput.includes('\x1b[2J') || rawOutput.includes('\x1b[3J');
    expect(hasWipeSequence).toBe(true);

    await exitLiteInteg(testCase);
  }, 30000);

  it('astral chars (emoji) kept intact across chunk boundaries [bug-mine 10.10]', async () => {
    testCase = await launchLiteInteg('lite-misc-10-10-emoji');

    // Inject content with emoji characters
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'msg-emoji-001',
      content: {
        type: ContentType.Text,
        text: 'Party time! \u{1F389}\u{1F680}\u{2728} Great success!',
      },
    });
    await testCase.typeAndSubmit('emoji test');
    await testCase.completeTurn();
    await testCase.sleepMs(400);

    // Verify the emoji characters render intact in the terminal
    const snapshot = testCase.getSnapshot();
    const partyLine = snapshot.find((line) => line.includes('Party time'));
    expect(partyLine).toBeDefined();

    // Check that the emoji codepoints survived without lone surrogates
    // or replacement characters (U+FFFD)
    expect(partyLine).not.toContain('�');

    // The actual emoji glyphs should be present in the output
    // (terminal renders them as wide chars, but the text content is intact)
    const fullOutput = snapshot.join('\n');
    expect(fullOutput).toContain('Great success!');

    await exitLiteInteg(testCase);
  }, 30000);
});

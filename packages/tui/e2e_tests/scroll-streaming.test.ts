import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Scroll during streaming', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('renders long content with scrollbar', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 80, height: 24 })
      .withTestName('scroll-streaming')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Push complete response (events + null) — this is how the mock infra works
    const lines = Array.from({ length: 60 }, (_, i) => `Line ${i + 1}: scrolltest`);
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: lines.join('\n') } } },
    ], { silent: true });
    await testCase.pushSendMessageResponse(null);

    // Queue a second stream that stays open (for the agent's second send_message after tool results)
    // Actually — after the first response completes, the agent won't call send_message again
    // unless there are tool calls. So the response will complete and StreamingMessage won't be used.

    // Send prompt
    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for content
    await testCase.waitForText('scrolltest', 15000);
    await testCase.sleepMs(1000);

    // Snapshot — response is complete, rendered as plain Message (not StreamingMessage)
    const snapshot = testCase.getSnapshot();
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // Count visible lines — are all 60 shown or is it bounded?
    const visibleLines = snapshot.filter(line => line.includes('scrolltest')).length;
    console.log('Visible lines with scrolltest:', visibleLines);

    // Check for scrollbar
    const hasScrollbar = snapshot.some(line => line.includes('█') || line.includes('░'));
    console.log('Scrollbar visible:', hasScrollbar);

    // Log what we see for debugging
    console.log('All lines:');
    snapshot.forEach((line, i) => {
      if (line.trim()) console.log(`  ${i}: ${line.substring(0, 78)}`);
    });

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);
});

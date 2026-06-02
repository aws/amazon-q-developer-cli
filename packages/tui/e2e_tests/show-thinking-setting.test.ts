/**
 * E2E test: `chat.showThinking` gates the `<ThinkingDisplay>` render in the
 * conversation view.
 *
 * Default (true): the header and the reasoning lines are visible.
 * Opt-out (false): the agent's reasoning text is ingested into the store
 *   (Model message has `thinking` populated) but the "Thinking" header
 *   from `ThinkingDisplay` is not rendered.
 *
 * The live "Thinking..." spinner (`ThinkingMessage`) is a separate
 * affordance and is not affected by this setting; we don't assert on it
 * here because it depends on processing state at snapshot time.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('chat.showThinking setting', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('hides <ThinkingDisplay> when chat.showThinking=false while still ingesting reasoning into the store', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('show-thinking-default-hidden')
      .withTerminal({ width: 120, height: 40 })
      .withGlobalSettings({ 'chat.showThinking': false })
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ReasoningEvent',
          data: {
            text:
              'Reasoning step one.\n' +
              'Reasoning step two.\n' +
              'Reasoning step three.',
          },
        },
      },
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: 'Final answer.' } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('hello');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Final answer.', 15000);

    const snapshot = testCase.getSnapshot().join('\n');

    // None of the reasoning lines should appear in the rendered output.
    expect(snapshot).not.toContain('Reasoning step one.');
    expect(snapshot).not.toContain('Reasoning step two.');
    expect(snapshot).not.toContain('Reasoning step three.');

    // But the data should still be present in the store, proving the gate is
    // render-only and not an ingestion-level drop.
    const store = await testCase.getStore();
    const modelMsg = store.messages.find(
      (m): m is typeof m & { role: 'model' } =>
        m.role === 'model' && m.content.includes('Final answer.')
    );
    expect(modelMsg).toBeTruthy();
    const thinking = (modelMsg as any)?.thinking as string | undefined;
    expect(thinking).toBeTruthy();
    expect(thinking).toContain('Reasoning step one.');
    expect(thinking).toContain('Reasoning step three.');

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 30000);

  it('renders <ThinkingDisplay> collapsed (header only) when chat.showThinking=true', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('show-thinking-collapsed')
      .withTerminal({ width: 120, height: 40 })
      .withGlobalSettings({ 'chat.showThinking': true })
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ReasoningEvent',
          data: {
            text:
              'Reasoning step one.\n' +
              'Reasoning step two.\n' +
              'Reasoning step three.',
          },
        },
      },
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: 'Final answer.' } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('hello');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Final answer.', 15000);
    // Legacy `true` maps to the collapsed mode: a header + "ctrl+o to view"
    // hint, but the reasoning body stays hidden until ctrl+o.
    await testCase.waitForText('ctrl+o to view', 5000);

    const snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).not.toContain('Reasoning step one.');
    expect(snapshot).not.toContain('Reasoning step three.');

    // ctrl+o expands the full reasoning stream.
    await testCase.sendKeys([0x0f]);
    await testCase.sleepMs(300);
    const expanded = testCase.getSnapshot().join('\n');
    expect(expanded).toContain('Reasoning step one.');
    expect(expanded).toContain('Reasoning step three.');

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 30000);

  it('renders reasoning inline when chat.showThinking=expanded', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('show-thinking-expanded')
      .withTerminal({ width: 120, height: 40 })
      .withGlobalSettings({ 'chat.showThinking': 'expanded' })
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ReasoningEvent',
          data: {
            text:
              'Reasoning step one.\n' +
              'Reasoning step two.\n' +
              'Reasoning step three.',
          },
        },
      },
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: 'Final answer.' } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('hello');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Final answer.', 15000);
    // expanded mode shows the full stream inline without any ctrl+o press.
    await testCase.waitForText('Reasoning step three.', 5000);

    const snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).toContain('Reasoning step one.');
    expect(snapshot).toContain('Reasoning step three.');

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 30000);
});

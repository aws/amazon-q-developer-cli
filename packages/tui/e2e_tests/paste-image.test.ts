/**
 * E2E tests for image pasting via Ctrl+V.
 *
 * Ctrl+V sends \x16 to the terminal, which Ink parses as { ctrl: true, name: 'v' }.
 * The handler calls executeCommand({ command: 'pasteImage' }) on the backend,
 * which reads the system clipboard. In mock mode, the MockSessionClient returns
 * a fake 100×50 PNG image.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

// Ctrl+V byte
const CTRL_V = '\x16';

describe('Paste Image', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('Ctrl+V inserts an image chip into the input', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-image-chip')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Send Ctrl+V to trigger image paste
    await testCase.sendKeys(CTRL_V);
    await testCase.sleepMs(500);

    // The mock returns a 100×50 image with 1024 bytes (1.0 KB)
    // PastedChip renders: "pasted image (100×50 1.0 KB)"
    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('pasted image');

    // Verify the store has a pending image
    const store = await testCase.getStore();
    expect(store.pendingImages.length).toBe(1);
    expect(store.pendingImages[0]!.width).toBe(100);
    expect(store.pendingImages[0]!.height).toBe(50);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);

  it('Ctrl+V image chip followed by text and submit', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-image-submit')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Push a mock response for the message submission
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: 'I see the image.' } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    // Paste image then type text
    await testCase.sendKeys(CTRL_V);
    await testCase.sleepMs(500);
    await testCase.sendKeys('describe this');
    await testCase.sleepMs(200);

    // Verify both image chip and text are visible
    const snapshot = testCase.getSnapshot();
    const screenText = snapshot.join('\n');
    expect(screenText).toContain('pasted image');
    expect(screenText).toContain('describe this');

    // Submit
    await testCase.pressEnter();
    await testCase.waitForText('I see the image.', 10000);

    // After submit, pending images should be cleared
    const store = await testCase.getStore();
    expect(store.pendingImages.length).toBe(0);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);

  it('multiple Ctrl+V pastes add multiple image chips', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('paste-image-multiple')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Paste two images
    await testCase.sendKeys(CTRL_V);
    await testCase.sleepMs(500);
    await testCase.sendKeys(CTRL_V);
    await testCase.sleepMs(500);

    // Verify the store has two pending images
    const store = await testCase.getStore();
    expect(store.pendingImages.length).toBe(2);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 30000);
});

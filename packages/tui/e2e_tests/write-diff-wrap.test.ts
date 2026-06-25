/**
 * E2E test verifying that long diff lines wrap instead of being truncated.
 *
 * Uses a narrow terminal (40 columns) and a diff line longer than 40 chars
 * to confirm the full content is visible across wrapped rows.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

describe('Write diff wrapping', () => {
  let testCase: E2ETestCase | null = null;
  let tempDir: string = '';

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    if (tempDir) {
      try { fs.rmSync(tempDir, { recursive: true }); } catch { /* ignore */ }
      tempDir = '';
    }
  });

  it('wraps long diff lines instead of truncating with ellipsis', async () => {
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-e2e-wrap-'));
    const filePath = path.join(tempDir, 'test.ts');
    const oldLine = 'export const short = "old";';
    const newLine = 'export const thisIsAVeryLongVariableName = "this value is intentionally long to exceed terminal width";';
    fs.writeFileSync(filePath, oldLine + '\n');

    testCase = await E2ETestCase.builder()
      .withTestName('write-diff-wrap')
      .withTerminal({ width: 60, height: 30 })
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-wrap-1',
            name: 'write',
            input: JSON.stringify({
              command: 'strReplace',
              path: filePath,
              oldStr: oldLine,
              newStr: newLine,
            }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Done.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('replace line');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('requires approval', 15000);

    const snapshot = testCase.getSnapshot();
    const allText = snapshot.join('');
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    // The full new line content should be visible (wrapped across rows), not truncated
    expect(allText).toContain('exceed terminal width');
    // No ellipsis truncation character
    expect(allText).not.toContain('…');
  }, 30000);
});

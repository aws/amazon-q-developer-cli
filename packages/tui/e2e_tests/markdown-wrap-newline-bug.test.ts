import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

/**
 * Bug: blank line before a **bold heading** is swallowed when the preceding
 * list item wraps so that its last word fills the final cell of a terminal line.
 *
 * Markdown input (note the blank line before **Bedrock Converse API**):
 *
 *   **Anthropic Messages API**
 *   - `redacted_thinking` block — a **separate block type** with just a `data` field (the encrypted blob)
 *
 *   **Bedrock Converse API**
 *
 * At terminal width 94 the list item wraps so "blob)" lands at exactly the
 * last cell of the line. The blank line before the bold heading disappears.
 *
 * - Width 93 (blob) wraps with room to spare): blank line preserved ✓
 * - Width 94 (blob) fills last cell exactly): blank line missing ✗
 * - Width 95 (blob) stays on previous line): blank line preserved ✓
 */

const MARKDOWN = [
  '**Anthropic Messages API**',
  '- `redacted_thinking` block — a **separate block type** with just a `data` field (the encrypted blob)',
  '',
  '**Bedrock Converse API**',
].join('\n');

describe('Markdown wrap newline bug', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('blank line before bold heading is preserved when list item wraps at exact boundary (width=94)', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('wrap-newline-bug')
      .withTerminal({ width: 94, height: 30 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: MARKDOWN } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Bedrock', 15000);
    await testCase.waitForIdle(15000);
    await testCase.sleepMs(500);

    const snapshot = testCase.getSnapshot();
    console.log('\n── snapshot ──');
    console.log(testCase.getSnapshotFormatted());

    const blobIdx = snapshot.findIndex(l => l.includes('blob)'));
    const bedrockIdx = snapshot.findIndex(l => l.includes('Bedrock'));

    expect(blobIdx).toBeGreaterThan(-1);
    expect(bedrockIdx).toBeGreaterThan(-1);

    // The markdown has a blank line before **Bedrock Converse API**.
    // The rendered output must have at least one blank line (gap >= 2).
    const gap = bedrockIdx - blobIdx;
    console.log(`blob) at line ${blobIdx}, Bedrock at line ${bedrockIdx}, gap=${gap}`);
    expect(gap).toBeGreaterThanOrEqual(2);
  }, 30000);
});

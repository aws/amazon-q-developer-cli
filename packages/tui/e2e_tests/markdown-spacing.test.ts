import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

/**
 * Count blank lines between two content lines in a terminal snapshot.
 * Returns the number of empty/whitespace-only lines between the line
 * containing `textA` and the line containing `textB`.
 */
function blankLinesBetween(snapshot: string[], textA: string, textB: string): number {
  const idxA = snapshot.findIndex(l => l.includes(textA));
  const idxB = snapshot.findIndex(l => l.includes(textB));
  if (idxA === -1) throw new Error(`"${textA}" not found in snapshot`);
  if (idxB === -1) throw new Error(`"${textB}" not found in snapshot`);
  const [start, end] = idxA < idxB ? [idxA, idxB] : [idxB, idxA];
  let blanks = 0;
  for (let i = start + 1; i < end; i++) {
    if (snapshot[i]!.trim() === '') blanks++;
  }
  return blanks;
}

/** Helper to launch a test case, send a markdown response, and return the snapshot. */
async function renderMarkdown(testName: string, content: string): Promise<{ testCase: E2ETestCase; snapshot: string[] }> {
  const testCase = await E2ETestCase.builder()
    .withTestName(testName)
    .withTerminal({ width: 100, height: 50 })
    .launch();
  await testCase.waitForText('ask a question', 10000);
  await testCase.getSessionId();

  await testCase.pushSendMessageResponse([
    { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content } } },
  ]);
  await testCase.pushSendMessageResponse(null);

  await testCase.sendKeys('test');
  await testCase.sleepMs(100);
  await testCase.pressEnter();
  await testCase.waitForIdle(15000);
  await testCase.sleepMs(500);

  const snapshot = testCase.getSnapshot();
  console.log(`\n── ${testName} snapshot ──`);
  console.log(testCase.getSnapshotFormatted());

  return { testCase, snapshot };
}

describe('Markdown Spacing', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  // ── Double newline bugs (expect exactly 1 blank line) ──

  it('code → code: exactly 1 blank line between consecutive code blocks', async () => {
    const result = await renderMarkdown('spacing-code-code',
      '```js\nconst a = 1;\n```\n\n```python\nx = 2\n```');
    testCase = result.testCase;
    const blanks = blankLinesBetween(result.snapshot, 'const a', 'x = 2');
    console.log('code→code blanks:', blanks);
    // BUG: marginBottom on first code + marginTop on second = 2 blank lines
    expect(blanks).toBe(1);
  }, 30000);

  it('code → header: exactly 1 blank line', async () => {
    const result = await renderMarkdown('spacing-code-header',
      '```js\nconst a = 1;\n```\n\n## Next Section');
    testCase = result.testCase;
    const blanks = blankLinesBetween(result.snapshot, 'const a', 'Next Section');
    console.log('code→header blanks:', blanks);
    expect(blanks).toBe(1);
  }, 30000);

  it('header → code: exactly 1 blank line', async () => {
    const result = await renderMarkdown('spacing-header-code',
      '## Title\n\n```js\nconst a = 1;\n```');
    testCase = result.testCase;
    const blanks = blankLinesBetween(result.snapshot, 'Title', 'const a');
    console.log('header→code blanks:', blanks);
    expect(blanks).toBe(1);
  }, 30000);

  // ── Missing newline bugs (expect exactly 1 blank line) ──

  it('text → list: exactly 1 blank line', async () => {
    const result = await renderMarkdown('spacing-text-list',
      'Here are the items:\n\n- First\n- Second');
    testCase = result.testCase;
    const blanks = blankLinesBetween(result.snapshot, 'Here are the items', '- First');
    console.log('text→list blanks:', blanks);
    // BUG: list items have no marginTop, so 0 blank lines
    expect(blanks).toBe(1);
  }, 30000);

  it('list → text: exactly 1 blank line', async () => {
    const result = await renderMarkdown('spacing-list-text',
      '- First\n- Second\n\nSome follow-up text.');
    testCase = result.testCase;
    const blanks = blankLinesBetween(result.snapshot, '- Second', 'follow-up text');
    console.log('list→text blanks:', blanks);
    expect(blanks).toBe(1);
  }, 30000);

  it('text → blockquote: exactly 1 blank line', async () => {
    const result = await renderMarkdown('spacing-text-blockquote',
      'Important:\n\n> This is a quote');
    testCase = result.testCase;
    const blanks = blankLinesBetween(result.snapshot, 'Important', 'This is a quote');
    console.log('text→blockquote blanks:', blanks);
    expect(blanks).toBe(1);
  }, 30000);

  it('blockquote → text: exactly 1 blank line', async () => {
    const result = await renderMarkdown('spacing-blockquote-text',
      '> A quote\n\nNormal text after.');
    testCase = result.testCase;
    const blanks = blankLinesBetween(result.snapshot, 'A quote', 'Normal text after');
    console.log('blockquote→text blanks:', blanks);
    expect(blanks).toBe(1);
  }, 30000);

  it('text → table: exactly 1 blank line', async () => {
    const result = await renderMarkdown('spacing-text-table',
      'Results:\n\n| A | B |\n|---|---|\n| 1 | 2 |');
    testCase = result.testCase;
    // Table renders with box-drawing; top border has ┌
    const snapshot = result.snapshot;
    testCase = result.testCase;
    const textIdx = snapshot.findIndex(l => l.includes('Results'));
    const tableIdx = snapshot.findIndex(l => l.includes('┌') || l.includes('A'));
    expect(textIdx).toBeGreaterThan(-1);
    expect(tableIdx).toBeGreaterThan(-1);
    let blanks = 0;
    for (let i = textIdx + 1; i < tableIdx; i++) {
      if (snapshot[i]!.trim() === '') blanks++;
    }
    console.log('text→table blanks:', blanks);
    expect(blanks).toBe(1);
  }, 30000);

  it('table → text: exactly 1 blank line', async () => {
    const result = await renderMarkdown('spacing-table-text',
      '| A | B |\n|---|---|\n| 1 | 2 |\n\nAfter the table.');
    testCase = result.testCase;
    const snapshot = result.snapshot;
    // Find last table row (contains └) or data row
    const tableEndIdx = snapshot.findLastIndex(l => l.includes('└'));
    const textIdx = snapshot.findIndex(l => l.includes('After the table'));
    expect(tableEndIdx).toBeGreaterThan(-1);
    expect(textIdx).toBeGreaterThan(-1);
    let blanks = 0;
    for (let i = tableEndIdx + 1; i < textIdx; i++) {
      if (snapshot[i]!.trim() === '') blanks++;
    }
    console.log('table→text blanks:', blanks);
    expect(blanks).toBe(1);
  }, 30000);

  it('header → list: exactly 1 blank line', async () => {
    const result = await renderMarkdown('spacing-header-list',
      '## Features\n\n- Alpha\n- Beta');
    testCase = result.testCase;
    const blanks = blankLinesBetween(result.snapshot, 'Features', '- Alpha');
    console.log('header→list blanks:', blanks);
    expect(blanks).toBe(1);
  }, 30000);

  it('list → header: exactly 1 blank line', async () => {
    const result = await renderMarkdown('spacing-list-header',
      '- Alpha\n- Beta\n\n## Next Section');
    testCase = result.testCase;
    const blanks = blankLinesBetween(result.snapshot, '- Beta', 'Next Section');
    console.log('list→header blanks:', blanks);
    expect(blanks).toBe(1);
  }, 30000);

  it('hr → text: exactly 1 blank line', async () => {
    const result = await renderMarkdown('spacing-hr-text',
      'Before the rule.\n\n---\n\nAfter the rule.');
    testCase = result.testCase;
    const snapshot = result.snapshot;
    const beforeIdx = snapshot.findIndex(l => l.includes('Before the rule'));
    const afterIdx = snapshot.findIndex(l => l.includes('After the rule'));
    expect(beforeIdx).toBeGreaterThan(-1);
    expect(afterIdx).toBeGreaterThan(-1);
    // There should be exactly 3 lines between: blank, hr, blank
    // (before → blank → hr → blank → after)
    const gap = afterIdx - beforeIdx;
    console.log('hr→text gap (before to after):', gap);
    expect(gap).toBe(4); // before, blank, hr, blank, after
  }, 30000);

  // ── Consistency: all block transitions should have uniform spacing ──

  it('LLM-style response: uniform 1 blank line between all sections', async () => {
    const result = await renderMarkdown('spacing-full-response',
      'Here is the solution:\n\n## Step 1\n\n```python\ndef solve():\n    pass\n```\n\n## Step 2\n\n- Do this\n- Do that\n\n> Note: be careful\n\n| Col | Val |\n|-----|-----|\n| x   | 1   |\n\nDone.');
    testCase = result.testCase;
    const snapshot = result.snapshot;

    // Verify all transitions have exactly 1 blank line
    const checks: [string, string][] = [
      ['Here is the solution', 'Step 1'],
      ['Step 1', 'def solve'],
      ['pass', 'Step 2'],
      ['Step 2', '- Do this'],
      ['- Do that', 'Note: be careful'],
      ['Done', 'Done'], // just verify it exists
    ];

    for (const [a, b] of checks) {
      if (a === b) {
        expect(snapshot.some(l => l.includes(a))).toBe(true);
        continue;
      }
      const blanks = blankLinesBetween(snapshot, a, b);
      console.log(`${a} → ${b}: ${blanks} blank lines`);
      expect(blanks).toBe(1);
    }
  }, 30000);

  // ── No blank line within consecutive list items ──

  it('consecutive list items: 0 blank lines between them', async () => {
    const result = await renderMarkdown('spacing-list-items',
      '- First\n- Second\n- Third');
    testCase = result.testCase;
    const blanks1 = blankLinesBetween(result.snapshot, '- First', '- Second');
    const blanks2 = blankLinesBetween(result.snapshot, '- Second', '- Third');
    console.log('list item blanks:', blanks1, blanks2);
    expect(blanks1).toBe(0);
    expect(blanks2).toBe(0);
  }, 30000);

  it('nested → top-level list item: exactly 1 blank line on de-indent', async () => {
    const result = await renderMarkdown('spacing-list-deindent',
      '1. First section\n   - sub-item A\n   - sub-item B\n2. Second section');
    testCase = result.testCase;
    // Top-level → nested: no spacing (sub-items belong to parent)
    const nestBlanks = blankLinesBetween(result.snapshot, 'First section', 'sub-item A');
    console.log('nest-indent blanks:', nestBlanks);
    expect(nestBlanks).toBe(0);
    // Sub-items should be tight (0 blank lines)
    const subBlanks = blankLinesBetween(result.snapshot, 'sub-item A', 'sub-item B');
    console.log('sub-item blanks:', subBlanks);
    expect(subBlanks).toBe(0);
    // De-indent from sub-item to top-level should have 1 blank line
    const deindentBlanks = blankLinesBetween(result.snapshot, 'sub-item B', '2.');
    console.log('de-indent blanks:', deindentBlanks);
    expect(deindentBlanks).toBe(1);
  }, 30000);

  // ── No blank line within consecutive blockquote lines ──

  it('consecutive blockquotes: 0 blank lines between them', async () => {
    const result = await renderMarkdown('spacing-blockquotes',
      '> Line one\n> Line two\n> Line three');
    testCase = result.testCase;
    const snapshot = result.snapshot;
    const lines = snapshot.filter(l => l.includes('Line'));
    console.log('blockquote lines found:', lines.length);
    // All three should be adjacent (no blank lines between)
    const idx1 = snapshot.findIndex(l => l.includes('Line one'));
    const idx2 = snapshot.findIndex(l => l.includes('Line two'));
    const idx3 = snapshot.findIndex(l => l.includes('Line three'));
    expect(idx2 - idx1).toBe(1);
    expect(idx3 - idx2).toBe(1);
  }, 30000);
});

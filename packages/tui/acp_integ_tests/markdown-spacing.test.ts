/**
 * Markdown rendering tests - inter-block spacing (paragraph -> list,
 * code -> header, etc.). Asserts exact blank-line counts between
 * adjacent block types.
 *
 * See `markdown-nested.test.ts` for the shared-TUI pattern.
 */
import { afterAll, beforeAll, describe, expect, it } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
  PromptRequest,
  PromptResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';

const SESSION_ID = 'markdown-spacing-session';

let tc: AcpTestCase;

beforeAll(async () => {
  tc = new AcpTestCase({
    testName: 'markdown-spacing',
    terminalSize: { width: 100, height: 50 },
  });

  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: SESSION_ID,
    modes: {
      currentModeId: 'vibe',
      availableModes: [{ id: 'vibe', name: 'Vibe' }],
    },
  }));
  tc.mock.on('session/set_config_option', () => ({}));

  await tc.launch();
  await tc.mock.awaitConnection();
  await tc.waitForVisibleText('ask a question', 10000);
});

afterAll(async () => {
  if (tc) await tc.cleanup();
});

/**
 * Count blank lines between two content lines in a terminal snapshot.
 * Returns the number of empty/whitespace-only lines between the line
 * containing `textA` and the line containing `textB`.
 *
 * Uses `lastIndexOf` rather than `findIndex` because the suite shares a
 * single TUI across all tests. `clearTerminal()` resets the xterm buffer
 * but ink may re-emit prior static items on its next render. The latest
 * test's content is always at the bottom of the buffer, so the last
 * occurrence is the correct one to assert against.
 */
function blankLinesBetween(
  snapshot: string[],
  textA: string,
  textB: string
): number {
  const idxA = snapshot.findLastIndex((l) => l.includes(textA));
  const idxB = snapshot.findLastIndex((l) => l.includes(textB));
  if (idxA === -1) throw new Error(`"${textA}" not found in snapshot`);
  if (idxB === -1) throw new Error(`"${textB}" not found in snapshot`);
  const [start, end] = idxA < idxB ? [idxA, idxB] : [idxB, idxA];
  let blanks = 0;
  for (let i = start + 1; i < end; i++) {
    if (snapshot[i]!.trim() === '') blanks++;
  }
  return blanks;
}

/**
 * Push `content` as a single agent_message_chunk and return the
 * post-render snapshot. Waits on rendered text plus a settle delay,
 * which is sufficient for spacing assertions (spacing is finalised
 * once the chunk is fully rendered).
 */
async function renderMarkdown(
  content: string,
  waitText: string
): Promise<string[]> {
  tc.clearTerminal();

  tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
    tc.mock.notify('session/update', {
      sessionId: SESSION_ID,
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: content },
      },
    });
    return { stopReason: 'end_turn' };
  });

  await tc.sendKeys('test');
  await tc.sleepMs(100);
  await tc.pressEnter();
  await tc.waitForVisibleText(waitText, 10000);
  await tc.sleepMs(500);

  return tc.getSnapshot();
}

describe('Markdown Spacing', () => {
  // ── Double newline bugs (expect exactly 1 blank line) ──

  it('code → code: exactly 1 blank line between consecutive code blocks', async () => {
    const snapshot = await renderMarkdown(
      '```js\nconst a = 1;\n```\n\n```python\nx = 2\n```',
      'x = 2'
    );
    const blanks = blankLinesBetween(snapshot, 'const a', 'x = 2');
    console.log('code→code blanks:', blanks);
    expect(blanks).toBe(1);
  });

  it('code → header: exactly 1 blank line', async () => {
    const snapshot = await renderMarkdown(
      '```js\nconst a = 1;\n```\n\n## Next Section',
      'Next Section'
    );
    const blanks = blankLinesBetween(snapshot, 'const a', 'Next Section');
    console.log('code→header blanks:', blanks);
    expect(blanks).toBe(1);
  });

  it('header → code: exactly 1 blank line', async () => {
    const snapshot = await renderMarkdown(
      '## Title\n\n```js\nconst a = 1;\n```',
      'const a'
    );
    const blanks = blankLinesBetween(snapshot, 'Title', 'const a');
    console.log('header→code blanks:', blanks);
    expect(blanks).toBe(1);
  });

  // ── Missing newline bugs (expect exactly 1 blank line) ──

  it('text → list: exactly 1 blank line', async () => {
    const snapshot = await renderMarkdown(
      'Here are the items:\n\n- First\n- Second',
      'Second'
    );
    const blanks = blankLinesBetween(snapshot, 'Here are the items', '- First');
    console.log('text→list blanks:', blanks);
    expect(blanks).toBe(1);
  });

  it('list → text: exactly 1 blank line', async () => {
    const snapshot = await renderMarkdown(
      '- First\n- Second\n\nSome follow-up text.',
      'follow-up text'
    );
    const blanks = blankLinesBetween(snapshot, '- Second', 'follow-up text');
    console.log('list→text blanks:', blanks);
    expect(blanks).toBe(1);
  });

  it('text → blockquote: exactly 1 blank line', async () => {
    const snapshot = await renderMarkdown(
      'Important:\n\n> This is a quote',
      'This is a quote'
    );
    const blanks = blankLinesBetween(snapshot, 'Important', 'This is a quote');
    console.log('text→blockquote blanks:', blanks);
    expect(blanks).toBe(1);
  });

  it('blockquote → text: exactly 1 blank line', async () => {
    const snapshot = await renderMarkdown(
      '> A quote\n\nNormal text after.',
      'Normal text after'
    );
    const blanks = blankLinesBetween(snapshot, 'A quote', 'Normal text after');
    console.log('blockquote→text blanks:', blanks);
    expect(blanks).toBe(1);
  });

  it('text → table: exactly 1 blank line', async () => {
    const snapshot = await renderMarkdown(
      'Results:\n\n| A | B |\n|---|---|\n| 1 | 2 |',
      'A'
    );
    // Find Results, then the next non-blank line (the table top border).
    const textIdx = snapshot.findLastIndex((l) => l.includes('Results'));
    expect(textIdx).toBeGreaterThan(-1);
    let tableIdx = textIdx + 1;
    while (tableIdx < snapshot.length && snapshot[tableIdx]!.trim() === '') {
      tableIdx++;
    }
    expect(tableIdx).toBeLessThan(snapshot.length);
    const blanks = tableIdx - textIdx - 1;
    console.log('text→table blanks:', blanks);
    expect(blanks).toBe(1);
  });

  it('table → text: exactly 1 blank line', async () => {
    const snapshot = await renderMarkdown(
      '| A | B |\n|---|---|\n| 1 | 2 |\n\nAfter the table.',
      'After the table'
    );
    // Find last table row (contains └) or data row
    const tableEndIdx = snapshot.findLastIndex((l) => l.includes('└'));
    const textIdx = snapshot.findLastIndex((l) =>
      l.includes('After the table')
    );
    expect(tableEndIdx).toBeGreaterThan(-1);
    expect(textIdx).toBeGreaterThan(-1);
    let blanks = 0;
    for (let i = tableEndIdx + 1; i < textIdx; i++) {
      if (snapshot[i]!.trim() === '') blanks++;
    }
    console.log('table→text blanks:', blanks);
    expect(blanks).toBe(1);
  });

  it('header → list: exactly 1 blank line', async () => {
    const snapshot = await renderMarkdown(
      '## Features\n\n- Alpha\n- Beta',
      'Beta'
    );
    const blanks = blankLinesBetween(snapshot, 'Features', '- Alpha');
    console.log('header→list blanks:', blanks);
    expect(blanks).toBe(1);
  });

  it('list → header: exactly 1 blank line', async () => {
    const snapshot = await renderMarkdown(
      '- Alpha\n- Beta\n\n## Next Section',
      'Next Section'
    );
    const blanks = blankLinesBetween(snapshot, '- Beta', 'Next Section');
    console.log('list→header blanks:', blanks);
    expect(blanks).toBe(1);
  });

  it('hr → text: exactly 1 blank line', async () => {
    const snapshot = await renderMarkdown(
      'Before the rule.\n\n---\n\nAfter the rule.',
      'After the rule'
    );
    const beforeIdx = snapshot.findLastIndex((l) =>
      l.includes('Before the rule')
    );
    const afterIdx = snapshot.findLastIndex((l) =>
      l.includes('After the rule')
    );
    expect(beforeIdx).toBeGreaterThan(-1);
    expect(afterIdx).toBeGreaterThan(-1);
    // There should be exactly 3 lines between: blank, hr, blank
    // (before → blank → hr → blank → after)
    const gap = afterIdx - beforeIdx;
    console.log('hr→text gap (before to after):', gap);
    expect(gap).toBe(4); // before, blank, hr, blank, after
  });

  // ── Consistency: all block transitions should have uniform spacing ──

  it('LLM-style response: uniform 1 blank line between all sections', async () => {
    const snapshot = await renderMarkdown(
      'Here is the solution:\n\n## Step 1\n\n```python\ndef solve():\n    pass\n```\n\n## Step 2\n\n- Do this\n- Do that\n\n> Note: be careful\n\n| Col | Val |\n|-----|-----|\n| x   | 1   |\n\nDone.',
      'Done'
    );

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
        expect(snapshot.some((l) => l.includes(a))).toBe(true);
        continue;
      }
      const blanks = blankLinesBetween(snapshot, a, b);
      console.log(`${a} → ${b}: ${blanks} blank lines`);
      expect(blanks).toBe(1);
    }
  });

  // ── No blank line within consecutive list items ──

  it('consecutive list items: 0 blank lines between them', async () => {
    const snapshot = await renderMarkdown(
      '- First\n- Second\n- Third',
      'Third'
    );
    const blanks1 = blankLinesBetween(snapshot, '- First', '- Second');
    const blanks2 = blankLinesBetween(snapshot, '- Second', '- Third');
    console.log('list item blanks:', blanks1, blanks2);
    expect(blanks1).toBe(0);
    expect(blanks2).toBe(0);
  });

  it('nested → top-level list item: exactly 1 blank line on de-indent', async () => {
    const snapshot = await renderMarkdown(
      '1. First section\n   - sub-item A\n   - sub-item B\n2. Second section',
      'Second section'
    );
    // Top-level → nested: no spacing (sub-items belong to parent)
    const nestBlanks = blankLinesBetween(
      snapshot,
      'First section',
      'sub-item A'
    );
    console.log('nest-indent blanks:', nestBlanks);
    expect(nestBlanks).toBe(0);
    // Sub-items should be tight (0 blank lines)
    const subBlanks = blankLinesBetween(snapshot, 'sub-item A', 'sub-item B');
    console.log('sub-item blanks:', subBlanks);
    expect(subBlanks).toBe(0);
    // De-indent from sub-item to top-level should have 1 blank line
    const deindentBlanks = blankLinesBetween(snapshot, 'sub-item B', '2.');
    console.log('de-indent blanks:', deindentBlanks);
    expect(deindentBlanks).toBe(1);
  });

  it('indented code block under list item: exactly 1 blank line before and after', async () => {
    const snapshot = await renderMarkdown(
      "1. **Max iterations** — you pass a cap when you run it:\n   ```bash\n   ./ralph.sh 5\n   ```\n   If you don't pass one, check the default.",
      'check the default'
    );
    const blanksBefore = blankLinesBetween(
      snapshot,
      'you pass a cap',
      './ralph.sh 5'
    );
    const blanksAfter = blankLinesBetween(
      snapshot,
      './ralph.sh 5',
      'check the default'
    );
    console.log('list→indented-code blanks:', blanksBefore);
    console.log('indented-code→text blanks:', blanksAfter);
    expect(blanksBefore).toBe(1);
    expect(blanksAfter).toBe(1);
  });

  // ── No blank line within consecutive blockquote lines ──

  it('consecutive blockquotes: 0 blank lines between them', async () => {
    const snapshot = await renderMarkdown(
      '> Line one\n> Line two\n> Line three',
      'Line three'
    );
    const lines = snapshot.filter((l) => l.includes('Line'));
    console.log('blockquote lines found:', lines.length);
    // All three should be adjacent (no blank lines between)
    const idx1 = snapshot.findLastIndex((l) => l.includes('Line one'));
    const idx2 = snapshot.findLastIndex((l) => l.includes('Line two'));
    const idx3 = snapshot.findLastIndex((l) => l.includes('Line three'));
    expect(idx2 - idx1).toBe(1);
    expect(idx3 - idx2).toBe(1);
  });
});

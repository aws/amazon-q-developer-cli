import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import type { CellAttributes } from '../src/test-utils/shared/pty-manager';

/** Launch test, send markdown, wait for idle, return testCase. */
async function render(testName: string, md: string, waitText: string): Promise<E2ETestCase> {
  const tc = await E2ETestCase.builder()
    .withTestName(testName)
    .withTerminal({ width: 100, height: 40 })
    .launch();
  await tc.waitForText('ask a question', 10000);
  await tc.getSessionId();
  await tc.pushSendMessageResponse([
    { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: md } } },
  ]);
  await tc.pushSendMessageResponse(null);
  await tc.sendKeys('test');
  await tc.sleepMs(100);
  await tc.pressEnter();
  await tc.waitForText(waitText, 10000);
  await tc.waitForIdle(15000);
  await tc.sleepMs(300);
  return tc;
}

function expectAllBold(cells: CellAttributes[]) {
  for (const c of cells) {
    expect(c.bold).toBe(true);
  }
}

function expectAllItalic(cells: CellAttributes[]) {
  for (const c of cells) {
    expect(c.italic).toBe(true);
  }
}

function expectAllStrikethrough(cells: CellAttributes[]) {
  for (const c of cells) {
    expect(c.strikethrough).toBe(true);
  }
}

function expectNone(cells: CellAttributes[], attr: keyof CellAttributes) {
  for (const c of cells) {
    expect(c[attr]).toBe(false);
  }
}

function expectHasFgColor(cells: CellAttributes[]) {
  for (const c of cells) {
    expect(c.fgColor).not.toBeNull();
  }
}

describe('Nested Markdown Rendering', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  // ── Inline nesting ──

  describe('inline nesting', () => {
    it('bold + italic: ***text***', async () => {
      testCase = await render('nested-bold-italic', 'Hello ***bold italic*** world', 'bold italic');
      const cells = testCase.findTextCells('bold italic');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      expectAllItalic(cells!);
      // surrounding text should be plain
      const hello = testCase.findTextCells('Hello');
      expect(hello).not.toBeNull();
      expectNone(hello!, 'bold');
      expectNone(hello!, 'italic');
    }, 30000);

    it('bold + code: **`code`**', async () => {
      testCase = await render('nested-bold-code', 'Use **`getValue`** here', 'getValue');
      const cells = testCase.findTextCells('getValue');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      expectHasFgColor(cells!);
    }, 30000);

    it('italic + code: *`code`*', async () => {
      testCase = await render('nested-italic-code', 'See *`config`* file', 'config');
      const cells = testCase.findTextCells('config');
      expect(cells).not.toBeNull();
      expectAllItalic(cells!);
      expectHasFgColor(cells!);
    }, 30000);

    it('bold + italic + strikethrough: ~~***text***~~', async () => {
      testCase = await render('nested-all-three', 'This is ~~***all three***~~ done', 'all three');
      const cells = testCase.findTextCells('all three');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      expectAllItalic(cells!);
      expectAllStrikethrough(cells!);
    }, 30000);

    it('bold inside strikethrough: ~~**deleted bold**~~', async () => {
      testCase = await render('nested-strike-bold', 'Was ~~**deleted bold**~~ removed', 'deleted bold');
      const cells = testCase.findTextCells('deleted bold');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      expectAllStrikethrough(cells!);
    }, 30000);

    it('italic inside strikethrough: ~~*deleted italic*~~', async () => {
      testCase = await render('nested-strike-italic', 'Was ~~*deleted italic*~~ removed', 'deleted italic');
      const cells = testCase.findTextCells('deleted italic');
      expect(cells).not.toBeNull();
      expectAllItalic(cells!);
      expectAllStrikethrough(cells!);
    }, 30000);

    it('link with bold text: [**bold link**](url)', async () => {
      testCase = await render('nested-bold-link', 'Click [**bold link**](https://example.com) now', 'bold link');
      const cells = testCase.findTextCells('bold link');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      // link text should be on same line as surrounding text
      const snapshot = testCase.getSnapshot();
      expect(snapshot.some(l => l.includes('bold link') && l.includes('now'))).toBe(true);
    }, 30000);

    it('link with italic text: [*italic link*](url)', async () => {
      testCase = await render('nested-italic-link', 'See [*italic link*](https://example.com) here', 'italic link');
      const cells = testCase.findTextCells('italic link');
      expect(cells).not.toBeNull();
      expectAllItalic(cells!);
    }, 30000);

    it('bold italic link: ***[text](url)***', async () => {
      testCase = await render('nested-bold-italic-link', 'See ***[bold italic link](https://example.com)*** here', 'bold italic link');
      const cells = testCase.findTextCells('bold italic link');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      expectAllItalic(cells!);
      const snapshot = testCase.getSnapshot();
      expect(snapshot.some(l => l.includes('bold italic link') && l.includes('here'))).toBe(true);
    }, 30000);

    it('link with code text: [`code link`](url)', async () => {
      testCase = await render('nested-code-link', 'Use [`useState`](https://react.dev) hook', 'useState');
      const snapshot = testCase.getSnapshot();
      // code link text and surrounding text on same line
      expect(snapshot.some(l => l.includes('useState') && l.includes('hook'))).toBe(true);
    }, 30000);
  });

  // ── Header nesting ──

  describe('header nesting', () => {
    it('code in header: ## The `main` function', async () => {
      testCase = await render('nested-header-code', '## The `main` function\n\nSome text', 'main');
      const snapshot = testCase.getSnapshot();
      // header text should be on its own line
      const headerLine = snapshot.find(l => l.includes('main') && l.includes('function'));
      expect(headerLine).toBeDefined();
      expect(headerLine!.includes('Some text')).toBe(false);
      // header should be bold
      const cells = testCase.findTextCells('The');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      // inline code in header should have highlight color
      const codeCells = testCase.findTextCells('main');
      expect(codeCells).not.toBeNull();
      expectHasFgColor(codeCells!);
    }, 30000);

    it('italic in header: ## *Emphasis* here', async () => {
      testCase = await render('nested-header-italic', '## *Emphasis* here\n\nContent', 'Emphasis');
      const cells = testCase.findTextCells('Emphasis');
      expect(cells).not.toBeNull();
      // header text is bold, and this word is also italic
      expectAllBold(cells!);
      expectAllItalic(cells!);
    }, 30000);

    it('link in header: ## [Section](url)', async () => {
      testCase = await render('nested-header-link', '## [API Reference](https://docs.example.com)\n\nDocs here', 'API Reference');
      const snapshot = testCase.getSnapshot();
      const headerLine = snapshot.find(l => l.includes('API Reference'));
      expect(headerLine).toBeDefined();
      expect(headerLine!.includes('Docs here')).toBe(false);
    }, 30000);
  });

  // ── Table nesting ──

  describe('table nesting', () => {
    it('bold in table cell', async () => {
      testCase = await render('nested-table-bold',
        '| Name | Status |\n|------|--------|\n| Alice | **active** |\n| Bob | inactive |',
        'Alice');
      const cells = testCase.findTextCells('active');
      expect(cells).not.toBeNull();
      // "active" in Alice's row should be bold
      expectAllBold(cells!);
    }, 30000);

    it('italic in table cell', async () => {
      testCase = await render('nested-table-italic',
        '| Key | Value |\n|-----|-------|\n| name | *unknown* |',
        'unknown');
      const cells = testCase.findTextCells('unknown');
      expect(cells).not.toBeNull();
      expectAllItalic(cells!);
    }, 30000);

    it('code in table cell', async () => {
      testCase = await render('nested-table-code',
        '| Command | Description |\n|---------|-------------|\n| `ls` | List files |\n| `cd` | Change dir |',
        'ls');
      const snapshot = testCase.getSnapshot();
      // code and description should be in the same row
      expect(snapshot.some(l => l.includes('ls') && l.includes('List files'))).toBe(true);
      expect(snapshot.some(l => l.includes('cd') && l.includes('Change dir'))).toBe(true);
      // inline code should have highlight color
      const cells = testCase.findTextCells('ls');
      expect(cells).not.toBeNull();
      expectHasFgColor(cells!);
    }, 30000);

    it('link in table cell', async () => {
      testCase = await render('nested-table-link',
        '| Resource | Link |\n|----------|------|\n| Docs | [here](https://example.com) |',
        'here');
      const snapshot = testCase.getSnapshot();
      expect(snapshot.some(l => l.includes('Docs') && l.includes('here'))).toBe(true);
    }, 30000);

    it('bold + italic in table cell', async () => {
      testCase = await render('nested-table-bold-italic',
        '| Item | Note |\n|------|------|\n| Test | ***critical*** |',
        'critical');
      const cells = testCase.findTextCells('critical');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      expectAllItalic(cells!);
    }, 30000);

    it('bold in wrapped table cell', async () => {
      testCase = await render('nested-table-wrap',
        '| Feature | Description |\n|---|---|\n| Startup | Companies would form the **world\'s 10th largest economy** |',
        "world's 10th");
      const cells = testCase.findTextCells("world's");
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
    }, 30000);
  });

  // ── List nesting ──

  describe('list nesting', () => {
    it('bold inside list item', async () => {
      testCase = await render('nested-list-bold',
        '- **Important** item\n- Normal item',
        'Important');
      const cells = testCase.findTextCells('Important');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      // "item" after bold should not be bold
      const rest = testCase.findTextCells('item');
      expect(rest).not.toBeNull();
      expectNone(rest!, 'bold');
    }, 30000);

    it('italic inside list item', async () => {
      testCase = await render('nested-list-italic',
        '- *Emphasis* here\n- Plain text',
        'Emphasis');
      const cells = testCase.findTextCells('Emphasis');
      expect(cells).not.toBeNull();
      expectAllItalic(cells!);
    }, 30000);

    it('code inside list item', async () => {
      testCase = await render('nested-list-code',
        '- Run `npm install` first\n- Then `npm start`',
        'npm install');
      const snapshot = testCase.getSnapshot();
      expect(snapshot.some(l => l.includes('npm install') && l.includes('first'))).toBe(true);
      expect(snapshot.some(l => l.includes('npm start'))).toBe(true);
      // inline code should have highlight color
      const cells = testCase.findTextCells('npm install');
      expect(cells).not.toBeNull();
      expectHasFgColor(cells!);
    }, 30000);

    it('link inside list item', async () => {
      testCase = await render('nested-list-link',
        '- See [docs](https://example.com) for details\n- Also [FAQ](https://faq.example.com)',
        'docs');
      const snapshot = testCase.getSnapshot();
      expect(snapshot.some(l => l.includes('docs') && l.includes('details'))).toBe(true);
    }, 30000);

    it('bold + italic inside list item', async () => {
      testCase = await render('nested-list-bold-italic',
        '- This is ***very important***\n- This is normal',
        'very important');
      const cells = testCase.findTextCells('very important');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      expectAllItalic(cells!);
    }, 30000);

    it('strikethrough inside list item', async () => {
      testCase = await render('nested-list-strike',
        '- ~~Deprecated~~ feature\n- New feature',
        'Deprecated');
      const cells = testCase.findTextCells('Deprecated');
      expect(cells).not.toBeNull();
      expectAllStrikethrough(cells!);
    }, 30000);

    it('nested list: ordered inside unordered', async () => {
      testCase = await render('nested-ol-in-ul',
        '- Fruits\n  1. Apple\n  2. Banana\n- Vegetables\n  1. Carrot',
        'Apple');
      const snapshot = testCase.getSnapshot();
      expect(snapshot.some(l => l.includes('Fruits'))).toBe(true);
      expect(snapshot.some(l => l.includes('1.') && l.includes('Apple'))).toBe(true);
      expect(snapshot.some(l => l.includes('2.') && l.includes('Banana'))).toBe(true);
      expect(snapshot.some(l => l.includes('Vegetables'))).toBe(true);
      expect(snapshot.some(l => l.includes('1.') && l.includes('Carrot'))).toBe(true);
      // nested items should be indented more than parent
      const fruitsIdx = snapshot.findIndex(l => l.includes('Fruits'));
      const appleIdx = snapshot.findIndex(l => l.includes('Apple'));
      const fruitsIndent = snapshot[fruitsIdx]!.search(/\S/);
      const appleIndent = snapshot[appleIdx]!.search(/\S/);
      expect(appleIndent).toBeGreaterThan(fruitsIndent);
    }, 30000);

    it('nested list: unordered inside ordered', async () => {
      testCase = await render('nested-ul-in-ol',
        '1. Step one\n   - Detail A\n   - Detail B\n2. Step two',
        'Step one');
      const snapshot = testCase.getSnapshot();
      expect(snapshot.some(l => l.includes('Step one'))).toBe(true);
      expect(snapshot.some(l => l.includes('Detail A'))).toBe(true);
      expect(snapshot.some(l => l.includes('Detail B'))).toBe(true);
      expect(snapshot.some(l => l.includes('Step two'))).toBe(true);
    }, 30000);

    it('bold inside nested list item', async () => {
      testCase = await render('nested-child-bold',
        '- Parent\n  - **Bold child**\n  - Normal child',
        'Bold child');
      const cells = testCase.findTextCells('Bold child');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
    }, 30000);

    it('code block inside list item', async () => {
      testCase = await render('nested-li-codeblock',
        '- Install:\n  ```bash\n  npm install\n  ```\n- Run it',
        'npm install');
      const snapshot = testCase.getSnapshot();
      expect(snapshot.some(l => l.includes('npm install'))).toBe(true);
      expect(snapshot.some(l => l.includes('Run it'))).toBe(true);
    }, 30000);

    it('multiple inline styles in one list item', async () => {
      testCase = await render('nested-list-multi-style',
        '- **Bold** and *italic* and `code` together\n- Plain item',
        'Bold');
      const snapshot = testCase.getSnapshot();
      // all on same line
      expect(snapshot.some(l =>
        l.includes('Bold') && l.includes('italic') && l.includes('code') && l.includes('together')
      )).toBe(true);
      const boldCells = testCase.findTextCells('Bold');
      expect(boldCells).not.toBeNull();
      expectAllBold(boldCells!);
      const italicCells = testCase.findTextCells('italic');
      expect(italicCells).not.toBeNull();
      expectAllItalic(italicCells!);
    }, 30000);
  });

  // ── Combined: realistic LLM response with nested elements ──

  describe('realistic combined scenarios', () => {
    it('LLM response with nested formatting throughout', async () => {
      const md = [
        '## The `useState` Hook',
        '',
        'React\'s ***most important*** hook for state management.',
        '',
        '| Hook | Purpose |',
        '|------|---------|',
        '| `useState` | **Local state** |',
        '| `useEffect` | *Side effects* |',
        '',
        '- Use [`useState`](https://react.dev) for **simple** state',
        '- Use ~~`componentDidMount`~~ *lifecycle methods* are deprecated',
        '',
        'See [**official docs**](https://react.dev) for more.',
      ].join('\n');

      testCase = await render('nested-combined-llm', md, 'useState');
      const snapshot = testCase.getSnapshot();

      // Header with code renders
      expect(snapshot.some(l => l.includes('useState') && l.includes('Hook'))).toBe(true);

      // Bold+italic in paragraph
      const mostImportant = testCase.findTextCells('most important');
      expect(mostImportant).not.toBeNull();
      expectAllBold(mostImportant!);
      expectAllItalic(mostImportant!);

      // Bold in table cell
      const localState = testCase.findTextCells('Local state');
      expect(localState).not.toBeNull();
      expectAllBold(localState!);

      // Italic in table cell
      const sideEffects = testCase.findTextCells('Side effects');
      expect(sideEffects).not.toBeNull();
      expectAllItalic(sideEffects!);

      // Bold inside list item
      const simple = testCase.findTextCells('simple');
      expect(simple).not.toBeNull();
      expectAllBold(simple!);

      // Bold link text
      const officialDocs = testCase.findTextCells('official docs');
      expect(officialDocs).not.toBeNull();
      expectAllBold(officialDocs!);
    }, 30000);
  });
});

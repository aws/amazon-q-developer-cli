/**
 * Markdown rendering tests - nested constructs (header+code, bold inside list,
 * code inside table cells, etc.).
 *
 * One TUI is shared across all tests in the file. `beforeAll` spawns it and
 * wires the constant ACP handshake handlers. Each test calls
 * `render(md, waitText)` which:
 *   1. Resets the xterm buffer so prior tests' content can't shadow
 *      `findTextCells()` lookups.
 *   2. Re-registers the `session/prompt` handler to push the markdown blob
 *      via a `session/update` notification, then returns `end_turn`.
 *   3. Drives the prompt bar (`test\n`) and waits for the rendered text.
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
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';
import type { CellAttributes } from '../src/test-utils/shared/pty-manager';

const SESSION_ID = 'markdown-nested-session';

let tc: AcpTestCase;

beforeAll(async () => {
  tc = new AcpTestCase({
    testName: 'markdown-nested',
    terminalSize: { width: 100, height: 40 },
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
    modes: defaultKasModes(),
  }));
  tc.mock.on('session/set_config_option', () => ({}));

  await tc.launch();
  await tc.mock.awaitConnection();
  await tc.waitForVisibleText('ask a question', 10000);
});

afterAll(async () => {
  if (tc) await tc.cleanup();
});

/** Push `md` as a single agent_message_chunk and wait for it to render. */
async function render(md: string, waitText: string): Promise<void> {
  tc.clearTerminal();

  // Re-register per render so we know the handler fires fresh content.
  // `AcpMockServer.on()` overwrites prior handlers for the same method.
  tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
    tc.mock.notify('session/update', {
      sessionId: SESSION_ID,
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: md },
      },
    });
    return { stopReason: 'end_turn' };
  });

  await tc.sendKeys('test');
  await tc.sleepMs(100);
  await tc.pressEnter();
  await tc.waitForVisibleText(waitText, 10000);
  await tc.sleepMs(300);
}

function expectAllBold(cells: CellAttributes[]) {
  for (const c of cells) expect(c.bold).toBe(true);
}

function expectAllItalic(cells: CellAttributes[]) {
  for (const c of cells) expect(c.italic).toBe(true);
}

function expectAllStrikethrough(cells: CellAttributes[]) {
  for (const c of cells) expect(c.strikethrough).toBe(true);
}

function expectNone(cells: CellAttributes[], attr: keyof CellAttributes) {
  for (const c of cells) expect(c[attr]).toBe(false);
}

function expectHasFgColor(cells: CellAttributes[]) {
  for (const c of cells) expect(c.fgColor).not.toBeNull();
}

describe('Nested Markdown Rendering', () => {
  // ── Inline nesting ──

  describe('inline nesting', () => {
    it('bold + italic: ***text***', async () => {
      await render('Hello ***bold italic*** world', 'bold italic');
      const cells = tc.findTextCells('bold italic');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      expectAllItalic(cells!);
      // surrounding text should be plain
      const hello = tc.findTextCells('Hello');
      expect(hello).not.toBeNull();
      expectNone(hello!, 'bold');
      expectNone(hello!, 'italic');
    });

    it('bold + code: **`code`**', async () => {
      await render('Use **`getValue`** here', 'getValue');
      const cells = tc.findTextCells('getValue');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      expectHasFgColor(cells!);
    });

    it('italic + code: *`code`*', async () => {
      await render('See *`config`* file', 'config');
      const cells = tc.findTextCells('config');
      expect(cells).not.toBeNull();
      expectAllItalic(cells!);
      expectHasFgColor(cells!);
    });

    it('bold + italic + strikethrough: ~~***text***~~', async () => {
      await render('This is ~~***all three***~~ done', 'all three');
      const cells = tc.findTextCells('all three');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      expectAllItalic(cells!);
      expectAllStrikethrough(cells!);
    });

    it('bold inside strikethrough: ~~**deleted bold**~~', async () => {
      await render('Was ~~**deleted bold**~~ removed', 'deleted bold');
      const cells = tc.findTextCells('deleted bold');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      expectAllStrikethrough(cells!);
    });

    it('italic inside strikethrough: ~~*deleted italic*~~', async () => {
      await render('Was ~~*deleted italic*~~ removed', 'deleted italic');
      const cells = tc.findTextCells('deleted italic');
      expect(cells).not.toBeNull();
      expectAllItalic(cells!);
      expectAllStrikethrough(cells!);
    });

    it('link with bold text: [**bold link**](url)', async () => {
      await render(
        'Click [**bold link**](https://example.com) now',
        'bold link'
      );
      const cells = tc.findTextCells('bold link');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      // link text should be on same line as surrounding text
      const snapshot = tc.getSnapshot();
      expect(
        snapshot.some((l) => l.includes('bold link') && l.includes('now'))
      ).toBe(true);
    });

    it('link with italic text: [*italic link*](url)', async () => {
      await render(
        'See [*italic link*](https://example.com) here',
        'italic link'
      );
      const cells = tc.findTextCells('italic link');
      expect(cells).not.toBeNull();
      expectAllItalic(cells!);
    });

    it('bold italic link: ***[text](url)***', async () => {
      await render(
        'See ***[bold italic link](https://example.com)*** here',
        'bold italic link'
      );
      const cells = tc.findTextCells('bold italic link');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      expectAllItalic(cells!);
      const snapshot = tc.getSnapshot();
      expect(
        snapshot.some(
          (l) => l.includes('bold italic link') && l.includes('here')
        )
      ).toBe(true);
    });

    it('link with code text: [`code link`](url)', async () => {
      await render('Use [`useState`](https://react.dev) hook', 'useState');
      const snapshot = tc.getSnapshot();
      // code link text and surrounding text on same line
      expect(
        snapshot.some((l) => l.includes('useState') && l.includes('hook'))
      ).toBe(true);
    });
  });

  // ── Header nesting ──

  describe('header nesting', () => {
    it('code in header: ## The `main` function', async () => {
      await render('## The `main` function\n\nSome text', 'main');
      const snapshot = tc.getSnapshot();
      // header text should be on its own line
      const headerLine = snapshot.find(
        (l) => l.includes('main') && l.includes('function')
      );
      expect(headerLine).toBeDefined();
      expect(headerLine!.includes('Some text')).toBe(false);
      // header should be bold
      const cells = tc.findTextCells('The');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      // inline code in header should have highlight color
      const codeCells = tc.findTextCells('main');
      expect(codeCells).not.toBeNull();
      expectHasFgColor(codeCells!);
    });

    it('italic in header: ## *Emphasis* here', async () => {
      await render('## *Emphasis* here\n\nContent', 'Emphasis');
      const cells = tc.findTextCells('Emphasis');
      expect(cells).not.toBeNull();
      // header text is bold, and this word is also italic
      expectAllBold(cells!);
      expectAllItalic(cells!);
    });

    it('link in header: ## [Section](url)', async () => {
      await render(
        '## [API Reference](https://docs.example.com)\n\nDocs here',
        'API Reference'
      );
      const snapshot = tc.getSnapshot();
      const headerLine = snapshot.find((l) => l.includes('API Reference'));
      expect(headerLine).toBeDefined();
      expect(headerLine!.includes('Docs here')).toBe(false);
    });
  });

  // ── Table nesting ──

  describe('table nesting', () => {
    it('bold in table cell', async () => {
      await render(
        '| Name | Status |\n|------|--------|\n| Alice | **active** |\n| Bob | inactive |',
        'Alice'
      );
      const cells = tc.findTextCells('active');
      expect(cells).not.toBeNull();
      // "active" in Alice's row should be bold
      expectAllBold(cells!);
    });

    it('italic in table cell', async () => {
      await render(
        '| Key | Value |\n|-----|-------|\n| name | *unknown* |',
        'unknown'
      );
      const cells = tc.findTextCells('unknown');
      expect(cells).not.toBeNull();
      expectAllItalic(cells!);
    });

    it('code in table cell', async () => {
      await render(
        '| Command | Description |\n|---------|-------------|\n| `ls` | List files |\n| `cd` | Change dir |',
        'ls'
      );
      const snapshot = tc.getSnapshot();
      // code and description should be in the same row
      expect(
        snapshot.some((l) => l.includes('ls') && l.includes('List files'))
      ).toBe(true);
      expect(
        snapshot.some((l) => l.includes('cd') && l.includes('Change dir'))
      ).toBe(true);
      // inline code should have highlight color
      const cells = tc.findTextCells('ls');
      expect(cells).not.toBeNull();
      expectHasFgColor(cells!);
    });

    it('link in table cell', async () => {
      await render(
        '| Resource | Link |\n|----------|------|\n| Docs | [here](https://example.com) |',
        'here'
      );
      const snapshot = tc.getSnapshot();
      expect(
        snapshot.some((l) => l.includes('Docs') && l.includes('here'))
      ).toBe(true);
    });

    it('bold + italic in table cell', async () => {
      await render(
        '| Item | Note |\n|------|------|\n| Test | ***critical*** |',
        'critical'
      );
      const cells = tc.findTextCells('critical');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      expectAllItalic(cells!);
    });

    it('bold in wrapped table cell', async () => {
      await render(
        "| Feature | Description |\n|---|---|\n| Startup | Companies would form the **world's 10th largest economy** |",
        "world's 10th"
      );
      const cells = tc.findTextCells("world's");
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
    });
  });

  // ── List nesting ──

  describe('list nesting', () => {
    it('bold inside list item', async () => {
      await render('- **Important** item\n- Normal item', 'Important');
      const cells = tc.findTextCells('Important');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      // "item" after bold should not be bold
      const rest = tc.findTextCells('item');
      expect(rest).not.toBeNull();
      expectNone(rest!, 'bold');
    });

    it('italic inside list item', async () => {
      await render('- *Emphasis* here\n- Plain text', 'Emphasis');
      const cells = tc.findTextCells('Emphasis');
      expect(cells).not.toBeNull();
      expectAllItalic(cells!);
    });

    it('code inside list item', async () => {
      await render(
        '- Run `npm install` first\n- Then `npm start`',
        'npm install'
      );
      const snapshot = tc.getSnapshot();
      expect(
        snapshot.some((l) => l.includes('npm install') && l.includes('first'))
      ).toBe(true);
      expect(snapshot.some((l) => l.includes('npm start'))).toBe(true);
      // inline code should have highlight color
      const cells = tc.findTextCells('npm install');
      expect(cells).not.toBeNull();
      expectHasFgColor(cells!);
    });

    it('link inside list item', async () => {
      await render(
        '- See [docs](https://example.com) for details\n- Also [FAQ](https://faq.example.com)',
        'docs'
      );
      const snapshot = tc.getSnapshot();
      expect(
        snapshot.some((l) => l.includes('docs') && l.includes('details'))
      ).toBe(true);
    });

    it('bold + italic inside list item', async () => {
      await render(
        '- This is ***very important***\n- This is normal',
        'very important'
      );
      const cells = tc.findTextCells('very important');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
      expectAllItalic(cells!);
    });

    it('strikethrough inside list item', async () => {
      await render('- ~~Deprecated~~ feature\n- New feature', 'Deprecated');
      const cells = tc.findTextCells('Deprecated');
      expect(cells).not.toBeNull();
      expectAllStrikethrough(cells!);
    });

    it('nested list: ordered inside unordered', async () => {
      await render(
        '- Fruits\n  1. Apple\n  2. Banana\n- Vegetables\n  1. Carrot',
        'Apple'
      );
      const snapshot = tc.getSnapshot();
      expect(snapshot.some((l) => l.includes('Fruits'))).toBe(true);
      expect(
        snapshot.some((l) => l.includes('1.') && l.includes('Apple'))
      ).toBe(true);
      expect(
        snapshot.some((l) => l.includes('2.') && l.includes('Banana'))
      ).toBe(true);
      expect(snapshot.some((l) => l.includes('Vegetables'))).toBe(true);
      expect(
        snapshot.some((l) => l.includes('1.') && l.includes('Carrot'))
      ).toBe(true);
      // nested items should be indented more than parent
      const fruitsIdx = snapshot.findLastIndex((l) => l.includes('Fruits'));
      const appleIdx = snapshot.findLastIndex((l) => l.includes('Apple'));
      const fruitsIndent = snapshot[fruitsIdx]!.search(/\S/);
      const appleIndent = snapshot[appleIdx]!.search(/\S/);
      expect(appleIndent).toBeGreaterThan(fruitsIndent);
    });

    it('nested list: unordered inside ordered', async () => {
      await render(
        '1. Step one\n   - Detail A\n   - Detail B\n2. Step two',
        'Step one'
      );
      const snapshot = tc.getSnapshot();
      expect(snapshot.some((l) => l.includes('Step one'))).toBe(true);
      expect(snapshot.some((l) => l.includes('Detail A'))).toBe(true);
      expect(snapshot.some((l) => l.includes('Detail B'))).toBe(true);
      expect(snapshot.some((l) => l.includes('Step two'))).toBe(true);
    });

    it('bold inside nested list item', async () => {
      await render(
        '- Parent\n  - **Bold child**\n  - Normal child',
        'Bold child'
      );
      const cells = tc.findTextCells('Bold child');
      expect(cells).not.toBeNull();
      expectAllBold(cells!);
    });

    it('code block inside list item', async () => {
      await render(
        '- Install:\n  ```bash\n  npm install\n  ```\n- Run it',
        'npm install'
      );
      const snapshot = tc.getSnapshot();
      expect(snapshot.some((l) => l.includes('npm install'))).toBe(true);
      expect(snapshot.some((l) => l.includes('Run it'))).toBe(true);
    });

    it('multiple inline styles in one list item', async () => {
      await render(
        '- **Bold** and *italic* and `code` together\n- Plain item',
        'Bold'
      );
      const snapshot = tc.getSnapshot();
      // all on same line
      expect(
        snapshot.some(
          (l) =>
            l.includes('Bold') &&
            l.includes('italic') &&
            l.includes('code') &&
            l.includes('together')
        )
      ).toBe(true);
      const boldCells = tc.findTextCells('Bold');
      expect(boldCells).not.toBeNull();
      expectAllBold(boldCells!);
      const italicCells = tc.findTextCells('italic');
      expect(italicCells).not.toBeNull();
      expectAllItalic(italicCells!);
    });
  });

  // ── Combined: realistic LLM response with nested elements ──

  describe('realistic combined scenarios', () => {
    it('LLM response with nested formatting throughout', async () => {
      const md = [
        '## The `useState` Hook',
        '',
        "React's ***most important*** hook for state management.",
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

      await render(md, 'useState');
      const snapshot = tc.getSnapshot();

      // Header with code renders
      expect(
        snapshot.some((l) => l.includes('useState') && l.includes('Hook'))
      ).toBe(true);

      // Bold+italic in paragraph
      const mostImportant = tc.findTextCells('most important');
      expect(mostImportant).not.toBeNull();
      expectAllBold(mostImportant!);
      expectAllItalic(mostImportant!);

      // Bold in table cell
      const localState = tc.findTextCells('Local state');
      expect(localState).not.toBeNull();
      expectAllBold(localState!);

      // Italic in table cell
      const sideEffects = tc.findTextCells('Side effects');
      expect(sideEffects).not.toBeNull();
      expectAllItalic(sideEffects!);

      // Bold inside list item
      const simple = tc.findTextCells('simple');
      expect(simple).not.toBeNull();
      expectAllBold(simple!);

      // Bold link text
      const officialDocs = tc.findTextCells('official docs');
      expect(officialDocs).not.toBeNull();
      expectAllBold(officialDocs!);
    });
  });
});

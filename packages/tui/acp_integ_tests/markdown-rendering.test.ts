/**
 * Markdown rendering tests - core constructs (bold, lists, paragraphs,
 * blockquotes, tables, links, mixed formatting, full documents).
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
import { defaultKasModes } from './shared/default-agent';

const SESSION_ID = 'markdown-rendering-session';

let tc: AcpTestCase;

beforeAll(async () => {
  tc = new AcpTestCase({
    testName: 'markdown-rendering',
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

/** Push `chunks` as sequential agent_message_chunks and wait for `waitText`. */
async function render(chunks: string[], waitText: string): Promise<void> {
  tc.clearTerminal();

  tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
    for (const chunk of chunks) {
      tc.mock.notify('session/update', {
        sessionId: SESSION_ID,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: chunk },
        },
      });
    }
    return { stopReason: 'end_turn' };
  });

  await tc.sendKeys('test');
  await tc.sleepMs(100);
  await tc.pressEnter();
  await tc.waitForVisibleText(waitText, 10000);
  await tc.sleepMs(300);
}

/** Single-chunk convenience wrapper for the common case. */
function renderOne(md: string, waitText: string): Promise<void> {
  return render([md], waitText);
}

describe('Markdown Rendering', () => {
  it('renders bold text', async () => {
    await renderOne('Hello **bold text** world', 'bold text');
    const snapshot = tc.getSnapshot();
    const boldLine = snapshot.find(
      (line) => line.includes('bold text') && line.includes('world')
    );
    expect(boldLine).toBeDefined();
  });

  it('renders code blocks with language', async () => {
    await renderOne(
      'Here is code:\n```python\nprint("hello")\n```\nDone.',
      'Done'
    );
    const snapshot = tc.getSnapshot();
    expect(snapshot.some((line) => line.includes('print'))).toBe(true);
    expect(snapshot.some((line) => line.includes('Done'))).toBe(true);
    const introIdx = snapshot.findLastIndex((l) => l.includes('Here is code'));
    const codeIdx = snapshot.findLastIndex((l) => l.includes('print'));
    const doneIdx = snapshot.findLastIndex((l) => l.includes('Done'));
    expect(codeIdx - introIdx).toBeGreaterThanOrEqual(2);
    expect(doneIdx - codeIdx).toBeGreaterThanOrEqual(2);
  });

  it('renders inline code', async () => {
    await renderOne('Use `console.log` to debug', 'console.log');
    const snapshot = tc.getSnapshot();
    const codeLine = snapshot.find(
      (line) => line.includes('console.log') && line.includes('debug')
    );
    expect(codeLine).toBeDefined();
  });

  it('renders headers with visual distinction', async () => {
    await renderOne(
      '# Main Title\n\nSome text\n\n## Subtitle\n\nMore text',
      'Subtitle'
    );
    const snapshot = tc.getSnapshot();
    const titleLine = snapshot.find((line) => line.includes('Main Title'));
    const subtitleLine = snapshot.find((line) => line.includes('Subtitle'));
    expect(titleLine).toBeDefined();
    expect(subtitleLine).toBeDefined();
    expect(titleLine!.includes('Some text')).toBe(false);
  });

  it('renders unordered lists with dashes', async () => {
    await renderOne(
      'Items:\n- First item\n- Second item\n- Third item',
      'Third item'
    );
    const snapshot = tc.getSnapshot();
    expect(snapshot.some((line) => line.includes('- First item'))).toBe(true);
    expect(snapshot.some((line) => line.includes('- Second item'))).toBe(true);
    expect(snapshot.some((line) => line.includes('- Third item'))).toBe(true);
  });

  it('renders list items with inline bold on same line', async () => {
    await renderOne(
      '- **Bold item** with description\n- **Another** one',
      'Bold item'
    );
    const snapshot = tc.getSnapshot();
    const line1 = snapshot.find(
      (line) => line.includes('Bold item') && line.includes('description')
    );
    expect(line1).toBeDefined();
  });

  it('preserves paragraph spacing', async () => {
    await renderOne(
      'First paragraph.\n\nSecond paragraph.',
      'Second paragraph'
    );
    const snapshot = tc.getSnapshot();
    const firstIdx = snapshot.findLastIndex((line) =>
      line.includes('First paragraph')
    );
    const secondIdx = snapshot.findLastIndex((line) =>
      line.includes('Second paragraph')
    );
    expect(secondIdx - firstIdx).toBeGreaterThanOrEqual(2);
  });

  it('renders blockquotes with prefix', async () => {
    await renderOne('> This is a quote\n\nNormal text', 'This is a quote');
    const snapshot = tc.getSnapshot();
    const quoteLine = snapshot.find((line) => line.includes('This is a quote'));
    expect(quoteLine).toBeDefined();
    expect(quoteLine!.includes('\u2502') || quoteLine!.includes('>')).toBe(
      true
    );
  });

  // Streaming test - markdown should keep rendering correctly across chunk
  // boundaries. ACP `agent_message_chunk` notifications are appended, so we
  // split the markdown into incremental chunks rather than overwriting state.
  it('preserves text during streaming', async () => {
    await render(['Hello ', '**bold', '** world'], 'world');
    const snapshot = tc.getSnapshot();
    const line = snapshot.find(
      (l) => l.includes('bold') && l.includes('world')
    );
    expect(line).toBeDefined();
  });

  it('renders underscore bold', async () => {
    await renderOne('Hello __bold text__ world', 'bold text');
    const snapshot = tc.getSnapshot();
    expect(
      snapshot.find((l) => l.includes('bold text') && l.includes('world'))
    ).toBeDefined();
  });

  it('renders strikethrough text', async () => {
    await renderOne('This is ~~deleted~~ text', 'deleted');
    const snapshot = tc.getSnapshot();
    expect(
      snapshot.find((l) => l.includes('deleted') && l.includes('text'))
    ).toBeDefined();
  });

  it('renders links with URL', async () => {
    await renderOne(
      'Visit [Example](https://example.com) for more',
      'example.com'
    );
    const snapshot = tc.getSnapshot();
    expect(
      // Match `'example.com)'` (with closing paren) so CodeQL's URL-substring
      // rule doesn't flag this terminal-row check as URL sanitization.
      snapshot.find((l) => l.includes('Example') && l.includes('example.com)'))
    ).toBeDefined();
  });

  it('renders mixed formatting in one message', async () => {
    await renderOne(
      '**bold** and *italic* and `code` and ~~struck~~',
      'struck'
    );
    const snapshot = tc.getSnapshot();
    expect(
      snapshot.find(
        (l) =>
          l.includes('bold') &&
          l.includes('italic') &&
          l.includes('code') &&
          l.includes('struck')
      )
    ).toBeDefined();
  });

  it('renders tables with aligned columns', async () => {
    await renderOne(
      '| Name | Age |\n|------|-----|\n| Alice | 30 |\n| Bob | 25 |',
      'Alice'
    );
    const snapshot = tc.getSnapshot();
    expect(snapshot.some((l) => l.includes('Alice') && l.includes('30'))).toBe(
      true
    );
    expect(snapshot.some((l) => l.includes('Bob') && l.includes('25'))).toBe(
      true
    );
    // Verify box-drawing borders are used instead of ASCII pipes
    expect(snapshot.some((l) => l.includes('│'))).toBe(true);
    expect(snapshot.some((l) => l.includes('┌') || l.includes('└'))).toBe(true);
    expect(snapshot.some((l) => l.includes('─'))).toBe(true);
  });

  it('renders horizontal rules', async () => {
    await renderOne('Above\n\n---\n\nBelow', 'Below');
    const snapshot = tc.getSnapshot();
    expect(snapshot.some((l) => l.includes('Above'))).toBe(true);
    expect(snapshot.some((l) => l.includes('Below'))).toBe(true);
  });

  it('renders ordered lists', async () => {
    await renderOne('1. First\n2. Second\n3. Third', 'Third');
    const snapshot = tc.getSnapshot();
    expect(snapshot.some((l) => l.includes('1.') && l.includes('First'))).toBe(
      true
    );
    expect(snapshot.some((l) => l.includes('2.') && l.includes('Second'))).toBe(
      true
    );
    expect(snapshot.some((l) => l.includes('3.') && l.includes('Third'))).toBe(
      true
    );
  });

  it('renders underscore italic', async () => {
    await renderOne('This is _italic text_ here', 'italic text');
    const snapshot = tc.getSnapshot();
    expect(
      snapshot.find((l) => l.includes('italic text') && l.includes('here'))
    ).toBeDefined();
  });

  it('renders header with spacing before it', async () => {
    await renderOne(
      'Intro text\n\n## Section One\n\nContent here\n\n## Section Two\n\nMore content',
      'Section Two'
    );
    const snapshot = tc.getSnapshot();
    const s1 = snapshot.findLastIndex((l) => l.includes('Section One'));
    const s2 = snapshot.findLastIndex((l) => l.includes('Section Two'));
    expect(s1).toBeGreaterThan(-1);
    expect(s2).toBeGreaterThan(s1);
    expect(snapshot[s1]!.includes('Content here')).toBe(false);
    const introIdx = snapshot.findLastIndex((l) => l.includes('Intro text'));
    expect(s1 - introIdx).toBeGreaterThanOrEqual(2);
  });

  it('renders **bold heading** with spacing like ## heading', async () => {
    await renderOne(
      '**Pancake Sort**\n- slow\n\n**Quick Sort**\n- fast',
      'fast'
    );
    await tc.sleepMs(500);
    const snapshot = tc.getSnapshot();
    const slowIdx = snapshot.findLastIndex((l) => l.includes('slow'));
    const quickIdx = snapshot.findLastIndex((l) => l.includes('Quick Sort'));
    expect(slowIdx).toBeGreaterThan(-1);
    expect(quickIdx).toBeGreaterThan(-1);
    expect(quickIdx - slowIdx).toBeGreaterThanOrEqual(2);
  });

  it('renders complete markdown document', async () => {
    await renderOne(
      '# Main Title\n\nIntro paragraph with **bold** and *italic*.\n\n## Features\n\n- First feature\n- **Second** feature\n- Third with `code`\n\n## Details\n\n> Important note here\n\n| Col A | Col B |\n|-------|-------|\n| 1 | 2 |\n\nVisit [docs](https://docs.example.com) for more.\n\n---\n\nFooter text.',
      'Footer text'
    );
    const snapshot = tc.getSnapshot();
    expect(snapshot.some((l) => l.includes('Main Title'))).toBe(true);
    expect(snapshot.some((l) => l.includes('bold'))).toBe(true);
    expect(snapshot.some((l) => l.includes('Features'))).toBe(true);
    expect(snapshot.some((l) => l.includes('- First feature'))).toBe(true);
    expect(snapshot.some((l) => l.includes('code'))).toBe(true);
    expect(snapshot.some((l) => l.includes('Details'))).toBe(true);
    expect(snapshot.some((l) => l.includes('Important note'))).toBe(true);
    expect(snapshot.some((l) => l.includes('Col A'))).toBe(true);
    expect(snapshot.some((l) => l.includes('docs'))).toBe(true);
    expect(snapshot.some((l) => l.includes('Footer text'))).toBe(true);
  });
});

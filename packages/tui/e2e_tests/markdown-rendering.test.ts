import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Markdown Rendering', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  // Baseline: existing features that must keep working
  it('renders bold text', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-bold')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Hello **bold text** world' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('bold text', 10000);
    await testCase.waitForText('world', 5000);

    // Verify it renders on a single line (not split)
    const snapshot = testCase.getSnapshot();
    const boldLine = snapshot.find(line => line.includes('bold text') && line.includes('world'));
    expect(boldLine).toBeDefined();
  }, 30000);

  it('renders code blocks with language', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-codeblock')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Here is code:\n```python\nprint("hello")\n```\nDone.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('print', 10000);
    await testCase.waitForText('Done', 5000);

    const snapshot = testCase.getSnapshot();
    // Code and surrounding text should both be visible
    expect(snapshot.some(line => line.includes('print'))).toBe(true);
    expect(snapshot.some(line => line.includes('Done'))).toBe(true);
    // Code block should be surrounded by blank lines
    const introIdx = snapshot.findIndex(l => l.includes('Here is code'));
    const codeIdx = snapshot.findIndex(l => l.includes('print'));
    const doneIdx = snapshot.findIndex(l => l.includes('Done'));
    expect(codeIdx - introIdx).toBeGreaterThanOrEqual(2);
    expect(doneIdx - codeIdx).toBeGreaterThanOrEqual(2);
  }, 30000);

  it('renders inline code', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-inline-code')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Use `console.log` to debug' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('console.log', 10000);
    // Should be on same line as surrounding text
    const snapshot = testCase.getSnapshot();
    const codeLine = snapshot.find(line => line.includes('console.log') && line.includes('debug'));
    expect(codeLine).toBeDefined();
  }, 30000);

  // Future features — these tests define expected behavior
  // Uncomment as features are implemented

  it('renders headers with visual distinction', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-headers')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: '# Main Title\n\nSome text\n\n## Subtitle\n\nMore text' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Main Title', 10000);
    await testCase.waitForText('Subtitle', 5000);

    const snapshot = testCase.getSnapshot();
    // Headers should be on their own lines
    const titleLine = snapshot.find(line => line.includes('Main Title'));
    const subtitleLine = snapshot.find(line => line.includes('Subtitle'));
    expect(titleLine).toBeDefined();
    expect(subtitleLine).toBeDefined();
    // Title should NOT contain 'Some text' (it's on a separate line)
    expect(titleLine!.includes('Some text')).toBe(false);
  }, 30000);

  it('renders unordered lists with dashes', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-unordered-list')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Items:\n- First item\n- Second item\n- Third item' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('First item', 10000);

    const snapshot = testCase.getSnapshot();
    // Each item on its own line with dash prefix
    expect(snapshot.some(line => line.includes('- First item'))).toBe(true);
    expect(snapshot.some(line => line.includes('- Second item'))).toBe(true);
    expect(snapshot.some(line => line.includes('- Third item'))).toBe(true);
  }, 30000);

  it('renders list items with inline bold on same line', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-list-bold')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: '- **Bold item** with description\n- **Another** one' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('Bold item', 10000);

    const snapshot = testCase.getSnapshot();
    // Bold text and description must be on the SAME line
    const line1 = snapshot.find(line => line.includes('Bold item') && line.includes('description'));
    expect(line1).toBeDefined();
  }, 30000);

  it('preserves paragraph spacing', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-paragraphs')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'First paragraph.\n\nSecond paragraph.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('First paragraph', 10000);
    await testCase.waitForText('Second paragraph', 5000);

    const snapshot = testCase.getSnapshot();
    // Find the two paragraphs
    const firstIdx = snapshot.findIndex(line => line.includes('First paragraph'));
    const secondIdx = snapshot.findIndex(line => line.includes('Second paragraph'));
    // There should be at least one blank line between them
    expect(secondIdx - firstIdx).toBeGreaterThanOrEqual(2);
  }, 30000);

  it('renders blockquotes with prefix', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-blockquote')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: '> This is a quote\n\nNormal text' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('This is a quote', 10000);

    const snapshot = testCase.getSnapshot();
    // Quote should have a visual prefix
    const quoteLine = snapshot.find(line => line.includes('This is a quote'));
    expect(quoteLine).toBeDefined();
    expect(quoteLine!.includes('\u2502') || quoteLine!.includes('>')).toBe(true);
  }, 30000);

  // Streaming test — verify markdown doesn't break during streaming
  it('preserves text during streaming', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-streaming')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Stream content in chunks (simulates token-by-token)
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Hello ' } } },
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Hello **bold' } } },
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Hello **bold** world' } } },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    await testCase.waitForText('bold', 10000);
    await testCase.waitForText('world', 5000);

    const snapshot = testCase.getSnapshot();
    // Final state should have bold and world on same line
    const line = snapshot.find(l => l.includes('bold') && l.includes('world'));
    expect(line).toBeDefined();
  }, 30000);

  it('renders underscore bold', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-underscore-bold')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Hello __bold text__ world' } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('bold text', 10000);
    const snapshot = testCase.getSnapshot();
    expect(snapshot.find(l => l.includes('bold text') && l.includes('world'))).toBeDefined();
  }, 30000);

  it('renders strikethrough text', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-strikethrough')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'This is ~~deleted~~ text' } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('deleted', 10000);
    const snapshot = testCase.getSnapshot();
    expect(snapshot.find(l => l.includes('deleted') && l.includes('text'))).toBeDefined();
  }, 30000);

  it('renders links with URL', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-links')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Visit [Example](https://example.com) for more' } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Example', 10000);
    await testCase.waitForText('example.com', 5000);
    const snapshot = testCase.getSnapshot();
    expect(snapshot.find(l => l.includes('Example') && l.includes('example.com'))).toBeDefined();
  }, 30000);

  it('renders mixed formatting in one message', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-mixed')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: '**bold** and *italic* and `code` and ~~struck~~' } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('bold', 10000);
    const snapshot = testCase.getSnapshot();
    expect(snapshot.find(l => l.includes('bold') && l.includes('italic') && l.includes('code') && l.includes('struck'))).toBeDefined();
  }, 30000);

  it('renders tables with aligned columns', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-table')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: '| Name | Age |\n|------|-----|\n| Alice | 30 |\n| Bob | 25 |' } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Alice', 10000);
    const snapshot = testCase.getSnapshot();
    expect(snapshot.some(l => l.includes('Alice') && l.includes('30'))).toBe(true);
    expect(snapshot.some(l => l.includes('Bob') && l.includes('25'))).toBe(true);
  }, 30000);

  it('renders horizontal rules', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-hr')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Above\n\n---\n\nBelow' } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Above', 10000);
    await testCase.waitForText('Below', 5000);
    const snapshot = testCase.getSnapshot();
    expect(snapshot.some(l => l.includes('Above'))).toBe(true);
    expect(snapshot.some(l => l.includes('Below'))).toBe(true);
  }, 30000);

  it('renders ordered lists', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-ordered-list')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: '1. First\n2. Second\n3. Third' } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('First', 10000);
    const snapshot = testCase.getSnapshot();
    expect(snapshot.some(l => l.includes('1.') && l.includes('First'))).toBe(true);
    expect(snapshot.some(l => l.includes('2.') && l.includes('Second'))).toBe(true);
    expect(snapshot.some(l => l.includes('3.') && l.includes('Third'))).toBe(true);
  }, 30000);

  it('renders underscore italic', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-underscore-italic')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'This is _italic text_ here' } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('italic text', 10000);
    const snapshot = testCase.getSnapshot();
    expect(snapshot.find(l => l.includes('italic text') && l.includes('here'))).toBeDefined();
  }, 30000);

  it('renders header with spacing before it', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-header-spacing')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'Intro text\n\n## Section One\n\nContent here\n\n## Section Two\n\nMore content' } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Section One', 10000);
    await testCase.waitForText('Section Two', 5000);
    const snapshot = testCase.getSnapshot();
    // Both headers should be present on their own lines
    const s1 = snapshot.findIndex(l => l.includes('Section One'));
    const s2 = snapshot.findIndex(l => l.includes('Section Two'));
    expect(s1).toBeGreaterThan(-1);
    expect(s2).toBeGreaterThan(s1);
    // Headers should not be on same line as content
    expect(snapshot[s1]!.includes('Content here')).toBe(false);
    // Headers should have a blank line before them
    const introIdx = snapshot.findIndex(l => l.includes('Intro text'));
    expect(s1 - introIdx).toBeGreaterThanOrEqual(2);
  }, 30000);

  it('renders **bold heading** with spacing like ## heading', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-bold-heading')
      .withTerminal({ width: 80, height: 24 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: '**Pancake Sort**\n- slow\n\n**Quick Sort**\n- fast' } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('fast', 10000);
    await testCase.sleepMs(500);
    const snapshot = testCase.getSnapshot();
    const slowIdx = snapshot.findIndex(l => l.includes('slow'));
    const quickIdx = snapshot.findIndex(l => l.includes('Quick Sort'));
    expect(slowIdx).toBeGreaterThan(-1);
    expect(quickIdx).toBeGreaterThan(-1);
    // **Quick Sort** should have a blank line before it (like a ## heading would)
    expect(quickIdx - slowIdx).toBeGreaterThanOrEqual(2);
  }, 30000);

  it('renders complete markdown document', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('markdown-full-doc')
      .withTerminal({ width: 100, height: 40 })
      .launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: '# Main Title\n\nIntro paragraph with **bold** and *italic*.\n\n## Features\n\n- First feature\n- **Second** feature\n- Third with `code`\n\n## Details\n\n> Important note here\n\n| Col A | Col B |\n|-------|-------|\n| 1 | 2 |\n\nVisit [docs](https://docs.example.com) for more.\n\n---\n\nFooter text.' } } },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.sendKeys('test');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Main Title', 15000);
    const snapshot = testCase.getSnapshot();
    // Verify all elements rendered
    expect(snapshot.some(l => l.includes('Main Title'))).toBe(true);
    expect(snapshot.some(l => l.includes('bold'))).toBe(true);
    expect(snapshot.some(l => l.includes('Features'))).toBe(true);
    expect(snapshot.some(l => l.includes('- First feature'))).toBe(true);
    expect(snapshot.some(l => l.includes('code'))).toBe(true);
    expect(snapshot.some(l => l.includes('Details'))).toBe(true);
    expect(snapshot.some(l => l.includes('Important note'))).toBe(true);
    expect(snapshot.some(l => l.includes('Col A'))).toBe(true);
    expect(snapshot.some(l => l.includes('docs'))).toBe(true);
    expect(snapshot.some(l => l.includes('Footer text'))).toBe(true);
  }, 30000);
});
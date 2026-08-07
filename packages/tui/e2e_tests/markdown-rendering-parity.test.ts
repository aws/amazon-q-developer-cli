import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

const MARKDOWN_CHUNKS = [
  [
    '# Shared markdown semantics',
    '',
    'Intro with **bold text**, *italic text*, `inline code`, and [semantic link](https://example.com/semantic).',
    '',
    '- parent item',
    '  - nested item',
    '1. ordered item',
    '',
    '> first quote line',
    '> second quote line',
    '',
    '```typescript',
    'const streamedFence = ',
  ].join('\n'),
  [
    '"fence-body";',
    '```',
    '',
    '| Mode | Renderer | Evidence |',
    '| --- | --- | --- |',
    '| TUI | **React** | table cells visible |',
    '| Lite | `ANSI` | wide 界 text |',
    '',
    'This deliberately long paragraph keeps semantic words in order while terminal wrapping differs between painters: alpha beta gamma delta epsilon zeta eta theta iota kappa lambda.',
    '',
    'Tail paragraph after the table.',
  ].join('\n'),
];

const INCOMPLETE_FENCE_PROBE = [
  '```typescript',
  'const incompleteFenceProbe = true;',
].join('\n');

const EXPECTED_SEMANTICS = [
  'Shared markdown semantics',
  'bold text',
  'italic text',
  'inline code',
  'semantic link',
  'https://example.com/semantic',
  'parent item',
  'nested item',
  'ordered item',
  'first quote line',
  'second quote line',
  'const streamedFence = "fence-body";',
  '',
  'Mode',
  'Renderer',
  'TUI',
  'React',
  'Lite',
  'ANSI',
  'wide 界 text',
  'alpha beta gamma',
  'iota kappa lambda',
  'Tail paragraph after the table.',
].filter(Boolean);

function expectInOrder(text: string, expected: readonly string[]): void {
  let cursor = 0;
  for (const semantic of expected) {
    const index = text.indexOf(semantic, cursor);
    expect(
      index,
      `missing or out of order: ${semantic}`
    ).toBeGreaterThanOrEqual(cursor);
    cursor = index + semantic.length;
  }
}

function expectRenderedStructure(lines: readonly string[]): void {
  const text = lines.join('\n');
  expect(text).not.toContain('**bold text**');
  expect(text).not.toContain('[semantic link](https://');
  expect(text).not.toContain('```typescript');
  expect(text).not.toContain('| --- |');

  const parent = lines.find((line) => line.includes('parent item'));
  const nested = lines.find((line) => line.includes('nested item'));
  expect(parent).toBeDefined();
  expect(nested).toBeDefined();
  expect(nested!.indexOf('nested item')).toBeGreaterThan(
    parent!.indexOf('parent item')
  );

  const firstQuote = lines.find((line) => line.includes('first quote line'));
  const secondQuote = lines.find((line) => line.includes('second quote line'));
  expect(firstQuote).toBeDefined();
  expect(secondQuote).toBeDefined();
  expect(firstQuote!.slice(0, firstQuote!.indexOf('first quote line'))).toBe(
    secondQuote!.slice(0, secondQuote!.indexOf('second quote line'))
  );

  expect(
    lines.some(
      (line) =>
        line.includes('Mode') &&
        line.includes('Renderer') &&
        (line.match(/│/g)?.length ?? 0) >= 2
    )
  ).toBe(true);
  expect(
    lines.some(
      (line) =>
        line.includes('const streamedFence = "fence-body";') &&
        !line.includes('```')
    )
  ).toBe(true);
}

async function pushStreamingTextChunk(
  testCase: E2ETestCase,
  content: string
): Promise<void> {
  // The response parser peeks past text to detect a following code-reference
  // event. An empty text event releases this chunk without closing the stream.
  await testCase.pushSendMessageResponse([
    {
      kind: 'event',
      data: {
        kind: 'AssistantResponseEvent',
        data: { content },
      },
    },
    {
      kind: 'event',
      data: {
        kind: 'AssistantResponseEvent',
        data: { content: '' },
      },
    },
  ]);
}

describe('Markdown rendering parity smoke', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  for (const mode of ['tui', 'lite'] as const) {
    it(`${mode} renders shared markdown table content`, async () => {
      let builder = E2ETestCase.builder()
        .withTestName(`markdown-rendering-parity-${mode}`)
        .withTerminal({ width: 100, height: 32 });
      if (mode === 'lite') builder = builder.withLite();

      testCase = await builder.launch();
      await testCase.waitForText('ask a question', 10000);
      await testCase.getSessionId();

      await pushStreamingTextChunk(testCase, INCOMPLETE_FENCE_PROBE);

      await testCase.sendKeys('show incomplete markdown');
      await testCase.pressEnter();
      await testCase.waitForText('const incompleteFenceProbe', 15000);
      const partial = testCase.getSnapshot().join('\n');
      expect(partial).toContain('const incompleteFenceProbe = true;');
      expect(partial).not.toContain('```typescript');
      expect((await testCase.getStore()).isProcessing).toBe(true);

      await testCase.pushSendMessageResponse(null);
      await testCase.waitForIdle(15000);

      await pushStreamingTextChunk(testCase, MARKDOWN_CHUNKS[0]!);

      await testCase.sendKeys('show markdown table');
      await testCase.pressEnter();
      await testCase.waitForText('const streamedFence', 15000);
      const firstChunk = testCase.getSnapshot().join('\n');
      expect(firstChunk).toContain('Shared markdown semantics');
      expect(firstChunk).toContain('const streamedFence =');
      expect(firstChunk).not.toContain('Tail paragraph after the table.');
      expect((await testCase.getStore()).isProcessing).toBe(true);

      await pushStreamingTextChunk(testCase, MARKDOWN_CHUNKS[1]!);
      await testCase.waitForText('Tail paragraph', 15000);
      expect((await testCase.getStore()).isProcessing).toBe(true);

      await testCase.pushSendMessageResponse(null);
      await testCase.waitForIdle(15000);
      await testCase.sleepMs(500);

      const snapshot = testCase.getSnapshot();
      const text = snapshot.join('\n');
      expectInOrder(text, EXPECTED_SEMANTICS);
      expectRenderedStructure(snapshot);
    }, 45000);
  }
});

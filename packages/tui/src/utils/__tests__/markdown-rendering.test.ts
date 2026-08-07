import { describe, expect, it } from 'bun:test';
import stripAnsi from 'strip-ansi';
import { chalk } from '../color.js';
import { UNICODE_GLYPHS } from '../glyphs.js';
import { parseMarkdown } from '../markdown.js';
import {
  buildMarkdownRenderBlocks,
  needsMarkdownSpacingBefore,
  renderMarkdownInlineText,
  renderMarkdownTableLines,
} from '../markdown-rendering.js';
import { visibleWidth } from '../text-width.js';

const fixture = [
  'Paragraph with **bold** and [label](https://example.com).',
  '',
  '- parent',
  '  - nested',
  '',
  '> quote',
  '',
  '```ts',
  'const value = true;',
  '```',
  '',
  '| Name | Value |',
  '| --- | ---: |',
  '| wide 界 | `code` |',
].join('\n');

describe('shared markdown rendering semantics', () => {
  it('preserves rich block order and payloads', () => {
    const blocks = buildMarkdownRenderBlocks(parseMarkdown(fixture));

    expect(blocks.map((block) => block.type)).toEqual([
      'text',
      'listItem',
      'listItem',
      'blockquote',
      'code',
      'table',
    ]);
    expect(blocks[1]).toMatchObject({
      type: 'listItem',
      segment: { text: 'parent', listItem: { indent: 0 } },
    });
    expect(blocks[2]).toMatchObject({
      type: 'listItem',
      segment: { text: 'nested', listItem: { indent: 1 } },
    });
    expect(blocks[4]).toMatchObject({
      type: 'code',
      segment: {
        codeBlock: {
          code: 'const value = true;',
          language: 'ts',
          isComplete: true,
        },
      },
    });
    expect(blocks[5]).toMatchObject({
      type: 'table',
      segment: {
        table: {
          headers: ['Name', 'Value'],
          rows: [['wide 界', '`code`']],
          alignments: ['left', 'right'],
        },
      },
    });
  });

  it('preserves de-indented lists, adjacent quotes, and incomplete fences in order', () => {
    const blocks = buildMarkdownRenderBlocks(
      parseMarkdown(
        [
          '- parent',
          '  - nested',
          '- sibling',
          '',
          '> first quote',
          '> second quote',
          '',
          '```ts',
          'const pending = true;',
        ].join('\n')
      )
    );

    expect(blocks.map((block) => block.type)).toEqual([
      'listItem',
      'listItem',
      'listItem',
      'blockquote',
      'blockquote',
      'code',
    ]);
    expect(
      blocks
        .slice(0, 3)
        .map((block) =>
          block.type === 'listItem' ? block.segment.listItem!.indent : null
        )
    ).toEqual([0, 1, 0]);
    expect(
      blocks
        .slice(3, 5)
        .map((block) =>
          block.type === 'blockquote' ? block.segment.text : null
        )
    ).toEqual(['first quote', 'second quote']);
    expect(blocks[5]).toMatchObject({
      type: 'code',
      segment: {
        codeBlock: {
          code: 'const pending = true;',
          language: 'ts',
          isComplete: false,
        },
      },
    });
  });

  it('passes equivalent inline meaning to mode-specific painters', () => {
    const calls: Array<[string, string, boolean]> = [];
    const rendered = renderMarkdownInlineText(
      '**bold** [label](https://example.com) https://amazon.com',
      {
        text: (text) => text,
        inlineCode: (text) => `<code>${text}</code>`,
        bold: (text) => `<bold>${text}</bold>`,
        italic: (text) => `<italic>${text}</italic>`,
        strikethrough: (text) => `<strike>${text}</strike>`,
        link: (text, url, isBareUrl) => {
          calls.push([text, url, isBareUrl]);
          return `<link>${text}</link>`;
        },
      }
    );

    expect(rendered).toBe(
      '<bold>bold</bold> <link>label</link> <link>https://amazon.com</link>'
    );
    expect(calls).toEqual([
      ['label', 'https://example.com', false],
      ['https://amazon.com', 'https://amazon.com', true],
    ]);
  });

  it('preserves nested ANSI styles with real chalk painters', () => {
    const previousLevel = chalk.level;
    chalk.level = 3;

    try {
      const rendered = renderMarkdownInlineText(
        '~~***all three***~~ **`code`**',
        {
          text: (text) => text,
          inlineCode: chalk.cyan,
          bold: chalk.bold,
          italic: chalk.italic,
          strikethrough: chalk.strikethrough,
          link: (text) => chalk.underline(text),
        }
      );

      expect(rendered).toContain('\x1b[1m');
      expect(rendered).toContain('\x1b[3m');
      expect(rendered).toContain('\x1b[9m');
      expect(rendered).toContain('\x1b[36m');
      expect(stripAnsi(rendered)).toBe('all three code');
    } finally {
      chalk.level = previousLevel;
    }
  });

  it('uses one list-spacing rule for nesting and de-indenting', () => {
    const blocks = buildMarkdownRenderBlocks(
      parseMarkdown(['- parent', '  - nested', '- sibling'].join('\n'))
    );

    expect(needsMarkdownSpacingBefore(blocks[0]!, blocks[1]!)).toBe(false);
    expect(needsMarkdownSpacingBefore(blocks[1]!, blocks[2]!)).toBe(true);
  });

  it('measures ANSI styling and wide characters by visible terminal width', () => {
    const { lines, stacked } = renderMarkdownTableLines(
      {
        headers: ['Name', 'Value'],
        rows: [['wide 界', '**styled**']],
        alignments: ['left', 'right'],
      },
      {
        termWidth: 40,
        glyphs: UNICODE_GLYPHS,
        renderInline: (text) => chalk.cyan(text.replace(/\*\*/g, '')),
      }
    );

    expect(stacked).toBe(false);
    expect(lines.every((line) => visibleWidth(line) <= 40)).toBe(true);
    expect(lines.join('\n')).toContain('wide 界');
    expect(lines.join('\n')).toContain('styled');
  });

  it('renders header-only tables when a narrow terminal selects stacked mode', () => {
    const { lines, stacked } = renderMarkdownTableLines(
      {
        headers: ['Command', 'Description'],
        rows: [],
        alignments: ['left', 'left'],
      },
      {
        termWidth: 12,
        glyphs: UNICODE_GLYPHS,
        renderInline: (text) => text,
      }
    );

    expect(stacked).toBe(true);
    expect(lines.map(stripAnsi)).toEqual(['Command', 'Description']);
  });
});

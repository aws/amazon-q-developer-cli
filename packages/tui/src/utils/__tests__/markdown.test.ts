import { describe, it, expect } from 'bun:test';
import { parseMarkdown } from '../markdown.js';

describe('parseMarkdown', () => {
  // 1. Basic Text (no markdown)
  describe('Basic Text', () => {
    it('should handle empty string', () => {
      expect(parseMarkdown('')).toEqual([]);
    });

    it('should handle plain text only', () => {
      expect(parseMarkdown('Hello world')).toEqual([{ text: 'Hello world' }]);
    });

    it('should handle text with newlines', () => {
      expect(parseMarkdown('Line 1\nLine 2')).toEqual([
        { text: 'Line 1\nLine 2' },
      ]);
    });
  });

  // 2. Inline Markdown
  describe('Inline Markdown', () => {
    it('should parse bold text', () => {
      expect(parseMarkdown('**bold**')).toEqual([{ text: 'bold', boldHeading: true }]);
    });

    it('should parse italic text', () => {
      expect(parseMarkdown('*italic*')).toEqual([
        { text: 'italic', italic: true },
      ]);
    });

    it('should parse inline code', () => {
      expect(parseMarkdown('`code`')).toEqual([{ text: 'code', quote: true }]);
    });

    it('should parse mixed inline markdown', () => {
      expect(parseMarkdown('Hello **bold** and *italic* text')).toEqual([
        { text: 'Hello ' },
        { text: 'bold', bold: true },
        { text: ' and ' },
        { text: 'italic', italic: true },
        { text: ' text' },
      ]);
    });
  });

  // 3. Complete Code Blocks
  describe('Complete Code Blocks', () => {
    it('should parse code block with language', () => {
      expect(parseMarkdown('```rust\nfn main() {}\n```')).toEqual([
        {
          text: '',
          codeBlock: {
            code: 'fn main() {}\n',
            language: 'rust',
            isComplete: true,
          },
        },
      ]);
    });

    it('should parse code block without language', () => {
      expect(parseMarkdown('```\nsome code\n```')).toEqual([
        {
          text: '',
          codeBlock: {
            code: 'some code\n',
            language: undefined,
            isComplete: true,
          },
        },
      ]);
    });

    it('should parse empty code block', () => {
      expect(parseMarkdown('```rust\n```')).toEqual([
        {
          text: '',
          codeBlock: { code: '', language: 'rust', isComplete: true },
        },
      ]);
    });
  });

  // 4. Mixed Content
  describe('Mixed Content', () => {
    it('should parse text + code + text', () => {
      expect(parseMarkdown('Hello ```rust\ncode\n``` world')).toEqual([
        { text: 'Hello ' },
        {
          text: '',
          codeBlock: { code: 'code\n', language: 'rust', isComplete: true },
        },
        { text: ' world' },
      ]);
    });

    it('should parse multiple code blocks', () => {
      expect(
        parseMarkdown('```rust\ncode1\n```\n```python\ncode2\n```')
      ).toEqual([
        {
          text: '',
          codeBlock: { code: 'code1\n', language: 'rust', isComplete: true },
        },
        {
          text: '',
          codeBlock: { code: 'code2\n', language: 'python', isComplete: true },
        },
      ]);
    });

    it('should parse inline markdown + code blocks', () => {
      expect(
        parseMarkdown('**Bold** text\n```rust\ncode\n```\n*italic*')
      ).toEqual([
        { text: 'Bold', bold: true },
        { text: ' text' },
        {
          text: '',
          codeBlock: { code: 'code\n', language: 'rust', isComplete: true },
        },
        { text: 'italic', italic: true },
      ]);
    });
  });

  // 5. Streaming/Incomplete Blocks
  describe('Streaming/Incomplete Blocks', () => {
    it('should handle just opening fence', () => {
      expect(parseMarkdown('```rust')).toEqual([
        {
          text: '',
          codeBlock: { code: '', language: 'rust', isComplete: false },
        },
      ]);
    });

    it('should handle opening + partial code', () => {
      expect(parseMarkdown('```rust\npartial code')).toEqual([
        {
          text: '',
          codeBlock: {
            code: 'partial code',
            language: 'rust',
            isComplete: false,
          },
        },
      ]);
    });

    it('should handle text before incomplete code block', () => {
      expect(parseMarkdown('Hello ```rust\npartial')).toEqual([
        { text: 'Hello ' },
        {
          text: '',
          codeBlock: { code: 'partial', language: 'rust', isComplete: false },
        },
      ]);
    });
  });

  // 6. Edge Cases
  describe('Edge Cases', () => {
    it('should handle code ending at EOF (no trailing newline)', () => {
      expect(parseMarkdown('```rust\ncode\n```')).toEqual([
        {
          text: '',
          codeBlock: { code: 'code\n', language: 'rust', isComplete: true },
        },
      ]);
    });

    it('should handle backticks in code content', () => {
      expect(parseMarkdown('```rust\nprintln!("use ```rust");\n```')).toEqual([
        {
          text: '',
          codeBlock: {
            code: 'println!("use ```rust");\n',
            language: 'rust',
            isComplete: true,
          },
        },
      ]);
    });

    it('should handle nested language names in code', () => {
      expect(
        parseMarkdown('```markdown\nUse ```python for Python\n```')
      ).toEqual([
        {
          text: '',
          codeBlock: {
            code: 'Use ```python for Python\n',
            language: 'markdown',
            isComplete: true,
          },
        },
      ]);
    });

    it('should handle whitespace after closing fence', () => {
      expect(parseMarkdown('```rust\ncode\n```   \nmore text')).toEqual([
        {
          text: '',
          codeBlock: { code: 'code\n', language: 'rust', isComplete: true },
        },
        { text: '   \nmore text' },
      ]);
    });
  });

  // 7. Paragraph Spacing
  describe('Paragraph Spacing', () => {
    it('should preserve blank lines between paragraphs', () => {
      expect(parseMarkdown('First paragraph.\n\nSecond paragraph.')).toEqual([
        { text: 'First paragraph.\n\nSecond paragraph.' },
      ]);
    });

    it('should handle multiple blank lines', () => {
      expect(parseMarkdown('Para 1.\n\n\nPara 2.')).toEqual([
        { text: 'Para 1.\n\n\nPara 2.' },
      ]);
    });
  });

  describe('Underscore Emphasis', () => {
    it('should parse underscore bold', () => {
      expect(parseMarkdown('__bold__')).toEqual([{ text: 'bold', bold: true }]);
    });
    it('should parse underscore italic', () => {
      expect(parseMarkdown('_italic_')).toEqual([
        { text: 'italic', italic: true },
      ]);
    });
    it('should parse mixed bold styles', () => {
      expect(parseMarkdown('**bold** and __also bold__')).toEqual([
        { text: 'bold', bold: true },
        { text: ' and ' },
        { text: 'also bold', bold: true },
      ]);
    });
  });

  describe('Strikethrough', () => {
    it('should parse strikethrough', () => {
      expect(parseMarkdown('~~deleted~~')).toEqual([
        { text: 'deleted', strikethrough: true },
      ]);
    });
    it('should parse strikethrough with surrounding text', () => {
      expect(parseMarkdown('keep ~~remove~~ keep')).toEqual([
        { text: 'keep ' },
        { text: 'remove', strikethrough: true },
        { text: ' keep' },
      ]);
    });
  });

  describe('Links', () => {
    it('should parse link', () => {
      expect(parseMarkdown('[Example](https://example.com)')).toEqual([
        { text: 'Example', link: { url: 'https://example.com' } },
      ]);
    });
    it('should parse link with surrounding text', () => {
      expect(parseMarkdown('Visit [here](https://x.com) now')).toEqual([
        { text: 'Visit ' },
        { text: 'here', link: { url: 'https://x.com' } },
        { text: ' now' },
      ]);
    });
  });

  describe('Tables', () => {
    it('should parse simple table', () => {
      const result = parseMarkdown(
        '| Name | Age |\n|------|-----|\n| John | 25  |'
      );
      expect(result).toEqual([
        {
          text: '',
          table: {
            headers: ['Name', 'Age'],
            rows: [['John', '25']],
            alignments: ['left', 'left'],
          },
        },
      ]);
    });

    it('should parse table with alignment', () => {
      const result = parseMarkdown(
        '| Name | Age | Score |\n|:-----|----:|:-----:|\n| John | 25  | 95    |'
      );
      expect(result).toEqual([
        {
          text: '',
          table: {
            headers: ['Name', 'Age', 'Score'],
            rows: [['John', '25', '95']],
            alignments: ['left', 'right', 'center'],
          },
        },
      ]);
    });

    it('should parse basic table', () => {
      const result = parseMarkdown(
        '| Name | Age |\n|------|-----|\n| Alice | 30 |'
      );
      expect(result.length).toBe(1);
      const firstResult = result[0];
      expect(firstResult).toBeDefined();
      expect(firstResult?.table).toBeDefined();
      if (firstResult?.table) {
        expect(firstResult.table.headers).toEqual(['Name', 'Age']);
        expect(firstResult.table.rows).toEqual([['Alice', '30']]);
      }
    });

    it('should parse table alignments', () => {
      const result = parseMarkdown(
        '| L | C | R |\n|:--|:--:|--:|\n| a | b | c |'
      );
      const firstResult = result[0];
      if (firstResult?.table) {
        expect(firstResult.table.alignments).toEqual([
          'left',
          'center',
          'right',
        ]);
      }
    });

    it('should parse table with multiple rows', () => {
      const result = parseMarkdown(
        '| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |'
      );
      const firstResult = result[0];
      if (firstResult?.table) {
        expect(firstResult.table.rows.length).toBe(2);
      }
    });

    it('should handle table with surrounding text', () => {
      const result = parseMarkdown('Before\n| A |\n|---|\n| 1 |\nAfter');
      expect(result.length).toBe(3); // text, table, text
      const secondResult = result[1];
      expect(secondResult).toBeDefined();
      expect(secondResult?.table).toBeDefined();
    });
  });

  describe('Headers', () => {
    it('should parse H1', () => {
      expect(parseMarkdown('# Title')).toEqual([{ text: 'Title', header: 1 }]);
    });
    it('should parse H2', () => {
      expect(parseMarkdown('## Subtitle')).toEqual([
        { text: 'Subtitle', header: 2 },
      ]);
    });
    it('should parse H3', () => {
      expect(parseMarkdown('### Section')).toEqual([
        { text: 'Section', header: 3 },
      ]);
    });
    it('should parse H4 through H6', () => {
      expect(parseMarkdown('#### H4')[0]?.header).toBe(4);
      expect(parseMarkdown('##### H5')[0]?.header).toBe(5);
      expect(parseMarkdown('###### H6')[0]?.header).toBe(6);
    });
    it('should not parse 7+ hashes as header', () => {
      expect(parseMarkdown('####### nope')[0]?.header).toBeUndefined();
    });
    it('should not parse # without space as header', () => {
      expect(parseMarkdown('#nospace')[0]?.header).toBeUndefined();
    });
    it('should parse header with surrounding content', () => {
      const result = parseMarkdown('text before\n## Header\ntext after');
      const header = result.find((s) => s.header);
      expect(header).toEqual({ text: 'Header', header: 2 });
    });
  });

  describe('Lists', () => {
    it('should parse unordered list', () => {
      const result = parseMarkdown('- item1\n- item2');
      expect(result[0]?.listItem).toEqual({ ordered: false, indent: 0 });
      expect(result[0]?.text).toBe('item1');
      expect(result[1]?.listItem).toEqual({ ordered: false, indent: 0 });
    });
    it('should parse ordered list', () => {
      const result = parseMarkdown('1. first\n2. second');
      expect(result[0]?.listItem).toEqual({
        ordered: true,
        number: 1,
        indent: 0,
      });
      expect(result[1]?.listItem).toEqual({
        ordered: true,
        number: 2,
        indent: 0,
      });
    });
    it('should parse nested list', () => {
      const result = parseMarkdown('- top\n  - nested');
      expect(result[0]?.listItem?.indent).toBe(0);
      expect(result[1]?.listItem?.indent).toBe(1);
    });
    it('should keep list item text raw for inline parsing', () => {
      const result = parseMarkdown('- **bold** text');
      expect(result[0]?.text).toBe('**bold** text');
      expect(result[0]?.listItem).toBeDefined();
    });
  });

  describe('Blockquotes', () => {
    it('should parse blockquote', () => {
      const result = parseMarkdown('> quoted text');
      expect(result[0]?.blockquote).toBe(true);
      expect(result[0]?.text).toBe('quoted text');
    });
    it('should parse blockquote without space after >', () => {
      const result = parseMarkdown('>no space');
      expect(result[0]?.blockquote).toBe(true);
      expect(result[0]?.text).toBe('no space');
    });
  });

  describe('Horizontal Rules', () => {
    it('should parse --- as horizontal rule', () => {
      const result = parseMarkdown('text\n---\nmore');
      const hr = result.find((s) => s.horizontalRule);
      expect(hr).toBeDefined();
    });
    it('should parse standalone ---', () => {
      const result = parseMarkdown('---');
      expect(result[0]?.horizontalRule).toBe(true);
    });
    it('should not parse -- as horizontal rule', () => {
      const result = parseMarkdown('--');
      expect(result[0]?.horizontalRule).toBeUndefined();
    });
  });

  describe('Combined Content Flow', () => {
    it('should parse header + list + text', () => {
      const result = parseMarkdown('## Title\n- item1\n- item2\nSome text');
      expect(result[0]?.header).toBe(2);
      expect(result[1]?.listItem).toBeDefined();
      expect(result[2]?.listItem).toBeDefined();
      expect(result[3]?.text).toBe('Some text');
    });
    it('should parse header + text + header', () => {
      const result = parseMarkdown('# First\nParagraph\n## Second');
      expect(result[0]?.header).toBe(1);
      expect(result[2]?.header).toBe(2);
    });
  });

  describe('Block Separator Handling', () => {
    // 1. Leading newlines
    describe('Leading newlines', () => {
      it('should strip leading \\n from first segment', () => {
        const result = parseMarkdown('\nHello world');
        expect(result).toEqual([{ text: 'Hello world' }]);
      });

      it('should strip leading \\n\\n from first segment', () => {
        const result = parseMarkdown('\n\nHello world');
        expect(result).toEqual([{ text: 'Hello world' }]);
      });

      it('should strip leading \\n\\n\\n from first segment', () => {
        const result = parseMarkdown('\n\n\nHello world');
        expect(result).toEqual([{ text: 'Hello world' }]);
      });

      it('should strip leading \\n\\n before a header as first content', () => {
        const result = parseMarkdown('\n\n## Title');
        expect(result).toEqual([{ text: 'Title', header: 2 }]);
      });

      it('should strip leading \\n\\n before a code block as first content', () => {
        const result = parseMarkdown('\n\n```python\ncode\n```');
        expect(result).toEqual([
          { text: '', codeBlock: { code: 'code\n', language: 'python', isComplete: true } },
        ]);
      });
    });

    // 2. Text & header interaction
    describe('Text and header interaction', () => {
      it('should strip \\n\\n between text and header', () => {
        const result = parseMarkdown('Hello\n\n## Title');
        expect(result).toEqual([
          { text: 'Hello' },
          { text: 'Title', header: 2 },
        ]);
      });

      it('should strip \\n\\n\\n between text and header', () => {
        const result = parseMarkdown('Hello\n\n\n## Title');
        expect(result).toEqual([
          { text: 'Hello' },
          { text: 'Title', header: 2 },
        ]);
      });

      it('should strip \\n between text and header', () => {
        const result = parseMarkdown('Hello\n## Title');
        expect(result).toEqual([
          { text: 'Hello' },
          { text: 'Title', header: 2 },
        ]);
      });

      it('should preserve \\n\\n between plain text paragraphs', () => {
        expect(parseMarkdown('First paragraph.\n\nSecond paragraph.')).toEqual([
          { text: 'First paragraph.\n\nSecond paragraph.' },
        ]);
      });

      it('should strip \\n\\n between text and list item', () => {
        const result = parseMarkdown('Hello\n\n- item');
        expect(result).toEqual([
          { text: 'Hello' },
          { text: 'item', listItem: { ordered: false, indent: 0 } },
        ]);
      });

      it('should strip \\n\\n between text and blockquote', () => {
        const result = parseMarkdown('Hello\n\n> quote');
        expect(result).toEqual([
          { text: 'Hello' },
          { text: 'quote', blockquote: true },
        ]);
      });

      it('should strip \\n\\n between text and horizontal rule', () => {
        const result = parseMarkdown('Hello\n\n---');
        expect(result).toEqual([
          { text: 'Hello' },
          { text: '', horizontalRule: true },
        ]);
      });
    });

    // 3. Header & code block interaction
    describe('Header and code block interaction', () => {
      it('should strip \\n\\n between header and code block', () => {
        const result = parseMarkdown('## Title\n\n```python\ncode\n```');
        expect(result).toEqual([
          { text: 'Title', header: 2 },
          { text: '', codeBlock: { code: 'code\n', language: 'python', isComplete: true } },
        ]);
      });

      it('should strip \\n\\n between code block and header', () => {
        const result = parseMarkdown('```python\ncode\n```\n\n## Title');
        expect(result).toEqual([
          { text: '', codeBlock: { code: 'code\n', language: 'python', isComplete: true } },
          { text: 'Title', header: 2 },
        ]);
      });

      it('should strip \\n\\n between code block and text', () => {
        const result = parseMarkdown('```python\ncode\n```\n\nSome text');
        expect(result).toEqual([
          { text: '', codeBlock: { code: 'code\n', language: 'python', isComplete: true } },
          { text: 'Some text' },
        ]);
      });

      it('should strip \\n\\n between text and code block', () => {
        const result = parseMarkdown('Some text\n\n```python\ncode\n```');
        expect(result).toEqual([
          { text: 'Some text' },
          { text: '', codeBlock: { code: 'code\n', language: 'python', isComplete: true } },
        ]);
      });
    });

    // 4. All three combined: text + header + code
    describe('Combined text, header, and code block', () => {
      it('should handle text → header → code with \\n\\n separators', () => {
        const result = parseMarkdown('Hello\n\n## Title\n\n```python\ncode\n```');
        expect(result).toEqual([
          { text: 'Hello' },
          { text: 'Title', header: 2 },
          { text: '', codeBlock: { code: 'code\n', language: 'python', isComplete: true } },
        ]);
      });

      it('should handle code → text → header with \\n\\n separators', () => {
        const result = parseMarkdown('```python\ncode\n```\n\nSome text\n\n## Title');
        expect(result).toEqual([
          { text: '', codeBlock: { code: 'code\n', language: 'python', isComplete: true } },
          { text: 'Some text' },
          { text: 'Title', header: 2 },
        ]);
      });

      it('should handle leading \\n\\n + text + header + code', () => {
        const result = parseMarkdown('\n\nHello\n\n## Title\n\n```python\ncode\n```');
        expect(result).toEqual([
          { text: 'Hello' },
          { text: 'Title', header: 2 },
          { text: '', codeBlock: { code: 'code\n', language: 'python', isComplete: true } },
        ]);
      });

      it('should handle LLM-style response with multiple sections', () => {
        const result = parseMarkdown(
          '\n\nHere are both algorithms:\n\n## Pancake Sort\n\n```python\ndef pancake_sort(arr):\n    pass\n```\n\n## Quick Sort\n\n```python\ndef quick_sort(arr):\n    pass\n```\n\nBoth are comparison sorts.'
        );
        expect(result).toEqual([
          { text: 'Here are both algorithms:' },
          { text: 'Pancake Sort', header: 2 },
          { text: '', codeBlock: { code: 'def pancake_sort(arr):\n    pass\n', language: 'python', isComplete: true } },
          { text: 'Quick Sort', header: 2 },
          { text: '', codeBlock: { code: 'def quick_sort(arr):\n    pass\n', language: 'python', isComplete: true } },
          { text: 'Both are comparison sorts.' },
        ]);
      });

      it('should handle code → header → list with \\n\\n separators', () => {
        const result = parseMarkdown('```js\ncode\n```\n\n## Notes\n\n- item1\n- item2');
        expect(result).toEqual([
          { text: '', codeBlock: { code: 'code\n', language: 'js', isComplete: true } },
          { text: 'Notes', header: 2 },
          { text: 'item1', listItem: { ordered: false, indent: 0 } },
          { text: 'item2', listItem: { ordered: false, indent: 0 } },
        ]);
      });
    });
  });
});

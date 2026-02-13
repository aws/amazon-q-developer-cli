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
      expect(parseMarkdown('**bold**')).toEqual([{ text: 'bold', bold: true }]);
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
        { text: '\n' },
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
        { text: ' text\n' },
        {
          text: '',
          codeBlock: { code: 'code\n', language: 'rust', isComplete: true },
        },
        { text: '\n' },
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
});

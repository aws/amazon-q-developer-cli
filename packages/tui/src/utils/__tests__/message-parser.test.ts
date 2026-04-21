import { describe, it, expect } from 'bun:test';
import {
  parseMarkdownChunk,
  parseMarkdownComplete,
  renderContentBlock,
  type ContentBlock,
} from '../message-parser';

describe('parseMarkdownChunk', () => {
  it('returns text blocks for plain text', () => {
    const blocks = parseMarkdownChunk('hello world');
    expect(blocks.length).toBe(1);
    expect(blocks[0]!.type).toBe('text');
    expect(blocks[0]!.content).toContain('hello world');
  });

  it('returns code block for line starting with ``` followed by language', () => {
    const blocks = parseMarkdownChunk('```python');
    expect(
      blocks.some((b) => b.type === 'code' && b.language === 'python')
    ).toBe(true);
  });

  it('returns text block for line that is just ```', () => {
    const blocks = parseMarkdownChunk('```');
    expect(blocks.some((b) => b.type === 'text')).toBe(true);
  });

  it('handles multiple lines', () => {
    const blocks = parseMarkdownChunk('line1\nline2\nline3');
    expect(blocks.length).toBe(3);
    for (const block of blocks) {
      expect(block.type).toBe('text');
    }
  });
});

describe('parseMarkdownComplete', () => {
  it('plain text returns single text block', () => {
    const blocks = parseMarkdownComplete('hello\nworld');
    expect(blocks.length).toBe(1);
    expect(blocks[0]!.type).toBe('text');
    expect(blocks[0]!.content).toBe('hello\nworld');
  });

  it('code block with language extracted', () => {
    const blocks = parseMarkdownComplete('```python\nprint("hi")\n```');
    expect(blocks.length).toBe(1);
    expect(blocks[0]!.type).toBe('code');
    expect(blocks[0]!.language).toBe('python');
    expect(blocks[0]!.content).toBe('print("hi")');
  });

  it('code block without language defaults to text', () => {
    const blocks = parseMarkdownComplete('```\nsome code\n```');
    expect(blocks.length).toBe(1);
    expect(blocks[0]!.type).toBe('code');
    expect(blocks[0]!.language).toBe('text');
    expect(blocks[0]!.content).toBe('some code');
  });

  it('Tool Call: line produces tool_call block', () => {
    const blocks = parseMarkdownComplete('Tool Call: read_file');
    expect(blocks.length).toBe(1);
    expect(blocks[0]!.type).toBe('tool_call');
    expect(blocks[0]!.content).toBe('Tool Call: read_file');
  });

  it('TOOL_CALL: line produces tool_call block', () => {
    const blocks = parseMarkdownComplete('TOOL_CALL: write_file');
    expect(blocks.length).toBe(1);
    expect(blocks[0]!.type).toBe('tool_call');
    expect(blocks[0]!.content).toBe('TOOL_CALL: write_file');
  });

  it('mixed content produces blocks in order', () => {
    const input = 'hello\n```python\ncode\n```\ngoodbye';
    const blocks = parseMarkdownComplete(input);
    expect(blocks.length).toBe(3);
    expect(blocks[0]!.type).toBe('text');
    expect(blocks[1]!.type).toBe('code');
    expect(blocks[2]!.type).toBe('text');
  });

  it('unclosed code block still produces code block', () => {
    const blocks = parseMarkdownComplete('```python\ncode here');
    expect(blocks.length).toBe(1);
    expect(blocks[0]!.type).toBe('code');
    expect(blocks[0]!.language).toBe('python');
    expect(blocks[0]!.content).toBe('code here');
  });

  it('consecutive text lines merged into single block', () => {
    const blocks = parseMarkdownComplete('line1\nline2\nline3');
    expect(blocks.length).toBe(1);
    expect(blocks[0]!.type).toBe('text');
    expect(blocks[0]!.content).toBe('line1\nline2\nline3');
  });

  it('multiple code blocks', () => {
    const input = '```js\nconst a = 1;\n```\n```py\nprint(1)\n```';
    const blocks = parseMarkdownComplete(input);
    const codeBlocks = blocks.filter((b) => b.type === 'code');
    expect(codeBlocks.length).toBe(2);
    expect(codeBlocks[0]!.language).toBe('js');
    expect(codeBlocks[1]!.language).toBe('py');
  });
});

describe('renderContentBlock', () => {
  it('text block returns content as-is', () => {
    const block: ContentBlock = { type: 'text', content: 'hello world' };
    expect(renderContentBlock(block)).toBe('hello world');
  });

  it('tool_call block returns string starting with wrench emoji', () => {
    const block: ContentBlock = { type: 'tool_call', content: 'read_file' };
    const result = renderContentBlock(block);
    expect(result).toContain('\u{1F527}');
    expect(result).toContain('read_file');
  });

  it('tool_output block returns string starting with outbox emoji', () => {
    const block: ContentBlock = {
      type: 'tool_output',
      content: 'file contents',
    };
    const result = renderContentBlock(block);
    expect(result).toContain('\u{1F4E4}');
    expect(result).toContain('file contents');
  });

  it('code block returns a string', () => {
    const block: ContentBlock = {
      type: 'code',
      content: 'const x = 1;',
      language: 'javascript',
    };
    const result = renderContentBlock(block);
    expect(typeof result).toBe('string');
    expect(result).toContain('x');
  });

  it('default/unknown type returns content', () => {
    const block = {
      type: 'unknown',
      content: 'something',
    } as unknown as ContentBlock;
    const result = renderContentBlock(block);
    expect(result).toBe('something');
  });
});

import { describe, it, expect } from 'bun:test';
import { serializeConversation } from '../serialize-conversation.js';
import { MessageRole } from '../../stores/app-store.js';

function user(content: string, id = crypto.randomUUID()) {
  return { id, role: MessageRole.User, content };
}

function model(content: string, id = crypto.randomUUID()) {
  return { id, role: MessageRole.Model, content };
}

function toolUse(name: string, content = '{}', id = crypto.randomUUID()) {
  return { id, role: MessageRole.ToolUse, name, content };
}

describe('serializeConversation plaintext', () => {
  it('uses User:/Kiro: labels instead of markdown headers', () => {
    const result = serializeConversation(
      [user('Hello'), model('Hi there!')],
      'plaintext'
    );
    expect(result).toBe('User:\n\nHello\n\nKiro:\n\nHi there!');
  });

  it('strips markdown bold and italic', () => {
    const result = serializeConversation(
      [model('This is **bold** and *italic* text')],
      'plaintext'
    );
    expect(result).toContain('This is bold and italic text');
  });

  it('strips markdown headings from content', () => {
    const result = serializeConversation(
      [model('## Section\n\nSome content')],
      'plaintext'
    );
    expect(result).toContain('Section');
    expect(result).toContain('Some content');
    expect(result).not.toContain('##');
  });

  it('strips code fence markers but keeps code content', () => {
    const result = serializeConversation(
      [model('```typescript\nconst x = 1;\n```')],
      'plaintext'
    );
    expect(result).toContain('const x = 1;');
    expect(result).not.toContain('```');
  });

  it('strips link syntax keeping text', () => {
    const result = serializeConversation(
      [model('See [the docs](https://example.com) for details')],
      'plaintext'
    );
    expect(result).toContain('See the docs for details');
  });

  it('merges consecutive model messages', () => {
    const result = serializeConversation(
      [
        user('Do something'),
        model('Let me check.'),
        toolUse('grep'),
        model('Found it.'),
      ],
      'plaintext'
    );
    expect(result).toBe(
      'User:\n\nDo something\n\nKiro:\n\nLet me check.\n\nFound it.'
    );
  });

  it('returns empty string for no messages', () => {
    expect(serializeConversation([], 'plaintext')).toBe('');
  });

  it('strips image syntax keeping alt text', () => {
    const result = serializeConversation(
      [model('Here: ![screenshot](./img.png)')],
      'plaintext'
    );
    expect(result).toContain('screenshot');
    expect(result).not.toContain('![');
  });

  it('preserves snake_case identifiers', () => {
    const result = serializeConversation(
      [model('Use the `my_function_name` to call it')],
      'plaintext'
    );
    expect(result).toContain('my_function_name');
  });
});

describe('serializeConversation json', () => {
  it('produces valid JSON with role and content', () => {
    const result = serializeConversation([user('Hello'), model('Hi!')], 'json');
    const parsed = JSON.parse(result);
    expect(parsed).toEqual([
      { role: MessageRole.User, content: 'Hello' },
      { role: MessageRole.Model, content: 'Hi!' },
    ]);
  });

  it('skips tool use messages', () => {
    const result = serializeConversation(
      [user('Do it'), toolUse('grep'), model('Done')],
      'json'
    );
    const parsed = JSON.parse(result);
    expect(parsed).toHaveLength(2);
  });
});

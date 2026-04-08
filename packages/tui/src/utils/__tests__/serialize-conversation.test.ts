import { describe, it, expect } from 'bun:test';
import { serializeConversationToMarkdown } from '../serialize-conversation.js';
import { MessageRole } from '../../stores/app-store.js';
import type { MessageType } from '../../stores/app-store.js';

function user(content: string, id = crypto.randomUUID()): MessageType {
  return { id, role: MessageRole.User, content };
}

function model(content: string, id = crypto.randomUUID()): MessageType {
  return { id, role: MessageRole.Model, content };
}

function toolUse(
  name: string,
  content = '{}',
  id = crypto.randomUUID()
): MessageType {
  return { id, role: MessageRole.ToolUse, name, content };
}

describe('serializeConversationToMarkdown', () => {
  it('serializes a single user→model turn', () => {
    const result = serializeConversationToMarkdown([
      user('Hello'),
      model('Hi there!'),
    ]);
    expect(result).toBe('## User\n\nHello\n\n## Kiro\n\nHi there!');
  });

  it('serializes multiple turns', () => {
    const result = serializeConversationToMarkdown([
      user('First question'),
      model('First answer'),
      user('Second question'),
      model('Second answer'),
    ]);
    expect(result).toBe(
      '## User\n\nFirst question\n\n## Kiro\n\nFirst answer\n\n## User\n\nSecond question\n\n## Kiro\n\nSecond answer'
    );
  });

  it('concatenates consecutive model messages split by tool calls', () => {
    const result = serializeConversationToMarkdown([
      user('Do something'),
      model('Let me check that.'),
      toolUse('grep'),
      model('Here are the results.'),
    ]);
    expect(result).toBe(
      '## User\n\nDo something\n\n## Kiro\n\nLet me check that.\n\nHere are the results.'
    );
  });

  it('skips tool use messages', () => {
    const result = serializeConversationToMarkdown([
      user('Search for foo'),
      toolUse('grep', '{"pattern":"foo"}'),
      model('Found it.'),
    ]);
    expect(result).toBe(
      '## User\n\nSearch for foo\n\n## Kiro\n\nFound it.'
    );
  });

  it('skips model messages with empty content', () => {
    const result = serializeConversationToMarkdown([
      user('Hi'),
      model(''),
      model('Real response'),
    ]);
    expect(result).toBe('## User\n\nHi\n\n## Kiro\n\nReal response');
  });

  it('returns empty string for no messages', () => {
    expect(serializeConversationToMarkdown([])).toBe('');
  });

  it('preserves raw markdown in model content', () => {
    const md = '```rust\nfn main() {}\n```\n\n- item 1\n- item 2';
    const result = serializeConversationToMarkdown([user('Show code'), model(md)]);
    expect(result).toBe(`## User\n\nShow code\n\n## Kiro\n\n${md}`);
  });

  it('handles model-only messages (e.g. welcome message)', () => {
    const result = serializeConversationToMarkdown([
      model('Welcome to Kiro!'),
      user('Hi'),
      model('Hello!'),
    ]);
    expect(result).toBe(
      '## Kiro\n\nWelcome to Kiro!\n\n## User\n\nHi\n\n## Kiro\n\nHello!'
    );
  });
});

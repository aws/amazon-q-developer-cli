import { describe, expect, it } from 'bun:test';
import { extractResultBodyText } from '../tool-result.js';
import type { ToolResult } from '../../stores/app-store.js';

const ok = (output: unknown): ToolResult =>
  ({ status: 'success', output }) as ToolResult;

describe('extractResultBodyText', () => {
  it.each<[string, ToolResult | undefined, string | null]>([
    ['non-success result', { status: 'error', error: 'x' } as ToolResult, null],
    ['undefined result', undefined, null],
    ['plain string output', ok('hello'), 'hello'],
    [
      'concatenated items (multi-file read), not just the first',
      ok({ items: [{ Text: 'file A' }, { Text: 'file B' }] }),
      'file A\nfile B',
    ],
    [
      'ACP {content:[{text}]} envelope',
      ok({ content: [{ text: 'acp line 1' }, { text: 'acp line 2' }] }),
      'acp line 1\nacp line 2',
    ],
    ['KAS {message} envelope', ok({ message: 'kas body' }), 'kas body'],
    ['nested Json items', ok({ items: [{ Json: { text: 'j' } }] }), 'j'],
    ['non-textual Json payload', ok({ items: [{ Json: { count: 3 } }] }), null],
    ['empty output object', ok({}), null],
  ])('reads %s', (_label, input, expected) => {
    expect(extractResultBodyText(input)).toBe(expected as never);
  });
});

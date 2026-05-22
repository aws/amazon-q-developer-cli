import { describe, expect, it } from 'bun:test';
import { formatToolParams } from '../tool-params.js';

describe('formatToolParams', () => {
  it('returns null for undefined content', () => {
    expect(formatToolParams(undefined)).toBeNull();
  });

  it('returns null for invalid JSON', () => {
    expect(formatToolParams('not json')).toBeNull();
  });

  it('shows basic params', () => {
    const content = JSON.stringify({ path: '/tmp', recursive: true });
    expect(formatToolParams(content)).toEqual(['path=/tmp', 'recursive=true']);
  });

  it('shows all params without a limit', () => {
    const content = JSON.stringify({
      a: '1',
      b: '2',
      c: '3',
      d: '4',
      e: '5',
      toolPath: 'important/path',
    });
    const result = formatToolParams(content);
    expect(result).toHaveLength(6);
  });

  it('excludes BASE_EXCLUDED fields', () => {
    const content = JSON.stringify({
      __tool_use_purpose: 'test',
      content: 'big blob',
      path: '/tmp',
    });
    expect(formatToolParams(content)).toEqual(['path=/tmp']);
  });

  it('respects custom exclude list', () => {
    const content = JSON.stringify({ path: '/tmp', command: 'ls' });
    expect(formatToolParams(content, ['path'])).toEqual(['command=ls']);
  });
});

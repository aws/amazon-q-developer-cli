import { afterEach, describe, expect, it } from 'bun:test';
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

  // The snake_case write payloads (old_str/new_str/file_text) are only hidden
  // in-cohort (they render as the diff there). Off-cohort they must still show
  // as args — mainline never excluded them.
  describe('snake_case write payloads are rollout-gated', () => {
    const content = JSON.stringify({
      path: '/tmp/x.ts',
      old_str: 'before',
      new_str: 'after',
    });
    const orig = process.env.KIRO_LITE_ROLLOUT_ENABLED;
    afterEach(() => {
      if (orig === undefined) delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
      else process.env.KIRO_LITE_ROLLOUT_ENABLED = orig;
    });

    it('in-cohort hides old_str/new_str', () => {
      process.env.KIRO_LITE_ROLLOUT_ENABLED = '1';
      expect(formatToolParams(content)).toEqual(['path=/tmp/x.ts']);
    });

    it('off-cohort keeps old_str/new_str (mainline parity)', () => {
      delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
      expect(formatToolParams(content)).toEqual([
        'path=/tmp/x.ts',
        'old_str=before',
        'new_str=after',
      ]);
    });
  });
});

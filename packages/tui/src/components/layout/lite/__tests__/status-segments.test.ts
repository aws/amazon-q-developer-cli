import { describe, test, expect } from 'vitest';
import { packStatusSegments } from '../status-segments.js';

const SEP = ' · ';

describe('packStatusSegments', () => {
  test('keeps everything on one line when it fits', () => {
    const out = packStatusSegments(['kiro', 'sonnet', 'high'], 80, SEP);
    expect(out).toEqual(['kiro · sonnet · high']);
  });

  test('skips empty/falsey segments without emitting separators for them', () => {
    const out = packStatusSegments(['kiro', '', 'high'], 80, SEP);
    expect(out).toEqual(['kiro · high']);
  });

  test('wraps onto a new line when the next segment would overflow', () => {
    // 'kiro'(4) + ' · '(3) + 'sonnet'(6) = 13 fits in 13.
    // adding ' · '(3) + 'branch'(6) = 22 > 13 → branch wraps.
    const out = packStatusSegments(['kiro', 'sonnet', 'branch'], 13, SEP);
    expect(out).toEqual(['kiro · sonnet', 'branch']);
  });

  test('packs greedily — fills each line before starting the next', () => {
    // width 20: 'kiro · sonnet'(13) +' · '+'high'(4)=20 fits;
    // +' · '+'ctx'(3) would be 26 > 20 → ctx starts line 2 with branch.
    const out = packStatusSegments(
      ['kiro', 'sonnet', 'high', 'ctx', 'br'],
      20,
      SEP
    );
    expect(out).toEqual(['kiro · sonnet · high', 'ctx · br']);
  });

  test('a single segment wider than maxCols gets its own line (soft-wrap left to terminal)', () => {
    const out = packStatusSegments(
      ['kiro', 'a-very-long-branch-name-that-overflows'],
      10,
      SEP
    );
    expect(out).toEqual(['kiro', 'a-very-long-branch-name-that-overflows']);
  });

  test('returns empty array when there are no non-empty segments', () => {
    expect(packStatusSegments(['', ''], 80, SEP)).toEqual([]);
    expect(packStatusSegments([], 80, SEP)).toEqual([]);
  });

  test('measures visible width ignoring ANSI escape codes', () => {
    // chalk-style SGR wrappers must not count toward width. The cyan 'kiro'
    // has a visible width of 4, so it still fits with 'sonnet' under 13 cols.
    const cyan = (s: string) => `\x1b[36m${s}\x1b[39m`;
    const out = packStatusSegments([cyan('kiro'), 'sonnet'], 13, SEP);
    expect(out).toEqual([`${cyan('kiro')} · sonnet`]);
  });
});

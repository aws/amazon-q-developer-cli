import { describe, test, expect } from 'vitest';
import chalk from 'chalk';
import { renderAgentMessage } from '../render.js';
import stripAnsi from 'strip-ansi';

// Force chalk colors for consistent test output
chalk.level = 3;

// Asserts the regression we fixed: no lone (unpaired) UTF-16 surrogate leaks
// through a wrap boundary, which would render as a garbled replacement char.
function assertNoLoneSurrogates(s: string): void {
  for (let k = 0; k < s.length; k++) {
    const code = s.charCodeAt(k);
    const isHigh = code >= 0xd800 && code <= 0xdbff;
    const isLow = code >= 0xdc00 && code <= 0xdfff;
    if (isHigh) {
      const next = s.charCodeAt(k + 1);
      expect(next >= 0xdc00 && next <= 0xdfff).toBe(true);
      k++; // skip the low surrogate we just validated
    } else if (isLow) {
      throw new Error(`lone low surrogate at index ${k}`);
    }
  }
}

// Surrogate-pair-aware wrap: emoji at U+1F000+ are two UTF-16 code units, but
// must stay paired across wrap boundaries. The previous code-unit iteration in
// wrapAnsiLine / formatBarBlock / wrapAtWords could split an emoji, leaving a
// lone high or low surrogate on row N and the other half on row N+1 — both
// halves render as garbled replacement chars. The fix iterates by code point.
describe('astral-codepoint wrap safety', () => {
  test('wrapAnsiLine preserves astral codepoints across wrap boundary', () => {
    // Build a line where the wrap point lands at an emoji. Each emoji has
    // visibleWidth == 2, so 4 emoji fit in 8 columns and the 5th wraps.
    // Render through renderAgentMessage at a narrow termCols to exercise
    // wrapStyled → wrapAnsiLine.
    const emoji = '\u{1F389}'; // 🎉, U+1F389 — astral, 2 UTF-16 code units
    const line = emoji.repeat(20);
    const out = renderAgentMessage(line, undefined, undefined, 14);
    const stripped = stripAnsi(out);
    // Each emoji must appear intact — never as a lone surrogate.
    assertNoLoneSurrogates(stripped);
    // And every row in the output must start with an emoji boundary, never
    // mid-codepoint. Splitting on '\n' and re-checking the first char.
    for (const row of stripped.split('\n')) {
      if (row.length === 0) continue;
      const first = row.charCodeAt(0);
      // A continuation row must not begin with a low surrogate.
      expect(first < 0xdc00 || first > 0xdfff).toBe(true);
    }
  });

  test('wrapAnsiLine handles a line of all-emoji at narrow width', () => {
    // Width accounting: with a column budget that's odd vs the emoji width
    // (2), the wrap math has to round down by emoji rather than splitting.
    const emoji = '\u{1F600}'; // 😀
    const line = emoji.repeat(10);
    const out = renderAgentMessage(line, undefined, undefined, 7);
    const stripped = stripAnsi(out);
    // Every emoji from the input must be present an integral number of times.
    const matches = stripped.match(/\p{Extended_Pictographic}/gu) ?? [];
    expect(matches.length).toBe(10);
  });

  test('wrapAtWords / formatBarBlock fenced-code path keeps astral chars whole', () => {
    // A fenced code block routes through formatBarBlock (the bar-prefixed
    // wrap). Force a long single-token line so the wrap logic runs.
    const emoji = '\u{1F4A1}'; // 💡
    const longRun = emoji.repeat(40);
    const md = '```\n' + longRun + '\n```';
    const out = renderAgentMessage(md, undefined, undefined, 30);
    const stripped = stripAnsi(out);
    // No lone surrogate in the output.
    assertNoLoneSurrogates(stripped);
    // All 40 emoji survive the wrap.
    const matches = stripped.match(/\p{Extended_Pictographic}/gu) ?? [];
    expect(matches.length).toBe(40);
  });
});

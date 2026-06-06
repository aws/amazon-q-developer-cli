import {
  describe,
  test,
  expect,
  beforeAll,
  afterAll,
} from 'vitest';
import chalk from 'chalk';
import { mkdtempSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import {
  renderAgentMessage,
} from '../render.js';
import stripAnsi from 'strip-ansi';

// Redirect KIRO_HOME so the verbose tests don't stomp on the developer's
// real ~/.kiro/settings/lite_verbose.json. The directory is removed
// after the suite finishes.
let tmpHome: string | undefined;
let originalKiroHome: string | undefined;
beforeAll(() => {
  originalKiroHome = process.env.KIRO_HOME;
  tmpHome = mkdtempSync(join(tmpdir(), 'kiro-verbose-test-'));
  process.env.KIRO_HOME = tmpHome;
});
afterAll(() => {
  if (originalKiroHome === undefined) {
    delete process.env.KIRO_HOME;
  } else {
    process.env.KIRO_HOME = originalKiroHome;
  }
  if (tmpHome) {
    try {
      rmSync(tmpHome, { recursive: true, force: true });
    } catch {
      // best-effort cleanup
    }
  }
});

// Force chalk colors for consistent test output
chalk.level = 3;


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
    // A lone high surrogate in JS toString is U+D83C (which is "\uD83C"),
    // but stripAnsi keeps it; assert no lone surrogates leak through.
    for (let k = 0; k < stripped.length; k++) {
      const code = stripped.charCodeAt(k);
      const isHigh = code >= 0xd800 && code <= 0xdbff;
      const isLow = code >= 0xdc00 && code <= 0xdfff;
      if (isHigh) {
        const next = stripped.charCodeAt(k + 1);
        expect(next >= 0xdc00 && next <= 0xdfff).toBe(true);
        k++; // skip the low surrogate we just validated
      } else if (isLow) {
        // a low surrogate without a preceding high is the bug we fixed
        throw new Error(`lone low surrogate at index ${k}`);
      }
    }
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
    for (let k = 0; k < stripped.length; k++) {
      const code = stripped.charCodeAt(k);
      const isLow = code >= 0xdc00 && code <= 0xdfff;
      const isHigh = code >= 0xd800 && code <= 0xdbff;
      if (isHigh) {
        const next = stripped.charCodeAt(k + 1);
        expect(next >= 0xdc00 && next <= 0xdfff).toBe(true);
        k++;
      } else if (isLow) {
        throw new Error(`lone low surrogate in fenced output at index ${k}`);
      }
    }
    // All 40 emoji survive the wrap.
    const matches = stripped.match(/\p{Extended_Pictographic}/gu) ?? [];
    expect(matches.length).toBe(40);
  });
});

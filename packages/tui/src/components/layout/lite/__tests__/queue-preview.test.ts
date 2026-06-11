import { describe, test, expect } from 'vitest';
import { previewLine } from '../queue-preview.js';
import { visibleWidth } from '../../../../utils/text-width.js';

describe('previewLine', () => {
  test('returns short messages unchanged when narrower than the cap', () => {
    expect(previewLine('hello', 80)).toBe('hello');
  });

  test('output width is bounded regardless of input size', () => {
    // Lock in the bug fix's load-bearing guarantee: a multi-KB queued
    // message must NOT produce a render whose Yoga measurement scales
    // with input length. visibleWidth captures the actual terminal-
    // column footprint, including the ellipsis that truncateToWidth
    // appends when it cuts.
    const huge = 'x'.repeat(50_000);
    const out = previewLine(huge, 80);
    expect(visibleWidth(out)).toBeLessThanOrEqual(80);
    // Sanity check: we didn't accidentally return the input verbatim.
    expect(out.length).toBeLessThan(huge.length);
  });

  test('output never contains embedded newlines', () => {
    // The render path treats embedded \n as hard line breaks and
    // emits one row per line, which is the same regression bare-text
    // rendering had. After previewLine the row count is always 1.
    const multi = 'first line\nsecond line\nthird line';
    const out = previewLine(multi, 80);
    expect(out).not.toContain('\n');
  });

  test('runs of whitespace collapse to a single space', () => {
    // Tabs, multiple spaces, and stacked newlines all flatten — the
    // preview is a one-line summary, not a faithful re-flow.
    expect(previewLine('a\n\n\t   b', 80)).toBe('a b');
  });

  test('leading and trailing whitespace are trimmed', () => {
    // The `.trim()` after collapse keeps the dim-styled `1.` index
    // sitting flush against the first real glyph of the message.
    expect(previewLine('   hello world   ', 80)).toBe('hello world');
  });

  test('floors at 8 cols when given an unusably narrow width', () => {
    // 0 / negative widths would otherwise make truncateToWidth return
    // just an ellipsis, which is a worse signal than a tiny preview.
    // Lock the floor so nobody's "narrow terminal" experience becomes
    // a mystery row of `…`.
    const out = previewLine('hello world', 0);
    expect(visibleWidth(out)).toBeLessThanOrEqual(8);
    expect(visibleWidth(out)).toBeGreaterThan(1);
  });

  test('huge input with embedded newlines stays bounded', () => {
    // Combined regression: multi-KB input *and* lots of newlines.
    // Without the pre-slice this would still walk every byte just to
    // collapse whitespace before truncating.
    const huge = 'paragraph line\n'.repeat(5_000);
    const out = previewLine(huge, 60);
    expect(visibleWidth(out)).toBeLessThanOrEqual(60);
    expect(out).not.toContain('\n');
  });
});

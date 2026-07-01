import { describe, it, expect } from 'bun:test';
import { formatKeyHints } from './KeyHints.js';
import { UNICODE_GLYPHS, ASCII_GLYPHS } from '../../../utils/glyphs.js';

describe('formatKeyHints', () => {
  const parts = [
    { keys: '↑↓', label: 'to navigate' },
    { keys: '↵', label: 'to select' },
  ];

  it('joins parts with the active small-dot separator (Unicode)', () => {
    expect(formatKeyHints(UNICODE_GLYPHS, parts)).toBe(
      '↑↓ to navigate · ↵ to select'
    );
  });

  it('joins parts with the active small-dot separator (ASCII)', () => {
    expect(formatKeyHints(ASCII_GLYPHS, parts)).toBe(
      '↑↓ to navigate . ↵ to select'
    );
  });

  it('uses the glyph set smallDot, padded with spaces, as the separator', () => {
    const out = formatKeyHints(ASCII_GLYPHS, parts);
    expect(out).toContain(` ${ASCII_GLYPHS.smallDot} `);
    // The Unicode dot must not leak in when ASCII glyphs are active.
    expect(out).not.toContain(UNICODE_GLYPHS.smallDot);
  });

  it('emits a single part with no separator', () => {
    expect(
      formatKeyHints(UNICODE_GLYPHS, [{ keys: 'esc', label: 'back' }])
    ).toBe('esc back');
  });

  it('returns an empty string for no parts', () => {
    expect(formatKeyHints(UNICODE_GLYPHS, [])).toBe('');
  });
});

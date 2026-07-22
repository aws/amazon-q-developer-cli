import { describe, it, expect } from 'bun:test';
import {
  UNICODE_GLYPHS,
  ASCII_GLYPHS,
  UNICODE_SPINNERS,
  ASCII_SPINNERS,
  UNICODE_BAR_RAMP,
  ASCII_BAR_RAMP,
  getBarRamp,
} from './glyphs.js';

describe('Glyph registry', () => {
  it('Unicode and ASCII glyph sets have the same keys', () => {
    const unicodeKeys = Object.keys(UNICODE_GLYPHS).sort();
    const asciiKeys = Object.keys(ASCII_GLYPHS).sort();
    expect(unicodeKeys).toEqual(asciiKeys);
  });

  it('all ASCII glyphs are single-width (length 1)', () => {
    // Some ASCII fallbacks are intentionally multi-char (arrows, tree
    // connectors, word-y key hints) since width fidelity matters less than
    // legibility in pure-ASCII terminals.
    const multiCharAllowed = new Set([
      'arrow',
      'treeCorner',
      'treeBranch',
      'ellipsis',
      'midEllipsis',
      'enter',
      'pause',
    ]);
    for (const [key, value] of Object.entries(ASCII_GLYPHS)) {
      if (multiCharAllowed.has(key)) continue;
      expect(value).toHaveLength(1);
    }
  });

  it('all Unicode glyphs are single-width characters (length 1 or known emoji)', () => {
    const multiCharAllowed = new Set([
      'clipboard', // 📋 emoji
      'sparkle', // ✨ emoji
      'treeCorner', // └──
      'treeBranch', // ├──
      'wrench', // 🔧 astral emoji (JS length 2)
      'cloud', // ☁️ emoji-presentation (base + VS16, JS length 2)
    ]);
    for (const [key, value] of Object.entries(UNICODE_GLYPHS)) {
      if (multiCharAllowed.has(key)) continue;
      expect(value).toHaveLength(1);
    }
  });

  it('extended vocabulary exists in both maps and degrades distinctly', () => {
    const extended = [
      'ellipsis',
      'midEllipsis',
      'arrowUp',
      'enter',
      'triangleLeft',
      'triangleRight',
      'loop',
      'times',
      'pause',
      'bar',
      'pencil',
      'wrench',
    ] as const;
    for (const key of extended) {
      expect(UNICODE_GLYPHS[key]).toBeTruthy();
      expect(ASCII_GLYPHS[key]).toBeTruthy();
      // ASCII fallback must be pure ASCII (no codepoint > 127)
      for (const ch of ASCII_GLYPHS[key]) {
        expect(ch.charCodeAt(0)).toBeLessThanOrEqual(127);
      }
      // Unicode and ASCII variants should differ
      expect(UNICODE_GLYPHS[key]).not.toBe(ASCII_GLYPHS[key]);
    }
  });

  it('Unicode and ASCII spinner sets have the same keys', () => {
    const unicodeKeys = Object.keys(UNICODE_SPINNERS).sort();
    const asciiKeys = Object.keys(ASCII_SPINNERS).sort();
    expect(unicodeKeys).toEqual(asciiKeys);
  });

  it('all spinner arrays have at least 2 frames', () => {
    for (const frames of Object.values(UNICODE_SPINNERS)) {
      expect(frames.length).toBeGreaterThanOrEqual(2);
    }
    for (const frames of Object.values(ASCII_SPINNERS)) {
      expect(frames.length).toBeGreaterThanOrEqual(2);
    }
  });

  it('ASCII spinner frames are all single-char', () => {
    for (const frames of Object.values(ASCII_SPINNERS)) {
      for (const frame of frames) {
        expect(frame).toHaveLength(1);
      }
    }
  });

  it('bar ramps are parallel and ASCII ramp is pure ASCII', () => {
    expect(UNICODE_BAR_RAMP).toHaveLength(ASCII_BAR_RAMP.length);
    for (const ch of ASCII_BAR_RAMP) {
      // each ramp cell is a single ASCII char
      expect(ch).toHaveLength(1);
      expect(ch.charCodeAt(0)).toBeLessThanOrEqual(127);
    }
  });

  it('getBarRamp selects Unicode when ASCII art is allowed, ASCII otherwise', () => {
    expect(getBarRamp(true)).toBe(UNICODE_BAR_RAMP);
    expect(getBarRamp(false)).toBe(ASCII_BAR_RAMP);
  });
});

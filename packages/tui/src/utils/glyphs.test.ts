import { describe, it, expect } from 'bun:test';
import {
  UNICODE_GLYPHS,
  ASCII_GLYPHS,
  UNICODE_SPINNERS,
  ASCII_SPINNERS,
} from './glyphs.js';

describe('Glyph registry', () => {
  it('Unicode and ASCII glyph sets have the same keys', () => {
    const unicodeKeys = Object.keys(UNICODE_GLYPHS).sort();
    const asciiKeys = Object.keys(ASCII_GLYPHS).sort();
    expect(unicodeKeys).toEqual(asciiKeys);
  });

  it('all ASCII glyphs are single-width (length 1)', () => {
    for (const [key, value] of Object.entries(ASCII_GLYPHS)) {
      // arrow and tree connectors are multi-char by design
      if (['arrow', 'treeCorner', 'treeBranch'].includes(key)) continue;
      expect(value).toHaveLength(1);
    }
  });

  it('all Unicode glyphs are single-width characters (length 1 or known emoji)', () => {
    const multiCharAllowed = new Set([
      'clipboard', // 📋 emoji
      'sparkle', // ✨ emoji
      'treeCorner', // └──
      'treeBranch', // ├──
    ]);
    for (const [key, value] of Object.entries(UNICODE_GLYPHS)) {
      if (multiCharAllowed.has(key)) continue;
      expect(value).toHaveLength(1);
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
});

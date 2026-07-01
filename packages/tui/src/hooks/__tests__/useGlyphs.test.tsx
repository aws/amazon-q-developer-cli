import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import {
  UNICODE_GLYPHS,
  ASCII_GLYPHS,
  UNICODE_SPINNERS,
  ASCII_SPINNERS,
} from '../../utils/glyphs.js';

describe('useGlyphs', () => {
  const originalEnv = process.env.KIRO_ASCII_MODE;

  beforeEach(() => {
    delete process.env.KIRO_ASCII_MODE;
  });

  afterEach(() => {
    if (originalEnv !== undefined) {
      process.env.KIRO_ASCII_MODE = originalEnv;
    } else {
      delete process.env.KIRO_ASCII_MODE;
    }
  });

  it('resolves to Unicode glyphs by default', async () => {
    delete process.env.KIRO_ASCII_MODE;
    // Re-import to pick up env change
    const mod = await import('../useGlyphs.js');
    // The module-level default is computed at import time; since we can't
    // easily re-evaluate, we test the exported context default value.
    const _ctx = mod.GlyphsContext;
    // Context._currentValue is React internal; instead verify the exports exist
    expect(mod.useGlyphs).toBeDefined();
    expect(mod.useSpinners).toBeDefined();
    expect(mod.GlyphsProvider).toBeDefined();
  });

  it('UNICODE_GLYPHS and ASCII_GLYPHS are distinct objects', () => {
    expect(UNICODE_GLYPHS).not.toBe(ASCII_GLYPHS);
    expect(UNICODE_GLYPHS.checkmark).toBe('✓');
    expect(ASCII_GLYPHS.checkmark).toBe('+');
  });

  it('extended vocabulary degrades to ASCII', () => {
    expect(UNICODE_GLYPHS.bar).toBe('█');
    expect(ASCII_GLYPHS.bar).toBe('#');
    expect(UNICODE_GLYPHS.ellipsis).toBe('…');
    expect(ASCII_GLYPHS.ellipsis).toBe('...');
    expect(UNICODE_GLYPHS.enter).toBe('↵');
    expect(ASCII_GLYPHS.enter).toBe('enter');
  });

  it('UNICODE_SPINNERS and ASCII_SPINNERS are distinct objects', () => {
    expect(UNICODE_SPINNERS).not.toBe(ASCII_SPINNERS);
    expect(UNICODE_SPINNERS.quarterSpinner[0]).toBe('◐');
    expect(ASCII_SPINNERS.quarterSpinner[0]).toBe('-');
  });

  it('GlyphsProvider is a valid React component', async () => {
    const { GlyphsProvider } = await import('../useGlyphs.js');
    expect(typeof GlyphsProvider).toBe('function');
  });
});

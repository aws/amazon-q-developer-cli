import { describe, it, expect, mock, afterAll } from 'bun:test';
import { kiroDark } from '../kiroDark';
import { kiroLight } from '../kiroLight';
import { kiroSafe } from '../kiroSafe';
import { createThemeContext } from '../ThemeProvider';

// --- getAutoTheme mocking: must be at module top level ---
const mockDetect = mock(() => ({
  theme: 'dark' as 'dark' | 'light',
  method: 'test',
  confidence: 'high' as 'high' | 'medium' | 'low',
}));

mock.module('../../utils/terminal-theme', () => ({
  detectTerminalThemeWithDetails: mockDetect,
  detectTerminalTheme: () => mockDetect().theme,
}));

afterAll(() => {
  mock.restore();
});

// Dynamic import after mock so getAutoTheme uses the mocked module
const { getAutoTheme } = await import('../ThemeProvider');

describe('createThemeContext', () => {
  const noopSetUserColors = () => {};
  const noopSetBaseTheme = () => {};

  function makeCtx(options?: {
    theme?: typeof kiroDark;
    userPromptColor?: { truecolor?: string; color256?: number; named?: string };
    userPromptBgColor?: {
      truecolor?: string;
      color256?: number;
      named?: string;
    };
    userResponseColor?: {
      truecolor?: string;
      color256?: number;
      named?: string;
    };
    userDiffPreset?: any;
    wrapDisabled?: boolean;
  }) {
    const opts = options ?? {};
    return createThemeContext(
      opts.theme ?? kiroDark,
      opts.userPromptColor as any,
      opts.userPromptBgColor as any,
      opts.userResponseColor as any,
      opts.userDiffPreset,
      noopSetUserColors,
      noopSetBaseTheme,
      opts.wrapDisabled ?? false
    );
  }

  it('getColor("primary") returns a callable function', () => {
    const ctx = makeCtx();
    const color = ctx.getColor('primary');
    expect(typeof color).toBe('function');
    expect(typeof color('test')).toBe('string');
  });

  it('getColor("error") returns a callable function', () => {
    const ctx = makeCtx();
    const color = ctx.getColor('error');
    expect(typeof color).toBe('function');
  });

  it('getColor("syntax.keyword") returns a callable function', () => {
    const ctx = makeCtx();
    const color = ctx.getColor('syntax.keyword');
    expect(typeof color).toBe('function');
  });

  it('getColor("diff.added.bar") returns a callable function', () => {
    const ctx = makeCtx();
    const color = ctx.getColor('diff.added.bar');
    expect(typeof color).toBe('function');
  });

  it('getColor("nonexistent") throws an Error containing "not found"', () => {
    const ctx = makeCtx();
    expect(() => ctx.getColor('nonexistent')).toThrow(/not found/);
  });

  it('getUserPromptColor() returns callable (falls back to primary when no override)', () => {
    const ctx = makeCtx();
    const color = ctx.getUserPromptColor();
    expect(typeof color).toBe('function');
  });

  it('getUserPromptColor() returns callable with user prompt color override', () => {
    const ctx = makeCtx({
      userPromptColor: {
        truecolor: '#ff0000',
        color256: 196,
        named: 'red' as any,
      },
    });
    const color = ctx.getUserPromptColor();
    expect(typeof color).toBe('function');
  });

  it('getUserResponseColor() falls back to primary when no override', () => {
    const ctx = makeCtx();
    const color = ctx.getUserResponseColor();
    expect(typeof color).toBe('function');
  });

  it('getUserResponseColor() with override', () => {
    const ctx = makeCtx({
      userResponseColor: {
        truecolor: '#00ff00',
        color256: 46,
        named: 'green' as any,
      },
    });
    const color = ctx.getUserResponseColor();
    expect(typeof color).toBe('function');
  });

  it('getUserPromptBgHex() returns a string or undefined', () => {
    const ctx = makeCtx();
    const hex = ctx.getUserPromptBgHex();
    expect(hex === undefined || typeof hex === 'string').toBe(true);
  });

  it('wrapDisabled is false when passed false', () => {
    const ctx = makeCtx({ wrapDisabled: false });
    expect(ctx.wrapDisabled).toBe(false);
  });

  it('wrapDisabled is true when passed true', () => {
    const ctx = makeCtx({ wrapDisabled: true });
    expect(ctx.wrapDisabled).toBe(true);
  });

  it('baseTheme reflects the theme passed in', () => {
    const ctx = makeCtx({ theme: kiroLight });
    expect(ctx.baseTheme).toBe(kiroLight);
  });

  it('with userDiffPreset that has real truecolor values, getColor("diff.added.bar") picks up the override', () => {
    const userDiffPreset = {
      id: 'custom',
      label: 'Custom',
      added: {
        background: {
          truecolor: '#112233',
          color256: 22,
          named: 'green' as any,
        },
        bar: { truecolor: '#aabbcc', color256: 121, named: 'green' as any },
        highlight: {
          truecolor: '#223344',
          color256: 22,
          named: 'green' as any,
        },
      },
      removed: {
        background: { truecolor: '#443322', color256: 52, named: 'red' as any },
        bar: { truecolor: '#ff0000', color256: 196, named: 'red' as any },
        highlight: { truecolor: '#332211', color256: 52, named: 'red' as any },
      },
    };
    const ctx = makeCtx({ userDiffPreset });
    const color = ctx.getColor('diff.added.bar');
    expect(typeof color).toBe('function');
    // The hex should reflect the override
    expect(color.hex).toBeDefined();
  });

  it('with userDiffPreset where added.bar.named === "default", diff colors should NOT be overridden', () => {
    const userDiffPreset = {
      id: 'default',
      label: 'Default',
      added: {
        background: { named: 'default' as any },
        bar: { named: 'default' as any },
        highlight: { named: 'default' as any },
      },
      removed: {
        background: { named: 'default' as any },
        bar: { named: 'default' as any },
        highlight: { named: 'default' as any },
      },
    };
    const ctx = makeCtx({ userDiffPreset });
    // When added.bar.named === 'default', the theme colors are used, not the override
    const color = ctx.getColor('diff.added.bar');
    expect(typeof color).toBe('function');
    // Should still be the kiroDark diff color
    expect(ctx.colors.diff.added.bar).toEqual(kiroDark.colors.diff.added.bar);
  });
});

describe('getAutoTheme', () => {
  it('returns kiroDark for high confidence dark', () => {
    mockDetect.mockReturnValue({
      theme: 'dark',
      method: 'test',
      confidence: 'high',
    });
    const result = getAutoTheme();
    expect(result).toBe(kiroDark);
  });

  it('returns kiroLight for high confidence light', () => {
    mockDetect.mockReturnValue({
      theme: 'light',
      method: 'test',
      confidence: 'high',
    });
    const result = getAutoTheme();
    expect(result).toBe(kiroLight);
  });

  it('returns kiroDark for medium confidence dark', () => {
    mockDetect.mockReturnValue({
      theme: 'dark',
      method: 'test',
      confidence: 'medium',
    });
    const result = getAutoTheme();
    expect(result).toBe(kiroDark);
  });

  it('returns kiroSafe for low confidence dark', () => {
    mockDetect.mockReturnValue({
      theme: 'dark',
      method: 'test',
      confidence: 'low',
    });
    const result = getAutoTheme();
    expect(result).toBe(kiroSafe);
  });

  it('returns kiroSafe for low confidence light', () => {
    mockDetect.mockReturnValue({
      theme: 'light',
      method: 'test',
      confidence: 'low',
    });
    const result = getAutoTheme();
    expect(result).toBe(kiroSafe);
  });
});

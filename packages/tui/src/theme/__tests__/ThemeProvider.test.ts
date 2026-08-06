import { describe, it, expect, mock, afterAll, afterEach } from 'bun:test';
import { kiroDark } from '../kiroDark';
import { kiroLight } from '../kiroLight';
import { kiroSafe } from '../kiroSafe';
import { createThemeContext } from '../ThemeProvider';
import type { ChalkColorName, TerminalColor } from '../../types/themeTypes';

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

  describe('KIRO_TERMINAL_THEME override', () => {
    const prev = process.env.KIRO_TERMINAL_THEME;
    afterEach(() => {
      if (prev === undefined) delete process.env.KIRO_TERMINAL_THEME;
      else process.env.KIRO_TERMINAL_THEME = prev;
    });

    it.each([
      ['dark', kiroDark],
      ['light', kiroLight],
      ['safe', kiroSafe],
      ['SAFE', kiroSafe],
    ] as const)('forces %s regardless of detection', (value, expected) => {
      process.env.KIRO_TERMINAL_THEME = value;
      // Detection says the opposite of the override to prove precedence.
      mockDetect.mockReturnValue({
        theme: 'light',
        method: 'test',
        confidence: 'high',
      });
      expect(getAutoTheme()).toBe(expected);
    });

    it('ignores unrecognized values and falls through to detection', () => {
      process.env.KIRO_TERMINAL_THEME = 'sparkly';
      mockDetect.mockReturnValue({
        theme: 'light',
        method: 'test',
        confidence: 'high',
      });
      expect(getAutoTheme()).toBe(kiroLight);
    });

    it('keeps forced-safe prompt chip colors explicit and adaptive', () => {
      process.env.KIRO_TERMINAL_THEME = 'safe';
      const chip = getAutoTheme().colors.components.promptChip;

      expect(chip).toEqual({
        background: { named: 'magentaBright' },
        text: { named: 'black' },
      });
      expect(chip.background.named).not.toBe('default');
      expect(chip.text.named).not.toBe('default');
    });
  });
});

type Rgb = [number, number, number];

const ansiNamedRgb: Record<Exclude<ChalkColorName, 'default'>, Rgb> = {
  black: [0, 0, 0],
  red: [255, 0, 0],
  green: [0, 255, 0],
  yellow: [255, 255, 0],
  blue: [0, 0, 255],
  magenta: [255, 0, 255],
  cyan: [0, 255, 255],
  white: [255, 255, 255],
  blackBright: [128, 128, 128],
  redBright: [255, 128, 128],
  greenBright: [128, 255, 128],
  yellowBright: [255, 255, 128],
  blueBright: [128, 128, 255],
  magentaBright: [255, 128, 255],
  cyanBright: [128, 255, 255],
  whiteBright: [255, 255, 255],
  gray: [128, 128, 128],
  grey: [128, 128, 128],
};

function hexToRgb(hex: string): Rgb {
  return [
    Number.parseInt(hex.slice(1, 3), 16),
    Number.parseInt(hex.slice(3, 5), 16),
    Number.parseInt(hex.slice(5, 7), 16),
  ];
}

function color256ToRgb(index: number): Rgb {
  if (index < 16) {
    const base: Rgb[] = [
      [0, 0, 0],
      [128, 0, 0],
      [0, 128, 0],
      [128, 128, 0],
      [0, 0, 128],
      [128, 0, 128],
      [0, 128, 128],
      [192, 192, 192],
      [128, 128, 128],
      [255, 0, 0],
      [0, 255, 0],
      [255, 255, 0],
      [0, 0, 255],
      [255, 0, 255],
      [0, 255, 255],
      [255, 255, 255],
    ];
    return base[index]!;
  }
  if (index < 232) {
    const level = [0, 95, 135, 175, 215, 255];
    const offset = index - 16;
    return [
      level[Math.floor(offset / 36)]!,
      level[Math.floor((offset % 36) / 6)]!,
      level[offset % 6]!,
    ];
  }
  const gray = 8 + (index - 232) * 10;
  return [gray, gray, gray];
}

function relativeLuminance(rgb: Rgb): number {
  const [red, green, blue] = rgb.map((channel) => {
    const value = channel / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * red! + 0.7152 * green! + 0.0722 * blue!;
}

function contrastRatio(foreground: Rgb, background: Rgb): number {
  const foregroundLuminance = relativeLuminance(foreground);
  const backgroundLuminance = relativeLuminance(background);
  return (
    (Math.max(foregroundLuminance, backgroundLuminance) + 0.05) /
    (Math.min(foregroundLuminance, backgroundLuminance) + 0.05)
  );
}

function namedToRgb(color: TerminalColor): Rgb {
  if (!color.named || color.named === 'default') {
    throw new Error('Prompt chip colors must not inherit terminal defaults');
  }
  return ansiNamedRgb[color.named];
}

describe('prompt chip contrast', () => {
  it.each([
    ['kiroDark', kiroDark],
    ['kiroLight', kiroLight],
    ['kiroSafe', kiroSafe],
  ] as const)('%s keeps its label at AA contrast', (_name, theme) => {
    const { background, text } = theme.colors.components.promptChip;
    const pairs: Array<[string, Rgb, Rgb]> = [];

    if (background.truecolor && text.truecolor) {
      pairs.push([
        'truecolor',
        hexToRgb(text.truecolor),
        hexToRgb(background.truecolor),
      ]);
    }
    if (background.color256 !== undefined && text.color256 !== undefined) {
      pairs.push([
        'color256',
        color256ToRgb(text.color256),
        color256ToRgb(background.color256),
      ]);
    }
    if (background.named && text.named) {
      pairs.push(['named', namedToRgb(text), namedToRgb(background)]);
    }

    expect(pairs.length).toBeGreaterThan(0);
    for (const [_mode, foreground, fill] of pairs) {
      expect(contrastRatio(foreground, fill)).toBeGreaterThanOrEqual(4.5);
    }
  });
});

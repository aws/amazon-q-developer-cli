import { describe, it, expect } from 'bun:test';
import { chalk } from '../color.js';
import {
  getTerminalChalkColor,
  getStatusColor,
  getColorHex,
} from '../colorUtils';

// Force truecolor so tests exercise real coloring even in CI (no TTY)
chalk.level = 3;

describe('getTerminalChalkColor', () => {
  // --- Basic return type contract ---

  it('always returns a callable function', () => {
    const cases = [
      {},
      { named: 'default' as const },
      { truecolor: '#ff0000' },
      { color256: 141 },
      { named: 'red' as const },
      { truecolor: '#ff0000', color256: 196, named: 'red' as const },
    ];
    for (const c of cases) {
      const color = getTerminalChalkColor(c);
      expect(typeof color).toBe('function');
      expect(typeof color('test')).toBe('string');
    }
  });

  it('always has a .hex string property', () => {
    const cases = [
      {},
      { named: 'default' as const },
      { truecolor: '#ff0000' },
      { color256: 141 },
      { named: 'red' as const },
    ];
    for (const c of cases) {
      expect(typeof getTerminalChalkColor(c).hex).toBe('string');
    }
  });

  // --- named: 'default' ---

  it('named "default" returns text unchanged', () => {
    const color = getTerminalChalkColor({ named: 'default' });
    expect(color('hello')).toBe('hello');
    expect(color.hex).toBe('inherit');
  });

  it('named "default" supports .bold chaining without reset codes', () => {
    const color = getTerminalChalkColor({ named: 'default' });
    expect(typeof color.bold).toBe('function');
    expect(typeof color.bold('test')).toBe('string');
    expect(color.bold('test')).not.toContain('\x1b[0m');
  });

  it('named "default" in bg mode also returns text unchanged', () => {
    const color = getTerminalChalkColor({ named: 'default' }, 'bg');
    expect(color('hello')).toBe('hello');
    expect(color.hex).toBe('inherit');
  });

  // --- Empty / no color values ---

  it('empty object returns fallback black hex', () => {
    const color = getTerminalChalkColor({});
    expect(color.hex).toBe('#000000');
  });

  // --- Foreground mode (default) ---

  it('truecolor produces correct hex', () => {
    const color = getTerminalChalkColor({ truecolor: '#ff0000' });
    expect(color.hex).toBe('#ff0000');
    expect(color('test')).toContain('test');
  });

  it('color256 produces a styled string', () => {
    const color = getTerminalChalkColor({ color256: 141 });
    expect(typeof color.hex).toBe('string');
    const output = color('test');
    expect(output).toContain('test');
    expect(output).not.toBe('test');
  });

  it('named color resolves to correct hex', () => {
    const cases: Array<{ named: any; expectedHex: string }> = [
      { named: 'red', expectedHex: '#ff0000' },
      { named: 'green', expectedHex: '#00ff00' },
      { named: 'blue', expectedHex: '#0000ff' },
      { named: 'magenta', expectedHex: '#ff00ff' },
      { named: 'cyan', expectedHex: '#00ffff' },
      { named: 'white', expectedHex: '#ffffff' },
      { named: 'black', expectedHex: '#000000' },
      { named: 'yellow', expectedHex: '#ffff00' },
      { named: 'gray', expectedHex: '#808080' },
      { named: 'grey', expectedHex: '#808080' },
    ];
    for (const { named, expectedHex } of cases) {
      expect(getTerminalChalkColor({ named }).hex).toBe(expectedHex);
    }
  });

  it('named bright colors resolve to correct hex', () => {
    const cases: Array<{ named: any; expectedHex: string }> = [
      { named: 'redBright', expectedHex: '#ff8080' },
      { named: 'greenBright', expectedHex: '#80ff80' },
      { named: 'blueBright', expectedHex: '#8080ff' },
      { named: 'magentaBright', expectedHex: '#ff80ff' },
      { named: 'cyanBright', expectedHex: '#80ffff' },
    ];
    for (const { named, expectedHex } of cases) {
      expect(getTerminalChalkColor({ named }).hex).toBe(expectedHex);
    }
  });

  // --- Background mode ---

  it('bg mode with truecolor produces different escape codes than fg', () => {
    const fg = getTerminalChalkColor({ truecolor: '#ff0000' }, 'fg');
    const bg = getTerminalChalkColor({ truecolor: '#ff0000' }, 'bg');
    {
      expect(fg('test')).not.toBe('test');
      expect(bg('test')).not.toBe('test');
      expect(fg('test')).not.toBe(bg('test'));
    }
  });

  it('bg mode with color256 produces different escape codes than fg', () => {
    const fg = getTerminalChalkColor({ color256: 196 }, 'fg');
    const bg = getTerminalChalkColor({ color256: 196 }, 'bg');
    {
      expect(fg('test')).not.toBe('test');
      expect(bg('test')).not.toBe('test');
      expect(fg('test')).not.toBe(bg('test'));
    }
  });

  it('bg mode with named color produces different escape codes than fg', () => {
    const fg = getTerminalChalkColor({ named: 'red' }, 'fg');
    const bg = getTerminalChalkColor({ named: 'red' }, 'bg');
    {
      expect(fg('test')).not.toBe('test');
      expect(bg('test')).not.toBe('test');
      expect(fg('test')).not.toBe(bg('test'));
    }
  });

  it('bg mode with empty object returns fallback', () => {
    const color = getTerminalChalkColor({}, 'bg');
    expect(color.hex).toBe('#000000');
  });

  // --- Fallback priority ---

  it('with all three fields set, output contains text', () => {
    const color = getTerminalChalkColor({
      truecolor: '#abcdef',
      color256: 100,
      named: 'red',
    });
    expect(color.hex).toBeTruthy();
    expect(color('test')).toContain('test');
  });

  it('truecolor + color256 (no named) produces styled output', () => {
    const color = getTerminalChalkColor({ truecolor: '#2d3a30', color256: 22 });
    expect(color('test')).toContain('test');
    expect(color('test')).not.toBe('test');
  });

  it('color256 + named (no truecolor) produces styled output', () => {
    const color = getTerminalChalkColor({ color256: 22, named: 'green' });
    expect(color('test')).toContain('test');
    expect(color('test')).not.toBe('test');
  });

  // --- Composability: fg + bg together ---

  it('fg and bg can be composed via nesting', () => {
    const fg = getTerminalChalkColor({ named: 'white' }, 'fg');
    const bg = getTerminalChalkColor({ named: 'red' }, 'bg');
    const result = bg(fg('hello'));
    expect(result).toContain('hello');
    expect(result).not.toBe('hello');
  });

  // --- Prototype chain (chalk method chaining) ---

  it('fg mode supports chalk chaining (.bold, .italic)', () => {
    const color = getTerminalChalkColor({ truecolor: '#ff0000' });
    expect(typeof color.bold).toBe('function');
    expect(typeof color.italic).toBe('function');
    expect(color.bold('test')).toContain('test');
  });

  it('bg mode supports chalk chaining', () => {
    const color = getTerminalChalkColor({ truecolor: '#ff0000' }, 'bg');
    expect(typeof color.bold).toBe('function');
    expect(color.bold('test')).toContain('test');
  });

  // --- Real theme color objects (integration-style) ---

  it('handles kiroDark diff added background', () => {
    const color = getTerminalChalkColor(
      { truecolor: '#2d3a30', color256: 22 },
      'bg'
    );
    expect(color('test')).toContain('test');
    expect(color('test')).not.toBe('test');
  });

  it('handles kiroDark diff added bar', () => {
    const color = getTerminalChalkColor(
      { truecolor: '#80ffb5', color256: 121 },
      'fg'
    );
    expect(color('+')).toContain('+');
    expect(color('+')).not.toBe('+');
  });

  it('handles kiroSafe named-only diff foreground', () => {
    const color = getTerminalChalkColor({ named: 'green' }, 'fg');
    expect(color('+')).toContain('+');
    expect(color('+')).not.toBe('+');
  });

  it('handles kiroSafe named "default" diff background as identity', () => {
    const color = getTerminalChalkColor({ named: 'default' }, 'bg');
    expect(color('test')).toBe('test');
  });

  // --- mode defaults to fg ---

  it('defaults to fg mode when mode is omitted', () => {
    const explicit = getTerminalChalkColor({ truecolor: '#ff0000' }, 'fg');
    const implicit = getTerminalChalkColor({ truecolor: '#ff0000' });
    expect(explicit('test')).toBe(implicit('test'));
  });
});

describe('getStatusColor', () => {
  const mockGetColor = (path: string) => {
    const colors: Record<string, any> = {
      brand: Object.assign((t: string) => t, { hex: '#8700FF' }),
      success: Object.assign((t: string) => t, { hex: '#00D787' }),
      info: Object.assign((t: string) => t, { hex: '#00FFFF' }),
      warning: Object.assign((t: string) => t, { hex: '#FFFF00' }),
      error: Object.assign((t: string) => t, { hex: '#FF0000' }),
      secondary: Object.assign((t: string) => t, { hex: '#808080' }),
    };
    return colors[path] ?? colors.brand;
  };

  it('maps active to brand', () => {
    expect(getStatusColor('active', mockGetColor).hex).toBe('#8700FF');
  });

  it('maps thinking to brand', () => {
    expect(getStatusColor('thinking', mockGetColor).hex).toBe('#8700FF');
  });

  it('maps success to success', () => {
    expect(getStatusColor('success', mockGetColor).hex).toBe('#00D787');
  });

  it('maps error to error', () => {
    expect(getStatusColor('error', mockGetColor).hex).toBe('#FF0000');
  });

  it('maps loading to secondary', () => {
    expect(getStatusColor('loading', mockGetColor).hex).toBe('#808080');
  });

  it('maps info to info', () => {
    expect(getStatusColor('info', mockGetColor).hex).toBe('#00FFFF');
  });

  it('maps warning to warning', () => {
    expect(getStatusColor('warning', mockGetColor).hex).toBe('#FFFF00');
  });

  it('maps executing to brand', () => {
    expect(getStatusColor('executing', mockGetColor).hex).toBe('#8700FF');
  });

  it('maps paused to secondary', () => {
    expect(getStatusColor('paused', mockGetColor).hex).toBe('#808080');
  });

  it('maps unknown status to brand (default case)', () => {
    expect(getStatusColor('unknown' as any, mockGetColor).hex).toBe('#8700FF');
  });
});

describe('getColorHex', () => {
  it('returns .hex from color function', () => {
    const colorFunc = Object.assign(() => '', { hex: '#ff0000' });
    expect(getColorHex(colorFunc)).toBe('#ff0000');
  });

  it('returns default fallback #ffffff when no .hex', () => {
    expect(getColorHex({})).toBe('#ffffff');
  });

  it('returns default fallback for null', () => {
    expect(getColorHex(null)).toBe('#ffffff');
  });

  it('returns default fallback for undefined', () => {
    expect(getColorHex(undefined)).toBe('#ffffff');
  });

  it('returns custom fallback', () => {
    expect(getColorHex(null, '#000000')).toBe('#000000');
  });
});

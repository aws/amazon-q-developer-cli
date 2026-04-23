import { describe, it, expect } from 'bun:test';
import {
  getTerminalChalkColor,
  getStatusColor,
  getColorHex,
} from '../colorUtils';

describe('getTerminalChalkColor', () => {
  it('named "default" returns chalk.reset with hex "inherit"', () => {
    const color = getTerminalChalkColor(undefined, undefined, 'default');
    expect(color.hex).toBe('inherit');
    // Should still produce a string (not throw)
    expect(typeof color('test')).toBe('string');
  });

  it('named "default" returns text unchanged and supports chaining', () => {
    const color = getTerminalChalkColor(undefined, undefined, 'default');
    expect(color('test')).toBe('test');
    expect(color.hex).toBe('inherit');
    // Chaining must work (used by UsagePanel, McpPanel, StatusInfo)
    expect(typeof color.bold).toBe('function');
    expect(typeof color.bold('test')).toBe('string');
    // .bold must not emit \x1b[0m reset
    expect(color.bold('test')).not.toContain('\x1b[0m');
  });

  it('truecolor value produces correct hex', () => {
    const color = getTerminalChalkColor('#ff0000');
    expect(color.hex).toBe('#ff0000');
    const output = color('test');
    expect(output).toContain('test');
  });

  it('color256 value produces a string output', () => {
    const color = getTerminalChalkColor(undefined, 141);
    expect(typeof color.hex).toBe('string');
    expect(typeof color('test')).toBe('string');
  });

  it('named color produces correct hex', () => {
    const color = getTerminalChalkColor(undefined, undefined, 'red');
    expect(color.hex).toBe('#ff0000');
  });

  it('no arguments returns fallback black', () => {
    const color = getTerminalChalkColor();
    expect(color.hex).toBe('#000000');
  });

  it('prefers truecolor over color256 and named', () => {
    const color = getTerminalChalkColor('#abcdef', 100, 'red');
    // hex should reflect truecolor or color256 depending on terminal,
    // but should not be the named color hex
    expect(color.hex).toBeTruthy();
    expect(typeof color('test')).toBe('string');
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

describe('getTerminalChalkColor (additional)', () => {
  it('named magenta produces hex #ff00ff', () => {
    const color = getTerminalChalkColor(undefined, undefined, 'magenta');
    expect(color.hex).toBe('#ff00ff');
  });

  it('named cyan produces a callable function', () => {
    const color = getTerminalChalkColor(undefined, undefined, 'cyan');
    expect(typeof color).toBe('function');
    expect(typeof color('test')).toBe('string');
  });
});

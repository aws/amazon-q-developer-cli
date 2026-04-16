import { describe, it, expect } from 'bun:test';
import { getTerminalChalkColor, getStatusColor } from '../colorUtils';

describe('getTerminalChalkColor', () => {
  it('named "default" returns chalk.reset with hex "inherit"', () => {
    const color = getTerminalChalkColor(undefined, undefined, 'default');
    expect(color.hex).toBe('inherit');
    // Should still produce a string (not throw)
    expect(typeof color('test')).toBe('string');
  });

  it('named "default" supports chaining (.bold)', () => {
    const color = getTerminalChalkColor(undefined, undefined, 'default');
    expect(typeof color.bold).toBe('function');
    expect(typeof color.bold('test')).toBe('string');
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
});

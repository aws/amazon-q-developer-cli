import { describe, it, expect } from 'bun:test';
import { visibleWidth, truncateToWidth, padToWidth } from '../text-width';

describe('visibleWidth', () => {
  it('returns length for ASCII string', () => {
    expect(visibleWidth('hello')).toBe(5);
  });

  it('returns 0 for empty string', () => {
    expect(visibleWidth('')).toBe(0);
  });

  it('emoji has width >= 1', () => {
    expect(visibleWidth('\u{1F600}')).toBeGreaterThanOrEqual(1);
  });
});

describe('truncateToWidth', () => {
  it('returns string unchanged if shorter than maxCols', () => {
    expect(truncateToWidth('hi', 10)).toBe('hi');
  });

  it('returns string unchanged if exactly at maxCols', () => {
    expect(truncateToWidth('hello', 5)).toBe('hello');
  });

  it('truncates long string with default ellipsis', () => {
    const result = truncateToWidth('hello world', 7);
    expect(result).toContain('\u2026'); // ellipsis char
    expect(visibleWidth(result)).toBeLessThanOrEqual(7);
  });

  it('truncates with custom ellipsis', () => {
    const result = truncateToWidth('hello world', 8, '...');
    expect(result).toContain('...');
    expect(visibleWidth(result)).toBeLessThanOrEqual(8);
  });

  it('maxCols = 0 returns ellipsis trimmed to fit', () => {
    const result = truncateToWidth('hello', 0);
    expect(result).toBe('');
  });

  it('empty string returned unchanged', () => {
    expect(truncateToWidth('', 10)).toBe('');
  });

  it('maxCols smaller than ellipsis width returns truncated ellipsis', () => {
    const result = truncateToWidth('hello world', 1, '...');
    expect(visibleWidth(result)).toBeLessThanOrEqual(1);
  });
});

describe('padToWidth', () => {
  it('pads short string with spaces to target width', () => {
    const result = padToWidth('hi', 10);
    expect(visibleWidth(result)).toBe(10);
    expect(result.startsWith('hi')).toBe(true);
  });

  it('returns string unchanged if at targetCols', () => {
    expect(padToWidth('hello', 5)).toBe('hello');
  });

  it('returns string unchanged if wider than targetCols', () => {
    expect(padToWidth('hello world', 5)).toBe('hello world');
  });

  it('pads empty string to targetCols spaces', () => {
    const result = padToWidth('', 5);
    expect(result).toBe('     ');
    expect(visibleWidth(result)).toBe(5);
  });
});

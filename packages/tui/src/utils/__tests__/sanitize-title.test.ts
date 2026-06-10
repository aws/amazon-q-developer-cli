import { describe, it, expect } from 'bun:test';
import { sanitizeSessionTitleForDisplay } from '../sanitize-title';

describe('sanitizeTitleForDisplay', () => {
  it('returns the input unchanged when it has no newlines', () => {
    expect(sanitizeSessionTitleForDisplay('fix the login bug')).toBe(
      'fix the login bug'
    );
  });

  it('replaces a single \\n with the literal two-char escape', () => {
    expect(sanitizeSessionTitleForDisplay('line one\nline two')).toBe(
      'line one\\nline two'
    );
  });

  it('replaces \\r\\n with one literal escape (Windows-style)', () => {
    expect(sanitizeSessionTitleForDisplay('line one\r\nline two')).toBe(
      'line one\\nline two'
    );
  });

  it('replaces a bare \\r with the literal escape', () => {
    expect(sanitizeSessionTitleForDisplay('foo\rbar')).toBe('foo\\nbar');
  });

  it('handles many embedded newlines', () => {
    expect(sanitizeSessionTitleForDisplay('a\nb\nc\nd')).toBe('a\\nb\\nc\\nd');
  });

  it('returns empty string for null', () => {
    expect(sanitizeSessionTitleForDisplay(null)).toBe('');
  });

  it('returns empty string for undefined', () => {
    expect(sanitizeSessionTitleForDisplay(undefined)).toBe('');
  });

  it('returns empty string for empty input', () => {
    expect(sanitizeSessionTitleForDisplay('')).toBe('');
  });

  it('never includes a real newline in the result', () => {
    const inputs = [
      'a\nb',
      '\n',
      '\r\n',
      '\r',
      'multi\nline\ntitle',
      'mixed\rsep\narator\r\nstuff',
    ];
    for (const input of inputs) {
      const out = sanitizeSessionTitleForDisplay(input);
      expect(out).not.toContain('\n');
      expect(out).not.toContain('\r');
    }
  });
});

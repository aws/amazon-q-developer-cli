import { describe, it, expect } from 'bun:test';
import {
  UNSUPPORTED_HIGHLIGHT_LANGUAGES,
  resolveHighlightLanguage,
} from '../highlight-languages';

describe('UNSUPPORTED_HIGHLIGHT_LANGUAGES', () => {
  it('is a Set', () => {
    expect(UNSUPPORTED_HIGHLIGHT_LANGUAGES).toBeInstanceOf(Set);
  });

  it('contains expected entries', () => {
    const expected = [
      'markdown',
      'md',
      'plaintext',
      'text',
      'txt',
      '',
      'console',
      'log',
      'none',
      'output',
      'plain',
    ];
    for (const lang of expected) {
      expect(UNSUPPORTED_HIGHLIGHT_LANGUAGES.has(lang)).toBe(true);
    }
  });
});

describe('resolveHighlightLanguage', () => {
  it('returns plaintext for undefined input', () => {
    expect(resolveHighlightLanguage(undefined)).toBe('plaintext');
  });

  it('returns plaintext for empty string', () => {
    expect(resolveHighlightLanguage('')).toBe('plaintext');
  });

  it('returns plaintext for unsupported languages', () => {
    const unsupported = [
      'markdown',
      'md',
      'text',
      'plaintext',
      'console',
      'log',
      'none',
      'output',
      'plain',
      'txt',
    ];
    for (const lang of unsupported) {
      expect(resolveHighlightLanguage(lang)).toBe('plaintext');
    }
  });

  it('passes through valid languages', () => {
    expect(resolveHighlightLanguage('typescript')).toBe('typescript');
    expect(resolveHighlightLanguage('python')).toBe('python');
    expect(resolveHighlightLanguage('rust')).toBe('rust');
    expect(resolveHighlightLanguage('javascript')).toBe('javascript');
    expect(resolveHighlightLanguage('c')).toBe('c');
    expect(resolveHighlightLanguage('go')).toBe('go');
  });

  it('strips backticks', () => {
    expect(resolveHighlightLanguage('`python`')).toBe('python');
  });

  it('trims whitespace', () => {
    expect(resolveHighlightLanguage('  python  ')).toBe('python');
  });

  it('returns plaintext for invalid characters', () => {
    expect(resolveHighlightLanguage('rm -rf /')).toBe('plaintext');
  });

  it('is case insensitive for unsupported languages', () => {
    expect(resolveHighlightLanguage('Markdown')).toBe('plaintext');
    expect(resolveHighlightLanguage('TEXT')).toBe('plaintext');
    expect(resolveHighlightLanguage('PLAINTEXT')).toBe('plaintext');
  });

  it('preserves case for valid languages', () => {
    expect(resolveHighlightLanguage('TypeScript')).toBe('TypeScript');
  });

  it('supports special chars in valid languages', () => {
    expect(resolveHighlightLanguage('c++')).toBe('c++');
    expect(resolveHighlightLanguage('c#')).toBe('c#');
    expect(resolveHighlightLanguage('f#')).toBe('f#');
  });

  it('returns plaintext for backtick-only string', () => {
    expect(resolveHighlightLanguage('```')).toBe('plaintext');
  });

  it('handles backtick-prefixed language from fence', () => {
    // When parsing ```rust from a mid-fence, backticks stripped -> 'rust'
    expect(resolveHighlightLanguage('```rust')).toBe('rust');
  });
});

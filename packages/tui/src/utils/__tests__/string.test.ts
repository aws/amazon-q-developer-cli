import { describe, it, expect } from 'bun:test';
import { expandTabs, normalizeLineEndings, unescapeShellPath } from '../string';

describe('expandTabs', () => {
  it('returns string unchanged when no tabs', () => {
    expect(expandTabs('hello world')).toBe('hello world');
  });

  it('expands tab at start of line (tabWidth=2)', () => {
    expect(expandTabs('\thi')).toBe('  hi');
  });

  it('expands tab respecting column position', () => {
    // 'a' is at col 0, tab at col 1 needs 1 space to reach col 2
    expect(expandTabs('a\tb')).toBe('a b');
  });

  it('expands multiple tabs', () => {
    // col 0: tab → 2 spaces, col 2: tab → 2 spaces
    expect(expandTabs('\t\thi')).toBe('    hi');
  });

  it('handles tab after even-width text', () => {
    // 'ab' is 2 chars, tab at col 2 → 2 spaces to reach col 4
    expect(expandTabs('ab\tc')).toBe('ab  c');
  });

  it('handles multiline strings', () => {
    expect(expandTabs('a\tb\n\tc')).toBe('a b\n  c');
  });

  it('respects custom tab width', () => {
    expect(expandTabs('\thi', 4)).toBe('    hi');
    expect(expandTabs('a\tb', 4)).toBe('a   b');
  });

  it('returns empty string unchanged', () => {
    expect(expandTabs('')).toBe('');
  });

  it('handles lines without tabs in multiline string', () => {
    expect(expandTabs('no tabs\n\thas tab')).toBe('no tabs\n  has tab');
  });
});

describe('normalizeLineEndings', () => {
  it('replaces tab with spaces so cursor math matches visual width', () => {
    // Tab is 1 char but string-width reports it as 0, causing cursor/wrap mismatch.
    // normalizeLineEndings should expand tabs so the segment value has no raw \t.
    expect(normalizeLineEndings('ISSUE-1234\t')).not.toContain('\t');
  });
});

describe('unescapeShellPath', () => {
  it('unescapes Finder-style shell-escaped paths', () => {
    // Spaces
    expect(unescapeShellPath('/Users/name/my\\ folder/file.txt')).toBe(
      '/Users/name/my folder/file.txt'
    );
    // Parentheses + spaces (common macOS download pattern)
    expect(unescapeShellPath('/path/to/file\\ \\(1\\).txt')).toBe(
      '/path/to/file (1).txt'
    );
    // Brackets, ampersand
    expect(unescapeShellPath('/path/to/A\\&B/file\\[1\\].txt')).toBe(
      '/path/to/A&B/file[1].txt'
    );
    // Home-relative
    expect(unescapeShellPath('~/my\\ folder/file.txt')).toBe(
      '~/my folder/file.txt'
    );
    // Surrounding single quotes (some terminals)
    expect(unescapeShellPath("'/Users/name/my folder/file.txt'")).toBe(
      '/Users/name/my folder/file.txt'
    );
    // Trailing whitespace
    expect(unescapeShellPath('  /Users/name/my\\ file.txt  ')).toBe(
      '/Users/name/my file.txt'
    );
  });

  it('does not modify non-path or ambiguous strings', () => {
    // Regular text
    expect(unescapeShellPath('hello world')).toBe('hello world');
    // Multi-line
    expect(unescapeShellPath('/path/to\\ file\nmore text')).toBe(
      '/path/to\\ file\nmore text'
    );
    // No escapes
    expect(unescapeShellPath('/simple/path/file.txt')).toBe(
      '/simple/path/file.txt'
    );
    // Windows path
    expect(unescapeShellPath('C:\\Users\\name\\file.txt')).toBe(
      'C:\\Users\\name\\file.txt'
    );
    // Empty
    expect(unescapeShellPath('')).toBe('');
  });

  it('rejects false positives (commands, regexes, unknown escapes)', () => {
    // Shell command with unescaped space after the escaped portion
    const cmd = '/usr/bin/grep -E foo\\ bar baz';
    expect(unescapeShellPath(cmd)).toBe(cmd);
    // Regex-like (forward-slash escapes aren't in the Finder set)
    const regex = '/path\\/to\\/something';
    expect(unescapeShellPath(regex)).toBe(regex);
    // Unknown escape char (\d)
    expect(unescapeShellPath('/some/path\\d+')).toBe('/some/path\\d+');
    // Multiple paths separated by unescaped space
    const multi = '/path/to/file\\ one.txt /path/to/file\\ two.txt';
    expect(unescapeShellPath(multi)).toBe(multi);
  });
});

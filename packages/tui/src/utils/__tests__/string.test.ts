import { describe, it, expect } from 'bun:test';
import {
  expandTabs,
  normalizeLineEndings,
  unescapeShellPath,
  isPrintable,
  shortenPath,
  stripNonPrintable,
} from '../string';

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

describe('isPrintable', () => {
  it('returns true for printable ASCII', () => {
    expect(isPrintable('hello world')).toBe(true);
    expect(isPrintable('ABC 123 !@#')).toBe(true);
  });

  it('returns true for empty string', () => {
    expect(isPrintable('')).toBe(true);
  });

  it('allows tab, newline, and CR', () => {
    expect(isPrintable('\t')).toBe(true);
    expect(isPrintable('\n')).toBe(true);
    expect(isPrintable('\r')).toBe(true);
    expect(isPrintable('hello\tworld\n')).toBe(true);
  });

  it('returns false for NUL char (0x00)', () => {
    expect(isPrintable('\x00')).toBe(false);
  });

  it('returns false for DEL (0x7F)', () => {
    expect(isPrintable('\x7F')).toBe(false);
  });

  it('returns false for C1 control chars (0x80-0x9F)', () => {
    expect(isPrintable('\x80')).toBe(false);
    expect(isPrintable('\x9F')).toBe(false);
    expect(isPrintable('\x85')).toBe(false);
  });

  it('returns true for chars >= 0xA0', () => {
    expect(isPrintable('\u00A0')).toBe(true); // non-breaking space
    expect(isPrintable('\u00FF')).toBe(true);
  });

  it('returns true for emoji', () => {
    expect(isPrintable('\u{1F600}')).toBe(true);
  });
});

describe('shortenPath', () => {
  const originalHome = process.env.HOME;

  it('replaces HOME prefix with ~', () => {
    process.env.HOME = '/Users/testuser';
    expect(shortenPath('/Users/testuser/documents/file.txt')).toBe(
      '~/documents/file.txt'
    );
    process.env.HOME = originalHome;
  });

  it('returns path unchanged if not starting with HOME', () => {
    process.env.HOME = '/Users/testuser';
    expect(shortenPath('/var/log/syslog')).toBe('/var/log/syslog');
    process.env.HOME = originalHome;
  });

  it('returns path unchanged if HOME is not set', () => {
    const origUserProfile = process.env.USERPROFILE;
    delete process.env.HOME;
    delete process.env.USERPROFILE;
    expect(shortenPath('/some/path/file.txt')).toBe('/some/path/file.txt');
    process.env.HOME = originalHome;
    if (origUserProfile !== undefined) {
      process.env.USERPROFILE = origUserProfile;
    }
  });

  it('returns ~ for exact HOME path', () => {
    process.env.HOME = '/Users/testuser';
    expect(shortenPath('/Users/testuser')).toBe('~');
    process.env.HOME = originalHome;
  });
});

describe('stripNonPrintable', () => {
  it('strips zero-width chars', () => {
    expect(stripNonPrintable('hello\u200Bworld')).toBe('helloworld');
    expect(stripNonPrintable('test\uFEFFvalue')).toBe('testvalue');
  });

  it('keeps normal text, tab, and newline', () => {
    expect(stripNonPrintable('hello\tworld\n')).toBe('hello\tworld\n');
  });

  it('strips C0 controls except tab and newline', () => {
    expect(stripNonPrintable('a\x01b\x02c')).toBe('abc');
    expect(stripNonPrintable('a\x0Bb')).toBe('ab'); // vertical tab
    expect(stripNonPrintable('a\x1Fb')).toBe('ab'); // unit separator
  });

  it('strips C1 control chars', () => {
    expect(stripNonPrintable('a\x7Fb')).toBe('ab'); // DEL
    expect(stripNonPrintable('a\x80b')).toBe('ab');
    expect(stripNonPrintable('a\x9Fb')).toBe('ab');
  });
});

describe('normalizeLineEndings (CRLF/CR conversion)', () => {
  it('converts CRLF to LF', () => {
    expect(normalizeLineEndings('hello\r\nworld')).toBe('hello\nworld');
  });

  it('converts lone CR to LF', () => {
    expect(normalizeLineEndings('hello\rworld')).toBe('hello\nworld');
  });

  it('handles mixed CRLF and CR', () => {
    expect(normalizeLineEndings('a\r\nb\rc')).toBe('a\nb\nc');
  });

  it('also expands tabs', () => {
    expect(normalizeLineEndings('a\r\n\tb')).toBe('a\n  b');
  });
});

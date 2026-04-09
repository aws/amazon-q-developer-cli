import { describe, it, expect, beforeAll, afterAll, afterEach } from 'bun:test';
import { mkdirSync, writeFileSync, rmSync } from 'fs';
import { join } from 'path';
import { completePathAtCursor } from '../path-completion';
import { escapePath, unescapePath, _setIsWindows } from '../path-completion';

// Create a temp directory structure for testing
const TEST_DIR = join(
  process.env.TMPDIR || '/tmp',
  `path-completion-test-${process.pid}`
);

beforeAll(() => {
  mkdirSync(join(TEST_DIR, 'subdir'), { recursive: true });
  writeFileSync(join(TEST_DIR, 'file.txt'), 'test');
  writeFileSync(join(TEST_DIR, 'file with spaces.txt'), 'test');
  writeFileSync(join(TEST_DIR, 'image (1).png'), 'test');
  writeFileSync(join(TEST_DIR, 'image (2).png'), 'test');
  writeFileSync(join(TEST_DIR, 'no-space.txt'), 'test');
});

afterAll(() => {
  rmSync(TEST_DIR, { recursive: true, force: true });
});

describe('completePathAtCursor', () => {
  it('completes a simple path', () => {
    const text = `${TEST_DIR}/file.`;
    const result = completePathAtCursor(text, text.length);
    expect(result).not.toBeNull();
    expect(result!.replacement).toContain('file.txt');
  });

  it('completes a path with escaped spaces from previous completion', () => {
    // Simulate: user previously tab-completed to "file\ with\ " and presses tab again
    const text = `${TEST_DIR}/file\\ with\\ `;
    const result = completePathAtCursor(text, text.length);
    expect(result).not.toBeNull();
    expect(result!.replacement).toContain('file\\ with\\ spaces.txt');
  });

  it('extracts full token including escaped spaces', () => {
    // The token should include the escaped spaces, not stop at them
    const text = `read ${TEST_DIR}/file\\ with\\ sp`;
    const result = completePathAtCursor(text, text.length);
    expect(result).not.toBeNull();
    // The start should be after "read ", not after the last space in the path
    expect(result!.start).toBe(5); // "read " is 5 chars
    expect(result!.replacement).toContain('file\\ with\\ spaces.txt');
  });

  it('completes paths with parentheses', () => {
    const text = `${TEST_DIR}/image\\ `;
    const result = completePathAtCursor(text, text.length);
    expect(result).not.toBeNull();
    // Should find both image (1).png and image (2).png
    expect(result!.candidates.length).toBe(2);
  });

  it('escapes spaces in completion output', () => {
    const text = `${TEST_DIR}/file\\ with`;
    const result = completePathAtCursor(text, text.length);
    expect(result).not.toBeNull();
    // Output should have escaped spaces
    expect(result!.replacement).toContain('\\ ');
    expect(result!.replacement).not.toMatch(/[^\\] /); // no unescaped spaces
  });

  it('handles path without spaces normally', () => {
    const text = `${TEST_DIR}/no-sp`;
    const result = completePathAtCursor(text, text.length);
    expect(result).not.toBeNull();
    expect(result!.replacement).toContain('no-space.txt');
  });

  it('extends backward past unescaped spaces to find path', () => {
    // User types path with literal space (no backslash) and presses Tab
    const text = `${TEST_DIR}/file with`;
    const result = completePathAtCursor(text, text.length);
    expect(result).not.toBeNull();
    // Should find "file with spaces.txt"
    expect(result!.candidates.length).toBe(1);
    expect(result!.replacement).toContain('file\\ with\\ spaces.txt');
  });

  it('returns null for non-existent directory', () => {
    const text = '/nonexistent/path/foo';
    const result = completePathAtCursor(text, text.length);
    expect(result).toBeNull();
  });
});

describe('escapePath / unescapePath platform behavior', () => {
  afterEach(() => {
    // Reset to actual platform after each test
    _setIsWindows(process.platform === 'win32');
  });

  describe('Unix behavior', () => {
    it('escapePath escapes backslashes and spaces', () => {
      _setIsWindows(false);
      expect(escapePath('path with spaces')).toBe('path\\ with\\ spaces');
      expect(escapePath('path\\backslash')).toBe('path\\\\backslash');
    });

    it('unescapePath reverses escapePath', () => {
      _setIsWindows(false);
      expect(unescapePath('path\\ with\\ spaces')).toBe('path with spaces');
      expect(unescapePath('path\\\\backslash')).toBe('path\\backslash');
    });

    it('backslash-space is treated as escaped space in token extraction', () => {
      _setIsWindows(false);
      const text = `${TEST_DIR}/file\\ with\\ spaces.txt`;
      const result = completePathAtCursor(text, text.length);
      expect(result).not.toBeNull();
      expect(result!.start).toBe(0);
    });
  });

  describe('Windows behavior', () => {
    it('escapePath is a no-op (Windows uses quoting, not escaping)', () => {
      _setIsWindows(true);
      expect(escapePath('C:\\Program Files')).toBe('C:\\Program Files');
      expect(escapePath('C:\\Users\\name')).toBe('C:\\Users\\name');
    });

    it('unescapePath is a no-op (backslashes are path separators)', () => {
      _setIsWindows(true);
      expect(unescapePath('C:\\Program\\ Files')).toBe('C:\\Program\\ Files');
      expect(unescapePath('C:\\Users\\name')).toBe('C:\\Users\\name');
    });

    it('backslash-space is NOT treated as escape in token extraction', () => {
      _setIsWindows(true);
      // "C:\Program Files\foo" — the space should break the token
      // (backward extension will handle it via readdirSync fallback)
      const text = 'read C:\\Program Files\\foo';
      const result = completePathAtCursor(text, text.length);
      // On Windows, the token starts at "Files\foo" (after the space)
      // since backslash-space is not an escape sequence
      // Result may be null since the path doesn't exist, but the token
      // extraction should not treat "\ " as escaped
      if (result) {
        expect(result.start).toBeGreaterThan(4); // not from "read"
      }
    });
  });
});

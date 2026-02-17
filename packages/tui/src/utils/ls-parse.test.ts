import { describe, it, expect } from 'bun:test';
import { parseLsEntries, getEntryName, resolveLsDisplayPath } from './ls-parse';

// Realistic ls long-format lines matching Rust Entry::to_long_format output
const FILE_LINE =
  '-rw-r--r-- 1 501 20 4096 Feb 17 21:01 /home/user/Documents/game.py';
const DIR_LINE =
  'drwxr-xr-x 5 501 20 160 Jan 28 09:30 /home/user/Documents/demo';
const SYMLINK_LINE =
  'lrwxr-xr-x 1 501 20 32 Mar 01 14:22 /home/user/Documents/link';

describe('parseLsEntries', () => {
  it('filters out User id: prefix line', () => {
    const text = `User id: 501\n${FILE_LINE}\n${DIR_LINE}`;
    const entries = parseLsEntries(text);
    expect(entries).toEqual([FILE_LINE, DIR_LINE]);
  });

  it('filters out Directory at truncation lines', () => {
    const text = [
      'User id: 501',
      'Directory at /home/user/project was truncated (has total 2714+ entries)',
      'Directory at /home/user/project/node_modules was truncated (has total 1500 entries)',
      FILE_LINE,
      DIR_LINE,
    ].join('\n');
    const entries = parseLsEntries(text);
    expect(entries).toEqual([FILE_LINE, DIR_LINE]);
  });

  it('filters out empty lines', () => {
    const text = `\n${FILE_LINE}\n\n${DIR_LINE}\n`;
    const entries = parseLsEntries(text);
    expect(entries).toEqual([FILE_LINE, DIR_LINE]);
  });

  it('returns empty array for empty string', () => {
    expect(parseLsEntries('')).toEqual([]);
  });

  it('returns empty array when only prefix lines exist', () => {
    const text =
      'User id: 501\nDirectory at /foo was truncated (has total 10 entries)';
    expect(parseLsEntries(text)).toEqual([]);
  });
});

describe('getEntryName', () => {
  it('extracts filename from standard file entry', () => {
    expect(getEntryName(FILE_LINE)).toBe('game.py');
  });

  it('extracts directory name from dir entry', () => {
    expect(getEntryName(DIR_LINE)).toBe('demo');
  });

  it('extracts symlink name', () => {
    expect(getEntryName(SYMLINK_LINE)).toBe('link');
  });

  it('handles path with spaces', () => {
    const line =
      '-rw-r--r-- 1 501 20 100 Feb 17 21:01 /home/user/My Documents/my file.txt';
    expect(getEntryName(line)).toBe('my file.txt');
  });

  it('handles root-level entry', () => {
    const line = 'drwxr-xr-x 5 0 0 160 Feb 17 21:01 /tmp';
    expect(getEntryName(line)).toBe('tmp');
  });

  it('does not include timestamp in name', () => {
    // This was the original bug — index 7 instead of 8 would return "21:01"
    const name = getEntryName(FILE_LINE);
    expect(name).not.toContain(':');
    expect(name).not.toMatch(/^\d{2}:\d{2}/);
  });

  it('falls back to last token for malformed lines', () => {
    expect(getEntryName('short line')).toBe('line');
  });

  it('returns full line for single-token input', () => {
    expect(getEntryName('something')).toBe('something');
  });
});

describe('resolveLsDisplayPath', () => {
  const entries = [FILE_LINE, DIR_LINE];

  it('returns absolute rawDirPath as-is', () => {
    expect(resolveLsDisplayPath('/home/user/Documents', entries)).toBe(
      '/home/user/Documents'
    );
  });

  it('resolves "." from first entry path', () => {
    expect(resolveLsDisplayPath('.', entries)).toBe('/home/user/Documents');
  });

  it('resolves relative path from first entry', () => {
    expect(resolveLsDisplayPath('Documents', entries)).toBe(
      '/home/user/Documents'
    );
  });

  it('resolves null rawDirPath from entries', () => {
    expect(resolveLsDisplayPath(null, entries)).toBe('/home/user/Documents');
  });

  it('does not include timestamp in resolved path', () => {
    const resolved = resolveLsDisplayPath('.', entries);
    expect(resolved).not.toContain('21:01');
  });

  it('falls back to rawDirPath when no entries', () => {
    expect(resolveLsDisplayPath('.', [])).toBe('.');
  });

  it('falls back to null when no entries and null rawDirPath', () => {
    expect(resolveLsDisplayPath(null, [])).toBeNull();
  });

  it('handles entries with paths containing spaces', () => {
    const spacedEntries = [
      '-rw-r--r-- 1 501 20 100 Feb 17 21:01 /home/user/My Documents/file.txt',
    ];
    expect(resolveLsDisplayPath('.', spacedEntries)).toBe(
      '/home/user/My Documents'
    );
  });

  it('handles root-level entries', () => {
    const rootEntries = ['drwxr-xr-x 5 0 0 160 Feb 17 21:01 /tmp'];
    // /tmp has no parent with lastSlash > 0 (slash is at index 0)
    // so it falls back to rawDirPath
    expect(resolveLsDisplayPath('.', rootEntries)).toBe('.');
  });
});

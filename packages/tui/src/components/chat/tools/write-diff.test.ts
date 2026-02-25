import { describe, it, expect } from 'bun:test';
import { diffLines } from 'diff';

/**
 * Tests for the diff normalization logic used in Write.tsx.
 *
 * The Write component normalizes trailing newlines before diffing to prevent
 * phantom "added 1 line" entries caused by mismatched trailing newlines
 * between oldStr and newStr.
 */

/** Mirrors the normalization + counting logic from Write.tsx */
function computeDiffCounts(oldText: string, newText: string) {
  const normalizedOld = (oldText || '').replace(/\r?\n$/, '');
  const normalizedNew = (newText || '').replace(/\r?\n$/, '');
  const changes = diffLines(normalizedOld, normalizedNew);

  let linesAdded = 0;
  let linesRemoved = 0;
  for (const change of changes) {
    const lines = change.value.split('\n');
    const count =
      lines[lines.length - 1] === '' ? lines.length - 1 : lines.length;
    if (change.added) linesAdded += count;
    else if (change.removed) linesRemoved += count;
  }
  return { linesAdded, linesRemoved };
}

describe('Write diff normalization', () => {
  it('pure deletion with trailing LF on oldStr shows no phantom added line', () => {
    const result = computeDiffCounts('line1\nline2\n', '');
    expect(result.linesAdded).toBe(0);
    expect(result.linesRemoved).toBe(2);
  });

  it('pure deletion with trailing CRLF on oldStr shows no phantom added line', () => {
    const result = computeDiffCounts('line1\r\nline2\r\n', '');
    expect(result.linesAdded).toBe(0);
    expect(result.linesRemoved).toBe(2);
  });

  it('deletion where newStr is just a newline shows no phantom added line', () => {
    const result = computeDiffCounts('line1\nline2\n', '\n');
    expect(result.linesAdded).toBe(0);
    expect(result.linesRemoved).toBe(2);
  });

  it('genuine replacement counts correctly', () => {
    const result = computeDiffCounts('line1\nline2\n', 'newline1\n');
    expect(result.linesAdded).toBe(1);
    expect(result.linesRemoved).toBe(2);
  });

  it('both sides with trailing newlines diffs correctly', () => {
    const result = computeDiffCounts('line1\nline2\n', 'line1\nnewline2\n');
    expect(result.linesAdded).toBe(1);
    expect(result.linesRemoved).toBe(1);
  });

  it('new file creation counts only added lines', () => {
    const result = computeDiffCounts('', 'hello\nworld\n');
    expect(result.linesAdded).toBe(2);
    expect(result.linesRemoved).toBe(0);
  });

  it('CRLF trailing newline on oldStr does not produce phantom diff', () => {
    // For StrReplace, both oldStr and newStr come from the LLM (both LF).
    // For Create, oldStr comes from read_to_string (could be CRLF on Windows).
    // This test covers the Create case where only the trailing newline differs.
    const result = computeDiffCounts('line1\nline2\r\n', 'line1\nline2\n');
    expect(result.linesAdded).toBe(0);
    expect(result.linesRemoved).toBe(0);
  });

  it('no changes when content is identical', () => {
    const result = computeDiffCounts('same\n', 'same\n');
    expect(result.linesAdded).toBe(0);
    expect(result.linesRemoved).toBe(0);
  });

  it('empty to empty produces no diff', () => {
    const result = computeDiffCounts('', '');
    expect(result.linesAdded).toBe(0);
    expect(result.linesRemoved).toBe(0);
  });
});

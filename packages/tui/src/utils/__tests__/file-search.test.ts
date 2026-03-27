import { describe, test, expect, beforeAll, afterAll } from 'bun:test';
import { mkdirSync, writeFileSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { searchFilesAbortable } from '../file-search';

const TEMP_DIR = join(tmpdir(), `.file-search-test-${process.pid}`);
const DIRS = 50;
const FILES_PER_DIR = 40; // 2000 files total
let origCwd: string;

beforeAll(() => {
  origCwd = process.cwd();

  mkdirSync(TEMP_DIR, { recursive: true });
  writeFileSync(join(TEMP_DIR, '.gitignore'), '');

  for (let d = 0; d < DIRS; d++) {
    const dirPath = join(TEMP_DIR, `dir-${String(d).padStart(3, '0')}`);
    mkdirSync(dirPath, { recursive: true });
    for (let f = 0; f < FILES_PER_DIR; f++) {
      writeFileSync(
        join(dirPath, `file-${String(f).padStart(3, '0')}.txt`),
        ''
      );
    }
  }
  writeFileSync(join(TEMP_DIR, 'unique-target.ts'), '');
  writeFileSync(join(TEMP_DIR, 'dir-000', 'nested-target.ts'), '');

  process.chdir(TEMP_DIR);
});

afterAll(() => {
  process.chdir(origCwd);
  rmSync(TEMP_DIR, { recursive: true, force: true });
});

/** Helper: run a search with no abort */
function search(query: string, limit?: number) {
  return searchFilesAbortable(query, new AbortController().signal, limit);
}

describe('file search', () => {
  test('finds matching files', async () => {
    expect(await search('unique-target')).toContain('unique-target.ts');
  });

  test('finds nested files', async () => {
    expect(await search('nested-target')).toContain('dir-000/nested-target.ts');
  });

  test('returns empty for no match', async () => {
    expect(await search('zzz-nonexistent-zzz')).toEqual([]);
  });

  test('returns empty for empty query', async () => {
    expect(await search('')).toEqual([]);
  });

  test('respects limit', async () => {
    const results = await search('file-', 5);
    expect(results.length).toBeLessThanOrEqual(5);
  });

  test('does not block the main thread', async () => {
    const searchPromise = search('file-', 20);

    const blocked = await new Promise<boolean>((resolve) => {
      const start = performance.now();
      setTimeout(() => resolve(performance.now() - start > 500), 0);
    });
    expect(blocked).toBe(false);

    const results = await searchPromise;
    expect(results.length).toBeGreaterThan(0);
    expect(results.length).toBeLessThanOrEqual(20);
  });

  test('returns empty when pre-aborted', async () => {
    const ac = new AbortController();
    ac.abort();
    expect(await searchFilesAbortable('file-', ac.signal)).toEqual([]);
  });

  test('returns fewer results when aborted mid-walk', async () => {
    const full = await search('file-', 200);

    const signal = AbortSignal.timeout(0);
    const partial = await searchFilesAbortable('file-', signal, 200);

    expect(partial.length).toBeLessThan(full.length);
  });

  test('abort cancels walk — result count is less than max', async () => {
    const signal = AbortSignal.timeout(0);
    const results = await searchFilesAbortable('file-', signal, 200);
    expect(results.length).toBeLessThan(200);
  });
});

import { describe, test, expect, beforeEach, afterAll } from 'bun:test';
import { rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { CommandHistory } from './command-history';

const TEST_HISTORY_FILE = join(tmpdir(), `kiro-test-history-${process.pid}`);

describe('CommandHistory', () => {
  let history: CommandHistory;

  beforeEach(() => {
    history = CommandHistory.createWithFile(TEST_HISTORY_FILE);
    history.clear();
  });

  afterAll(() => {
    try {
      rmSync(TEST_HISTORY_FILE);
    } catch {
      /* ignore */
    }
  });

  test('add command to history', () => {
    history.add('test command');
    expect(history.getAll()).toEqual(['test command']);
  });

  test('skip empty commands', () => {
    history.add('');
    history.add('   ');
    expect(history.getAll()).toEqual([]);
  });

  test('navigate up through history', () => {
    history.add('first');
    history.add('second');
    history.add('third');

    expect(history.navigate('up')).toBe('third');
    expect(history.navigate('up')).toBe('second');
    expect(history.navigate('up')).toBe('first');
    expect(history.navigate('up')).toBe('first'); // Stay at oldest
  });

  test('navigate down through history', () => {
    history.add('first');
    history.add('second');
    history.add('third');

    history.navigate('up'); // third
    history.navigate('up'); // second

    expect(history.navigate('down')).toBe('third');
    expect(history.navigate('down')).toBe(''); // Restored to saved input
  });

  test('navigate down from current returns null', () => {
    history.add('test');
    expect(history.navigate('down')).toBeNull();
  });

  test('reset index after adding command', () => {
    history.add('first');
    history.add('second');

    history.navigate('up');
    history.add('third');

    expect(history.navigate('up')).toBe('third');
  });

  test('empty history returns null', () => {
    expect(history.navigate('up')).toBeNull();
    expect(history.navigate('down')).toBeNull();
  });

  test('multiline entry should survive save/load round-trip as single entry', () => {
    history.add('single line');
    history.add('line1\nline2\nline3');

    // In-memory: should be 2 entries
    expect(history.getAll()).toEqual(['single line', 'line1\nline2\nline3']);

    // Reload from disk into a fresh instance
    const h2 = CommandHistory.createWithFile(TEST_HISTORY_FILE);

    // Should preserve the multiline entry as a single item
    expect(h2.getAll()).toEqual(['single line', 'line1\nline2\nline3']);
  });
});

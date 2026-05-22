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

  test('switchToFile isolates history per file', () => {
    const fileA = join(tmpdir(), `kiro-test-history-a-${process.pid}`);
    const fileB = join(tmpdir(), `kiro-test-history-b-${process.pid}`);
    const h = CommandHistory.createWithFile(fileA);

    h.add('from-a');
    h.switchToFile(fileB);
    h.add('from-b');

    expect(h.getAll()).toEqual(['from-b']);

    // Switch back to A — should reload A's history
    h.switchToFile(fileA);
    expect(h.getAll()).toEqual(['from-a']);

    try {
      rmSync(fileA);
    } catch {
      /* ignore */
    }
    try {
      rmSync(fileB);
    } catch {
      /* ignore */
    }
  });

  test('setSessionId switches to session-specific file', () => {
    history.add('global-cmd');
    history.setSessionId('test-session-123');
    history.add('session-cmd');

    expect(history.getAll()).toContain('session-cmd');
  });

  test('switchToFile is no-op when path unchanged', () => {
    const file = join(tmpdir(), `kiro-test-noop-${process.pid}`);
    const h = CommandHistory.createWithFile(file);
    h.add('cmd1');
    h.switchToFile(file);
    expect(h.getAll()).toEqual(['cmd1']);
    try {
      rmSync(file);
    } catch {
      /* ignore */
    }
  });

  test('switchToFile to empty session does not inherit previous history', () => {
    const fileA = join(tmpdir(), `kiro-test-inherit-a-${process.pid}`);
    const fileB = join(tmpdir(), `kiro-test-inherit-b-${process.pid}`);
    const h = CommandHistory.createWithFile(fileA);

    h.add('global-cmd-1');
    h.add('global-cmd-2');

    // Switch to a new empty session — should NOT carry over previous history
    h.switchToFile(fileB);
    expect(h.getAll()).toEqual([]);
    expect(h.navigate('up')).toBeNull();

    try {
      rmSync(fileA);
    } catch {
      /* ignore */
    }
    try {
      rmSync(fileB);
    } catch {
      /* ignore */
    }
  });
});

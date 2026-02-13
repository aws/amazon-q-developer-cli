import { describe, test, expect, beforeEach } from 'bun:test';
import { CommandHistory } from './command-history';

describe('CommandHistory', () => {
  let history: CommandHistory;

  beforeEach(() => {
    history = CommandHistory.getInstance();
    history.clear();
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
    expect(history.navigate('down')).toBeNull(); // Return to current
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
});

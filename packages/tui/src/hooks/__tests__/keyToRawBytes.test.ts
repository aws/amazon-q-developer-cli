import { describe, it, expect } from 'bun:test';
import { keyToRawBytes, type Key } from '../useKeypress';

const baseKey: Key = {
  upArrow: false,
  downArrow: false,
  leftArrow: false,
  rightArrow: false,
  pageUp: false,
  pageDown: false,
  home: false,
  end: false,
  return: false,
  escape: false,
  ctrl: false,
  shift: false,
  meta: false,
  tab: false,
  backspace: false,
  delete: false,
};

function key(overrides: Partial<Key>): Key {
  return { ...baseKey, ...overrides };
}

describe('keyToRawBytes', () => {
  it('returns \\r for return key', () => {
    expect(keyToRawBytes(key({ return: true }), '')).toBe('\r');
  });

  it('returns \\x7f for backspace', () => {
    expect(keyToRawBytes(key({ backspace: true }), '')).toBe('\x7f');
  });

  it('returns \\t for tab', () => {
    expect(keyToRawBytes(key({ tab: true }), '')).toBe('\t');
  });

  it('returns \\x1b for escape', () => {
    expect(keyToRawBytes(key({ escape: true }), '')).toBe('\x1b');
  });

  it('returns arrow key escape sequences', () => {
    expect(keyToRawBytes(key({ upArrow: true }), '')).toBe('\x1b[A');
    expect(keyToRawBytes(key({ downArrow: true }), '')).toBe('\x1b[B');
    expect(keyToRawBytes(key({ rightArrow: true }), '')).toBe('\x1b[C');
    expect(keyToRawBytes(key({ leftArrow: true }), '')).toBe('\x1b[D');
  });

  it('returns delete escape sequence', () => {
    expect(keyToRawBytes(key({ delete: true }), '')).toBe('\x1b[3~');
  });

  it('returns home/end escape sequences', () => {
    expect(keyToRawBytes(key({ home: true }), '')).toBe('\x1b[H');
    expect(keyToRawBytes(key({ end: true }), '')).toBe('\x1b[F');
  });

  it('returns page up/down escape sequences', () => {
    expect(keyToRawBytes(key({ pageUp: true }), '')).toBe('\x1b[5~');
    expect(keyToRawBytes(key({ pageDown: true }), '')).toBe('\x1b[6~');
  });

  it('returns userInput for regular characters', () => {
    expect(keyToRawBytes(baseKey, 'a')).toBe('a');
    expect(keyToRawBytes(baseKey, 'Z')).toBe('Z');
    expect(keyToRawBytes(baseKey, '!')).toBe('!');
  });

  it('returns userInput for ctrl sequences', () => {
    expect(keyToRawBytes(key({ ctrl: true }), '\x03')).toBe('\x03');
    expect(keyToRawBytes(key({ ctrl: true }), '\x04')).toBe('\x04');
  });

  it('returns empty string when no special key and no userInput', () => {
    expect(keyToRawBytes(baseKey, '')).toBe('');
  });
});

import { describe, it, expect } from 'bun:test';
import {
  parseKeybinding,
  resolveKeybinding,
  matchesKeybinding,
  formatKeybinding,
} from '../keybindings';
import type { Key } from '../../hooks/useKeypress';

const blankKey = (overrides: Partial<Key> = {}): Key => ({
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
  ...overrides,
});

describe('parseKeybinding', () => {
  it('parses single named key', () => {
    expect(parseKeybinding('esc')).toEqual({
      ctrl: false,
      shift: false,
      meta: false,
      key: 'escape',
    });
  });

  it('aliases escape ↔ esc', () => {
    expect(parseKeybinding('escape')).toEqual(parseKeybinding('esc'));
  });

  it('parses ctrl+c', () => {
    expect(parseKeybinding('ctrl+c')).toEqual({
      ctrl: true,
      shift: false,
      meta: false,
      key: 'c',
    });
  });

  it('parses ctrl+shift+q', () => {
    expect(parseKeybinding('ctrl+shift+q')).toEqual({
      ctrl: true,
      shift: true,
      meta: false,
      key: 'q',
    });
  });

  it('treats alt, meta, cmd as the same meta modifier', () => {
    expect(parseKeybinding('alt+x')).toEqual(parseKeybinding('meta+x'));
    expect(parseKeybinding('cmd+x')).toEqual(parseKeybinding('meta+x'));
  });

  it('is case-insensitive and whitespace tolerant', () => {
    expect(parseKeybinding('  CTRL + Shift + Q  ')).toEqual({
      ctrl: true,
      shift: true,
      meta: false,
      key: 'q',
    });
  });

  it('returns null for empty input', () => {
    expect(parseKeybinding('')).toBeNull();
    expect(parseKeybinding('   ')).toBeNull();
  });

  it('returns null when no key is given (modifiers only)', () => {
    expect(parseKeybinding('ctrl')).toBeNull();
    expect(parseKeybinding('ctrl+shift')).toBeNull();
  });

  it('returns null for two non-modifier tokens', () => {
    expect(parseKeybinding('a+b')).toBeNull();
  });

  it('returns null for unknown multi-char key', () => {
    expect(parseKeybinding('fnord')).toBeNull();
  });
});

describe('resolveKeybinding', () => {
  it('falls back to default when setting is missing', () => {
    const binding = resolveKeybinding({}, 'cancelStream');
    expect(binding.key).toBe('escape');
  });

  it('falls back to default when settings is null', () => {
    const binding = resolveKeybinding(null, 'quit');
    expect(binding).toEqual({
      ctrl: true,
      shift: false,
      meta: false,
      key: 'c',
    });
  });

  it('uses the user value when parseable', () => {
    const binding = resolveKeybinding(
      { 'chat.keybindings.cancelStream': 'ctrl+g' },
      'cancelStream'
    );
    expect(binding).toEqual({
      ctrl: true,
      shift: false,
      meta: false,
      key: 'g',
    });
  });

  it('falls back to default when user value is unparseable', () => {
    const binding = resolveKeybinding(
      { 'chat.keybindings.closeMenu': 'garbage+++' },
      'closeMenu'
    );
    expect(binding.key).toBe('escape');
  });

  it('falls back to default when user value is wrong type', () => {
    const binding = resolveKeybinding({ 'chat.keybindings.quit': 42 }, 'quit');
    expect(binding).toEqual({
      ctrl: true,
      shift: false,
      meta: false,
      key: 'c',
    });
  });
});

describe('matchesKeybinding', () => {
  it('matches escape against the esc key', () => {
    const binding = parseKeybinding('esc')!;
    expect(matchesKeybinding(binding, '', blankKey({ escape: true }))).toBe(
      true
    );
  });

  it('does not match escape when ctrl is pressed', () => {
    const binding = parseKeybinding('esc')!;
    expect(
      matchesKeybinding(binding, '', blankKey({ escape: true, ctrl: true }))
    ).toBe(false);
  });

  it('matches ctrl+c against Ctrl+C press', () => {
    const binding = parseKeybinding('ctrl+c')!;
    expect(matchesKeybinding(binding, 'c', blankKey({ ctrl: true }))).toBe(
      true
    );
  });

  it('does not match ctrl+c against plain c', () => {
    const binding = parseKeybinding('ctrl+c')!;
    expect(matchesKeybinding(binding, 'c', blankKey({}))).toBe(false);
  });

  it('matches ctrl+shift+q only when shift is reported', () => {
    const binding = parseKeybinding('ctrl+shift+q')!;
    expect(
      matchesKeybinding(binding, 'q', blankKey({ ctrl: true, shift: true }))
    ).toBe(true);
    expect(matchesKeybinding(binding, 'q', blankKey({ ctrl: true }))).toBe(
      false
    );
  });

  it('ascii key match is case-insensitive on input', () => {
    const binding = parseKeybinding('ctrl+c')!;
    expect(matchesKeybinding(binding, 'C', blankKey({ ctrl: true }))).toBe(
      true
    );
  });

  it('matches named arrow keys', () => {
    const binding = parseKeybinding('up')!;
    expect(matchesKeybinding(binding, '', blankKey({ upArrow: true }))).toBe(
      true
    );
  });
});

describe('formatKeybinding', () => {
  it('formats esc without modifiers as lowercase', () => {
    expect(formatKeybinding(parseKeybinding('esc')!)).toBe('esc');
  });

  it('title-cases combos', () => {
    expect(formatKeybinding(parseKeybinding('ctrl+c')!)).toBe('Ctrl+C');
    expect(formatKeybinding(parseKeybinding('ctrl+shift+q')!)).toBe(
      'Ctrl+Shift+Q'
    );
  });

  it('uses arrows for arrow keys', () => {
    expect(formatKeybinding(parseKeybinding('ctrl+up')!)).toBe('Ctrl+↑');
  });
});

import { describe, expect, test } from 'bun:test';
import {
  sanitizeSessionTitleForDisplay,
  stripTerminalEscapes,
} from '../sanitize-title.js';

describe('stripTerminalEscapes', () => {
  test('removes OSC 52 clipboard writes with BEL and ST terminators', () => {
    expect(stripTerminalEscapes('a\x1b]52;c;aGVsbG8=\x07b')).toBe('ab');
    expect(stripTerminalEscapes('a\x1b]52;c;aGVsbG8=\x1b\\b')).toBe('ab');
  });

  test('removes an unterminated OSC so nothing dangles into the terminal', () => {
    expect(stripTerminalEscapes('title\x1b]0;evil')).toBe('title');
  });

  test('removes DCS, SOS, PM, and APC strings', () => {
    expect(stripTerminalEscapes('x\x1bPq#0;2;0;0;0#0~~@@\x1b\\y')).toBe('xy');
    expect(stripTerminalEscapes('x\x1b^privacy\x1b\\y')).toBe('xy');
    expect(stripTerminalEscapes('x\x1b_apc\x1b\\y')).toBe('xy');
  });

  test('removes bare ESC finals like full reset and cursor CSI', () => {
    expect(stripTerminalEscapes('a\x1bcb')).toBe('ab');
    expect(stripTerminalEscapes('a\x1b[2Jb')).toBe('ab');
    expect(stripTerminalEscapes('a\x1b[1;1Hb')).toBe('ab');
  });

  test('removes C0 controls but keeps tabs converted text intact', () => {
    expect(stripTerminalEscapes('a\x00\x08\x0bb')).toBe('ab');
  });

  test('leaves plain multilingual text and emoji untouched', () => {
    expect(stripTerminalEscapes('修复 auth 🐛 bug')).toBe('修复 auth 🐛 bug');
  });
});

describe('sanitizeSessionTitleForDisplay', () => {
  test('strips escapes and escapes newlines', () => {
    expect(sanitizeSessionTitleForDisplay('a\x1b]0;t\x07b\nc')).toBe('ab\\nc');
  });

  test('non-strings render empty', () => {
    expect(sanitizeSessionTitleForDisplay(undefined)).toBe('');
    expect(sanitizeSessionTitleForDisplay(42)).toBe('');
  });
});

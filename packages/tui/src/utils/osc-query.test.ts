import { describe, test, expect } from 'bun:test';
import { parseOsc11Response } from './osc-query';

describe('parseOsc11Response', () => {
  test('parses 4-digit hex channels (dark background)', () => {
    // Black background: rgb:0000/0000/0000
    const response = '\x1b]11;rgb:0000/0000/0000\x1b\\';
    expect(parseOsc11Response(response)).toBe('dark');
  });

  test('parses 4-digit hex channels (light background)', () => {
    // White background: rgb:ffff/ffff/ffff
    const response = '\x1b]11;rgb:ffff/ffff/ffff\x1b\\';
    expect(parseOsc11Response(response)).toBe('light');
  });

  test('parses 2-digit hex channels (dark background)', () => {
    // Dark background: rgb:1c/1c/1c
    const response = '\x1b]11;rgb:1c/1c/1c\x07';
    expect(parseOsc11Response(response)).toBe('dark');
  });

  test('parses 2-digit hex channels (light background)', () => {
    // Light background: rgb:f5/f5/f5
    const response = '\x1b]11;rgb:f5/f5/f5\x07';
    expect(parseOsc11Response(response)).toBe('light');
  });

  test('parses 1-digit hex channels', () => {
    // White: rgb:f/f/f -> 255/255/255
    const response = '\x1b]11;rgb:f/f/f\x07';
    expect(parseOsc11Response(response)).toBe('light');

    // Black: rgb:0/0/0 -> 0/0/0
    const response2 = '\x1b]11;rgb:0/0/0\x07';
    expect(parseOsc11Response(response2)).toBe('dark');
  });

  test('handles typical iTerm2 dark theme response', () => {
    // iTerm2 default dark profile background
    const response = '\x1b]11;rgb:0000/0000/0000\x07';
    expect(parseOsc11Response(response)).toBe('dark');
  });

  test('handles typical macOS Terminal.app light response', () => {
    // Terminal.app default white background
    const response = '\x1b]11;rgb:ffff/ffff/ffff\x1b\\';
    expect(parseOsc11Response(response)).toBe('light');
  });

  test('handles Solarized Dark background', () => {
    // Solarized Dark: #002b36 -> rgb:0000/2b2b/3636
    const response = '\x1b]11;rgb:0000/2b2b/3636\x07';
    expect(parseOsc11Response(response)).toBe('dark');
  });

  test('handles Solarized Light background', () => {
    // Solarized Light: #fdf6e3 -> rgb:fdfd/f6f6/e3e3
    const response = '\x1b]11;rgb:fdfd/f6f6/e3e3\x07';
    expect(parseOsc11Response(response)).toBe('light');
  });

  test('handles Dracula theme background', () => {
    // Dracula: #282a36 -> rgb:2828/2a2a/3636
    const response = '\x1b]11;rgb:2828/2a2a/3636\x07';
    expect(parseOsc11Response(response)).toBe('dark');
  });

  test('handles Gruvbox Light background', () => {
    // Gruvbox Light: #fbf1c7 -> rgb:fbfb/f1f1/c7c7
    const response = '\x1b]11;rgb:fbfb/f1f1/c7c7\x07';
    expect(parseOsc11Response(response)).toBe('light');
  });

  test('returns null for empty response', () => {
    expect(parseOsc11Response('')).toBeNull();
  });

  test('returns null for garbage response', () => {
    expect(parseOsc11Response('not a valid response')).toBeNull();
  });

  test('returns null for partial response', () => {
    expect(parseOsc11Response('\x1b]11;rgb:')).toBeNull();
  });

  test('handles response with extra data before/after', () => {
    // Some terminals may include extra escape sequences
    const response = '\x1b[?1;2c\x1b]11;rgb:0000/0000/0000\x07';
    expect(parseOsc11Response(response)).toBe('dark');
  });

  test('mid-range luminance boundary (just above 128)', () => {
    // rgb:82/82/82 -> luminance = 0.299*130 + 0.587*130 + 0.114*130 = 130 > 128
    const response = '\x1b]11;rgb:82/82/82\x07';
    expect(parseOsc11Response(response)).toBe('light');
  });

  test('mid-range luminance boundary (just below 128)', () => {
    // rgb:7f/7f/7f -> luminance = 0.299*127 + 0.587*127 + 0.114*127 = 127 < 128
    const response = '\x1b]11;rgb:7f/7f/7f\x07';
    expect(parseOsc11Response(response)).toBe('dark');
  });
});

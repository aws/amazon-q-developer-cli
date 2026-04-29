import {
  describe,
  it,
  expect,
  beforeEach,
  afterEach,
  mock,
  afterAll,
} from 'bun:test';

const mockExecSync = mock((): string => '');
mock.module('child_process', () => ({ execSync: mockExecSync }));

afterAll(() => {
  mock.restore();
});

const { queryTerminalBackground, parseOsc11Response } =
  await import('../osc-query');

let originalPlatform: string;
let originalStdinIsTTY: boolean | undefined;

beforeEach(() => {
  originalPlatform = process.platform;
  originalStdinIsTTY = process.stdin.isTTY;
  mockExecSync.mockReset();
});

afterEach(() => {
  Object.defineProperty(process, 'platform', {
    value: originalPlatform,
    configurable: true,
  });
  Object.defineProperty(process.stdin, 'isTTY', {
    value: originalStdinIsTTY,
    writable: true,
    configurable: true,
  });
});

describe('queryTerminalBackground', () => {
  it('returns null on win32', () => {
    Object.defineProperty(process, 'platform', {
      value: 'win32',
      configurable: true,
    });
    expect(queryTerminalBackground()).toBeNull();
    expect(mockExecSync).not.toHaveBeenCalled();
  });

  it('returns null when stdin is not a TTY', () => {
    Object.defineProperty(process.stdin, 'isTTY', {
      value: false,
      writable: true,
      configurable: true,
    });
    expect(queryTerminalBackground()).toBeNull();
    expect(mockExecSync).not.toHaveBeenCalled();
  });

  it('returns dark when execSync returns a dark background response', () => {
    Object.defineProperty(process.stdin, 'isTTY', {
      value: true,
      writable: true,
      configurable: true,
    });
    mockExecSync.mockReturnValue('\x1b]11;rgb:0000/0000/0000\x07');
    expect(queryTerminalBackground()).toBe('dark');
    expect(mockExecSync).toHaveBeenCalledTimes(1);
  });

  it('returns light when execSync returns a light background response', () => {
    Object.defineProperty(process.stdin, 'isTTY', {
      value: true,
      writable: true,
      configurable: true,
    });
    mockExecSync.mockReturnValue('\x1b]11;rgb:ffff/ffff/ffff\x07');
    expect(queryTerminalBackground()).toBe('light');
  });

  it('returns null when execSync throws', () => {
    Object.defineProperty(process.stdin, 'isTTY', {
      value: true,
      writable: true,
      configurable: true,
    });
    mockExecSync.mockImplementation(() => {
      throw new Error('Command timed out');
    });
    expect(queryTerminalBackground()).toBeNull();
  });

  it('returns null when execSync returns unparseable output', () => {
    Object.defineProperty(process.stdin, 'isTTY', {
      value: true,
      writable: true,
      configurable: true,
    });
    mockExecSync.mockReturnValue('garbage data with no rgb info');
    expect(queryTerminalBackground()).toBeNull();
  });
});

describe('parseOsc11Response edge cases', () => {
  it('handles mixed case hex digits', () => {
    const response = '\x1b]11;rgb:Ff/Ff/Ff\x07';
    expect(parseOsc11Response(response)).toBe('light');
  });

  it('handles 3-digit hex per channel', () => {
    // 3-digit hex: 0xfff = 4095, default normalize branch returns 4095
    // which is > 128 so it should be light
    const response = '\x1b]11;rgb:fff/fff/fff\x07';
    expect(parseOsc11Response(response)).toBe('light');
  });

  it('handles response terminated with ST (ESC backslash)', () => {
    const response = '\x1b]11;rgb:1a1a/1a1a/1a1a\x1b\\';
    expect(parseOsc11Response(response)).toBe('dark');
  });

  it('handles response terminated with BEL', () => {
    const response = '\x1b]11;rgb:1a1a/1a1a/1a1a\x07';
    expect(parseOsc11Response(response)).toBe('dark');
  });

  it('parses when rgb match has no surrounding escape sequences', () => {
    const response = 'rgb:00/00/00';
    expect(parseOsc11Response(response)).toBe('dark');
  });

  it('uses the first rgb match when multiple are present', () => {
    // First match is dark, second is light
    const response =
      '\x1b]11;rgb:0000/0000/0000\x07\x1b]11;rgb:ffff/ffff/ffff\x07';
    expect(parseOsc11Response(response)).toBe('dark');
  });

  it('returns null for hex channels with no digits', () => {
    const response = '\x1b]11;rgb:///\x07';
    expect(parseOsc11Response(response)).toBeNull();
  });
});

import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import {
  hasCapability,
  resetCapabilityCache,
  setTerminalProgressIndeterminate,
  setTerminalProgressNormal,
  setTerminalProgressError,
  setTerminalProgressWarning,
  clearTerminalProgress,
  hyperlink,
} from '../terminal-capabilities';

let writtenData: string[];
let originalWrite: typeof process.stdout.write;
let savedEnv: NodeJS.ProcessEnv;
let originalIsTTY: boolean | undefined;

beforeEach(() => {
  writtenData = [];
  originalWrite = process.stdout.write;
  process.stdout.write = ((chunk: any) => {
    writtenData.push(String(chunk));
    return true;
  }) as any;
  savedEnv = { ...process.env };
  originalIsTTY = process.stdout.isTTY;
  // Clean env for predictable tests
  delete process.env.TERM_PROGRAM;
  delete process.env.TERM;
  delete process.env.STY;
  delete process.env.TERMINAL_EMULATOR;
  delete process.env.TMUX;
  delete process.env.WT_SESSION;
  delete process.env.KITTY_WINDOW_ID;
  delete process.env.TERM_PROGRAM_VERSION;
  delete process.env.KIRO_NO_HYPERLINKS;
  delete process.env.KIRO_NO_PROGRESS;
  delete process.env.KIRO_NO_SYNCHRONIZED;
  Object.defineProperty(process.stdout, 'isTTY', {
    value: true,
    writable: true,
    configurable: true,
  });
  resetCapabilityCache();
});

afterEach(() => {
  process.stdout.write = originalWrite;
  process.env = savedEnv;
  Object.defineProperty(process.stdout, 'isTTY', {
    value: originalIsTTY,
    writable: true,
    configurable: true,
  });
  resetCapabilityCache();
});

describe('hasCapability', () => {
  it('returns false for all capabilities when not a TTY', () => {
    Object.defineProperty(process.stdout, 'isTTY', {
      value: false,
      writable: true,
      configurable: true,
    });
    expect(hasCapability('synchronizedOutput')).toBe(false);
    expect(hasCapability('hyperlinks')).toBe(false);
    expect(hasCapability('progressIndicator')).toBe(false);
  });

  describe('synchronizedOutput', () => {
    it('detects iTerm.app', () => {
      process.env.TERM_PROGRAM = 'iTerm.app';
      expect(hasCapability('synchronizedOutput')).toBe(true);
    });

    it('detects kitty via TERM', () => {
      process.env.TERM = 'xterm-kitty';
      expect(hasCapability('synchronizedOutput')).toBe(true);
    });

    it('detects synchronized output in tmux 3.7+', () => {
      process.env.TMUX = '/tmp/tmux-1000/default,12345,0';
      process.env.TERM = 'screen-256color';
      process.env.TERM_PROGRAM = 'tmux';
      process.env.TERM_PROGRAM_VERSION = '3.7b';
      expect(hasCapability('synchronizedOutput')).toBe(true);
    });

    it('uses the bounded path for tmux before 3.7', () => {
      process.env.TMUX = '/tmp/tmux-1000/default,12345,0';
      process.env.TERM_PROGRAM = 'tmux';
      process.env.TERM_PROGRAM_VERSION = '3.6a';
      expect(hasCapability('synchronizedOutput')).toBe(false);
    });

    it('uses the bounded path when the tmux version is unknown', () => {
      process.env.TMUX = '/tmp/tmux-1000/default,12345,0';
      expect(hasCapability('synchronizedOutput')).toBe(false);
    });

    it('ignores inherited KITTY_WINDOW_ID when inside tmux', () => {
      process.env.TMUX = '/tmp/tmux-1000/default,12345,0';
      process.env.KITTY_WINDOW_ID = '7';
      expect(hasCapability('synchronizedOutput')).toBe(false);
    });

    it('uses the bounded path for GNU screen with inherited iTerm metadata', () => {
      process.env.STY = '12345.pts-0.host';
      process.env.TERM = 'screen-256color';
      process.env.TERM_PROGRAM = 'iTerm.app';
      expect(hasCapability('synchronizedOutput')).toBe(false);
    });

    it('recognizes GNU screen from TERM when STY is unavailable', () => {
      process.env.TERM = 'screen.xterm-256color';
      process.env.KITTY_WINDOW_ID = '7';
      expect(hasCapability('synchronizedOutput')).toBe(false);
    });

    it('keeps the bounded path when GNU screen contains tmux', () => {
      process.env.STY = '12345.pts-0.host';
      process.env.TERM = 'screen-256color';
      process.env.TMUX = '/tmp/tmux-1000/default,12345,0';
      process.env.TERM_PROGRAM = 'tmux';
      process.env.TERM_PROGRAM_VERSION = '3.7b';
      expect(hasCapability('synchronizedOutput')).toBe(false);
    });

    it('is disabled by KIRO_NO_SYNCHRONIZED', () => {
      process.env.TERM_PROGRAM = 'iTerm.app';
      process.env.KIRO_NO_SYNCHRONIZED = '1';
      expect(hasCapability('synchronizedOutput')).toBe(false);
    });

    it('is false for unsupported terminal', () => {
      process.env.TERM_PROGRAM = 'SomeUnknownTerminal';
      expect(hasCapability('synchronizedOutput')).toBe(false);
    });
  });

  describe('hyperlinks', () => {
    it('detects WezTerm', () => {
      process.env.TERM_PROGRAM = 'WezTerm';
      expect(hasCapability('hyperlinks')).toBe(true);
    });

    it('detects Hyper', () => {
      process.env.TERM_PROGRAM = 'Hyper';
      expect(hasCapability('hyperlinks')).toBe(true);
    });

    it('detects JetBrains-JediTerm', () => {
      process.env.TERMINAL_EMULATOR = 'JetBrains-JediTerm';
      expect(hasCapability('hyperlinks')).toBe(true);
    });

    it('detects kitty via TERM', () => {
      process.env.TERM = 'xterm-kitty';
      expect(hasCapability('hyperlinks')).toBe(true);
    });

    it('is disabled by KIRO_NO_HYPERLINKS', () => {
      process.env.TERM_PROGRAM = 'WezTerm';
      process.env.KIRO_NO_HYPERLINKS = 'true';
      expect(hasCapability('hyperlinks')).toBe(false);
    });

    it('is false for unsupported terminal', () => {
      process.env.TERM_PROGRAM = 'SomeUnknownTerminal';
      expect(hasCapability('hyperlinks')).toBe(false);
    });
  });

  describe('progressIndicator', () => {
    it('detects iTerm.app', () => {
      process.env.TERM_PROGRAM = 'iTerm.app';
      expect(hasCapability('progressIndicator')).toBe(true);
    });

    it('detects Windows Terminal via WT_SESSION', () => {
      process.env.WT_SESSION = 'some-session-id';
      expect(hasCapability('progressIndicator')).toBe(true);
    });

    it('is disabled by KIRO_NO_PROGRESS', () => {
      process.env.TERM_PROGRAM = 'iTerm.app';
      process.env.KIRO_NO_PROGRESS = '1';
      expect(hasCapability('progressIndicator')).toBe(false);
    });

    it('is false for unsupported terminal', () => {
      process.env.TERM_PROGRAM = 'Alacritty';
      expect(hasCapability('progressIndicator')).toBe(false);
    });
  });

  it('resetCapabilityCache allows re-detection with changed env', () => {
    process.env.TERM_PROGRAM = 'SomeUnknownTerminal';
    expect(hasCapability('synchronizedOutput')).toBe(false);
    expect(hasCapability('hyperlinks')).toBe(false);

    resetCapabilityCache();
    process.env.TERM_PROGRAM = 'WezTerm';
    expect(hasCapability('synchronizedOutput')).toBe(true);
    expect(hasCapability('hyperlinks')).toBe(true);
  });
});

describe('progress functions', () => {
  it('setTerminalProgressIndeterminate writes OSC 9;4;3', () => {
    process.env.TERM_PROGRAM = 'iTerm.app';
    setTerminalProgressIndeterminate();
    expect(writtenData).toContain('\x1b]9;4;3\x07');
  });

  it('setTerminalProgressNormal writes OSC 9;4;1;percent', () => {
    process.env.TERM_PROGRAM = 'WezTerm';
    setTerminalProgressNormal(42);
    expect(writtenData).toContain('\x1b]9;4;1;42\x07');
  });

  it('setTerminalProgressError writes OSC 9;4;2', () => {
    process.env.TERM_PROGRAM = 'iTerm.app';
    setTerminalProgressError();
    expect(writtenData).toContain('\x1b]9;4;2\x07');
  });

  it('setTerminalProgressWarning writes OSC 9;4;4;percent', () => {
    process.env.TERM_PROGRAM = 'WezTerm';
    setTerminalProgressWarning(75);
    expect(writtenData).toContain('\x1b]9;4;4;75\x07');
  });

  it('clearTerminalProgress writes OSC 9;4;0', () => {
    process.env.TERM_PROGRAM = 'iTerm.app';
    clearTerminalProgress();
    expect(writtenData).toContain('\x1b]9;4;0\x07');
  });

  it('progress functions are no-ops when not capable', () => {
    // No progress-capable terminal set
    process.env.TERM_PROGRAM = 'Alacritty';
    setTerminalProgressIndeterminate();
    setTerminalProgressNormal(50);
    setTerminalProgressError();
    setTerminalProgressWarning(25);
    clearTerminalProgress();
    expect(writtenData).toEqual([]);
  });

  it('setTerminalProgressNormal clamps percent to 0-100', () => {
    process.env.TERM_PROGRAM = 'iTerm.app';
    setTerminalProgressNormal(-10);
    expect(writtenData).toContain('\x1b]9;4;1;0\x07');
    writtenData.length = 0;
    resetCapabilityCache();
    process.env.TERM_PROGRAM = 'iTerm.app';
    setTerminalProgressNormal(200);
    expect(writtenData).toContain('\x1b]9;4;1;100\x07');
  });

  it('setTerminalProgressWarning clamps percent to 0-100', () => {
    process.env.TERM_PROGRAM = 'WezTerm';
    setTerminalProgressWarning(-5);
    expect(writtenData).toContain('\x1b]9;4;4;0\x07');
    writtenData.length = 0;
    resetCapabilityCache();
    process.env.TERM_PROGRAM = 'WezTerm';
    setTerminalProgressWarning(150);
    expect(writtenData).toContain('\x1b]9;4;4;100\x07');
  });
});

describe('hyperlink', () => {
  it('wraps text in OSC 8 when capable', () => {
    process.env.TERM_PROGRAM = 'iTerm.app';
    const result = hyperlink('https://example.com', 'Click here');
    expect(result).toBe(
      '\x1b]8;;https://example.com\x07Click here\x1b]8;;\x07'
    );
  });

  it('returns plain text when not capable', () => {
    process.env.TERM_PROGRAM = 'SomeUnknownTerminal';
    const result = hyperlink('https://example.com', 'Click here');
    expect(result).toBe('Click here');
  });
});

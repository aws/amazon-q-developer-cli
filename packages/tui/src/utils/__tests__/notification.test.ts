import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { resolveNotificationMethod, playNotification } from '../notification';

let writtenData: string[];
let originalWrite: typeof process.stdout.write;
let savedEnv: NodeJS.ProcessEnv;

beforeEach(() => {
  writtenData = [];
  originalWrite = process.stdout.write;
  process.stdout.write = ((chunk: any) => {
    writtenData.push(String(chunk));
    return true;
  }) as any;
  savedEnv = { ...process.env };
  delete process.env.TERM_PROGRAM;
  delete process.env.TERM;
});

afterEach(() => {
  process.stdout.write = originalWrite;
  process.env = savedEnv;
});

describe('resolveNotificationMethod', () => {
  describe('explicit settings', () => {
    it('returns bel when setting is bel', () => {
      expect(resolveNotificationMethod('bel')).toBe('bel');
    });

    it('returns osc9 when setting is osc9', () => {
      expect(resolveNotificationMethod('osc9')).toBe('osc9');
    });
  });

  describe('auto-detection via TERM_PROGRAM', () => {
    it('returns osc9 for ghostty', () => {
      process.env.TERM_PROGRAM = 'Ghostty';
      expect(resolveNotificationMethod()).toBe('osc9');
    });

    it('returns osc9 for iterm.app', () => {
      process.env.TERM_PROGRAM = 'iTerm.app';
      expect(resolveNotificationMethod()).toBe('osc9');
    });

    it('returns osc9 for wezterm', () => {
      process.env.TERM_PROGRAM = 'WezTerm';
      expect(resolveNotificationMethod()).toBe('osc9');
    });

    it('returns osc9 for windows_terminal', () => {
      process.env.TERM_PROGRAM = 'windows_terminal';
      expect(resolveNotificationMethod()).toBe('osc9');
    });
  });

  describe('auto-detection via TERM', () => {
    it('returns osc9 for xterm-ghostty', () => {
      process.env.TERM = 'xterm-ghostty';
      expect(resolveNotificationMethod()).toBe('osc9');
    });

    it('returns bel for xterm', () => {
      process.env.TERM = 'xterm';
      expect(resolveNotificationMethod()).toBe('bel');
    });

    it('returns bel for xterm-256color', () => {
      process.env.TERM = 'xterm-256color';
      expect(resolveNotificationMethod()).toBe('bel');
    });

    it('returns bel for alacritty', () => {
      process.env.TERM = 'alacritty';
      expect(resolveNotificationMethod()).toBe('bel');
    });

    it('returns bel for tmux-256color', () => {
      process.env.TERM = 'tmux-256color';
      expect(resolveNotificationMethod()).toBe('bel');
    });

    it('returns bel for screen', () => {
      process.env.TERM = 'screen';
      expect(resolveNotificationMethod()).toBe('bel');
    });

    it('returns bel for linux', () => {
      process.env.TERM = 'linux';
      expect(resolveNotificationMethod()).toBe('bel');
    });
  });

  it('returns null when no TERM_PROGRAM or TERM is set', () => {
    expect(resolveNotificationMethod()).toBeNull();
  });

  it('returns null for unknown TERM value', () => {
    process.env.TERM = 'completely-unknown-terminal';
    expect(resolveNotificationMethod()).toBeNull();
  });
});

describe('playNotification', () => {
  it('writes BEL character for bel method', () => {
    playNotification('bel');
    expect(writtenData).toEqual(['\x07']);
  });

  it('writes OSC 9 with default message for osc9 method', () => {
    playNotification('osc9');
    expect(writtenData).toEqual(['\x1b]9;Kiro CLI needs attention\x07']);
  });

  it('writes OSC 9 with custom message for osc9 method', () => {
    playNotification('osc9', 'Task complete');
    expect(writtenData).toEqual(['\x1b]9;Task complete\x07']);
  });

  it('bel method ignores custom message', () => {
    playNotification('bel', 'ignored message');
    expect(writtenData).toEqual(['\x07']);
  });
});

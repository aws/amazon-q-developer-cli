import { describe, it, expect, beforeEach, afterEach, mock } from 'bun:test';

const mockExecSync = mock((_cmd?: unknown, _opts?: unknown): string => {
  throw new Error('not available');
});

mock.module('child_process', () => ({
  execSync: mockExecSync,
}));

// Import AFTER mock.module so child_process is mocked throughout the dependency tree
const { detectTerminalTheme, detectTerminalThemeWithDetails } =
  await import('../terminal-theme');

let savedEnv: NodeJS.ProcessEnv;
const originalPlatform = process.platform;
const originalStdinIsTTY = process.stdin.isTTY;

beforeEach(() => {
  savedEnv = { ...process.env };
  // Clean all env vars that affect detection
  for (const key of [
    'COLORFGBG',
    'GHOSTTY_RESOURCES_DIR',
    'TERM_PROGRAM',
    'TERM',
    'ITERM_PROFILE',
    'KITTY_THEME',
    'WT_SESSION',
    'VSCODE_TERMINAL_THEME',
    'HYPER_THEME',
    'GTK_THEME',
    'LOCALAPPDATA',
    'TMUX',
    'TERMINAL_EMULATOR',
  ]) {
    delete process.env[key];
  }
  // Prevent OSC 11 query (requires TTY)
  Object.defineProperty(process.stdin, 'isTTY', {
    value: false,
    writable: true,
    configurable: true,
  });
  // Default to linux to skip Windows-only paths
  Object.defineProperty(process, 'platform', {
    value: 'linux',
    configurable: true,
  });
  mockExecSync.mockReset();
  mockExecSync.mockImplementation(() => {
    throw new Error('not available');
  });
});

afterEach(() => {
  process.env = savedEnv;
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

describe('detectTerminalTheme', () => {
  it('returns a string value (dark or light)', () => {
    const result = detectTerminalTheme();
    expect(typeof result).toBe('string');
    expect(['dark', 'light']).toContain(result);
  });
});

describe('detectTerminalThemeWithDetails', () => {
  it('returns default dark fallback when no signals available', () => {
    const result = detectTerminalThemeWithDetails();
    expect(result).toEqual({
      theme: 'dark',
      method: 'default',
      confidence: 'low',
    });
  });

  describe('COLORFGBG', () => {
    it('detects dark theme with bg=0 (COLORFGBG "15;0")', () => {
      process.env.COLORFGBG = '15;0';
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'dark',
        method: 'COLORFGBG',
        confidence: 'high',
      });
    });

    it('detects light theme with bg=15 (COLORFGBG "0;15")', () => {
      process.env.COLORFGBG = '0;15';
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'light',
        method: 'COLORFGBG',
        confidence: 'high',
      });
    });

    it('detects dark theme with bg=8 (special case)', () => {
      process.env.COLORFGBG = '0;8';
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'dark',
        method: 'COLORFGBG',
        confidence: 'high',
      });
    });

    it('detects light theme with bg=7', () => {
      process.env.COLORFGBG = '0;7';
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'light',
        method: 'COLORFGBG',
        confidence: 'high',
      });
    });

    it('falls through on malformed COLORFGBG', () => {
      process.env.COLORFGBG = 'abc';
      const result = detectTerminalThemeWithDetails();
      // Should fall through to default since no other signals
      expect(result.method).toBe('default');
    });
  });

  describe('Terminal-specific env vars', () => {
    it('detects Ghostty via GHOSTTY_RESOURCES_DIR', () => {
      process.env.GHOSTTY_RESOURCES_DIR = '/usr/share/ghostty';
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'dark',
        method: 'Ghostty-default',
        confidence: 'medium',
      });
    });

    it('detects Ghostty via TERM_PROGRAM=ghostty', () => {
      process.env.TERM_PROGRAM = 'ghostty';
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'dark',
        method: 'Ghostty-default',
        confidence: 'medium',
      });
    });

    it('detects light theme from ITERM_PROFILE', () => {
      process.env.ITERM_PROFILE = 'My Light Theme';
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'light',
        method: 'ITERM_PROFILE',
        confidence: 'medium',
      });
    });

    it('detects dark theme from ITERM_PROFILE', () => {
      process.env.ITERM_PROFILE = 'Solarized Dark';
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'dark',
        method: 'ITERM_PROFILE',
        confidence: 'medium',
      });
    });

    it('detects light theme from KITTY_THEME', () => {
      process.env.KITTY_THEME = 'Gruvbox Light';
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'light',
        method: 'KITTY_THEME',
        confidence: 'medium',
      });
    });

    it('detects dark theme from KITTY_THEME', () => {
      process.env.KITTY_THEME = 'Dracula Dark';
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'dark',
        method: 'KITTY_THEME',
        confidence: 'medium',
      });
    });

    it('detects dark theme from VS Code terminal', () => {
      process.env.TERM_PROGRAM = 'vscode';
      process.env.VSCODE_TERMINAL_THEME = 'One Dark';
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'dark',
        method: 'VSCODE_TERMINAL_THEME',
        confidence: 'medium',
      });
    });

    it('detects light theme from HYPER_THEME', () => {
      process.env.HYPER_THEME = 'hyper-snazzy-light';
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'light',
        method: 'HYPER_THEME',
        confidence: 'medium',
      });
    });
  });

  describe('Linux detection', () => {
    it('detects dark from GNOME gsettings', () => {
      mockExecSync.mockImplementation((cmd: unknown) => {
        if (typeof cmd === 'string' && cmd.includes('gsettings')) {
          return "'prefer-dark'";
        }
        throw new Error('not available');
      });
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'dark',
        method: 'GNOME-color-scheme',
        confidence: 'low',
      });
    });

    it('detects light from GNOME gsettings', () => {
      mockExecSync.mockImplementation((cmd: unknown) => {
        if (typeof cmd === 'string' && cmd.includes('gsettings')) {
          return "'prefer-light'";
        }
        throw new Error('not available');
      });
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'light',
        method: 'GNOME-color-scheme',
        confidence: 'low',
      });
    });

    it('detects dark from KDE kreadconfig5', () => {
      mockExecSync.mockImplementation((cmd: unknown) => {
        if (typeof cmd === 'string' && cmd.includes('kreadconfig5')) {
          return 'BreezeDark';
        }
        throw new Error('not available');
      });
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'dark',
        method: 'KDE-ColorScheme',
        confidence: 'low',
      });
    });

    it('detects dark from GTK_THEME env var', () => {
      process.env.GTK_THEME = 'Adwaita:dark';
      const result = detectTerminalThemeWithDetails();
      expect(result).toEqual({
        theme: 'dark',
        method: 'GTK_THEME',
        confidence: 'low',
      });
    });
  });
});

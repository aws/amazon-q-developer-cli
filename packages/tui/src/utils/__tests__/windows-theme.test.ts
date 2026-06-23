import {
  describe,
  it,
  test,
  expect,
  beforeEach,
  afterEach,
  mock,
} from 'bun:test';
import { mkdirSync, writeFileSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import {
  detectWindowsTerminalTheme,
  detectWindowsConsoleBackground,
  type WindowsConsoleDeps,
} from '../windows-theme.js';

/**
 * Single source of truth for windows-theme. Covers Windows Terminal
 * settings.json parsing (file-based, real fs) AND the console-background
 * detection logic + its security hardening (CWE-426 untrusted search path):
 * PowerShell is invoked via its ABSOLUTE System32 path with `shell:false` and
 * an args array, never a bare `powershell` through cmd.exe.
 *
 * The exec dependency is injected directly. We deliberately do NOT use
 * `mock.module('child_process', ...)`: bun's module mocks are process-global
 * and leak into every other test file in the run. detectWindowsTerminalTheme
 * uses the real `fs` against a per-test tmpdir, so it needs no mocking.
 */

const execFileSyncMock = mock(
  (_file: string, _args?: readonly string[], _opts?: unknown): string => ''
);

const consoleDeps: WindowsConsoleDeps = {
  execFileSync:
    execFileSyncMock as unknown as WindowsConsoleDeps['execFileSync'],
};

const originalPlatform = process.platform;
const originalLocalAppData = process.env.LOCALAPPDATA;
const originalSystemRoot = process.env.SystemRoot;
let testDir: string;

beforeEach(() => {
  testDir = join(
    tmpdir(),
    `wt-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
  mkdirSync(testDir, { recursive: true });
  execFileSyncMock.mockClear();
  execFileSyncMock.mockImplementation(() => '');
});

afterEach(() => {
  Object.defineProperty(process, 'platform', {
    value: originalPlatform,
    configurable: true,
  });
  if (originalLocalAppData === undefined) {
    delete process.env.LOCALAPPDATA;
  } else {
    process.env.LOCALAPPDATA = originalLocalAppData;
  }
  if (originalSystemRoot === undefined) {
    delete process.env.SystemRoot;
  } else {
    process.env.SystemRoot = originalSystemRoot;
  }
  try {
    rmSync(testDir, { recursive: true, force: true });
  } catch {
    /* ignore */
  }
});

function setWin32() {
  Object.defineProperty(process, 'platform', {
    value: 'win32',
    configurable: true,
  });
  process.env.LOCALAPPDATA = testDir;
}

/**
 * Write a settings.json file at the first Windows Terminal settings path
 * (Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/).
 */
function writeSettingsJson(content: string) {
  const dir = join(
    testDir,
    'Packages',
    'Microsoft.WindowsTerminal_8wekyb3d8bbwe',
    'LocalState'
  );
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'settings.json'), content, 'utf8');
}

/**
 * Write a settings.json file at the second (Scoop/portable) settings path.
 */
function writeSettingsJsonAlt(content: string) {
  const dir = join(testDir, 'Microsoft', 'Windows Terminal');
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'settings.json'), content, 'utf8');
}

function makeWTSettings(opts: {
  colorScheme?: string;
  schemes?: Array<{ name: string; background?: string }>;
  comments?: boolean;
  blockComments?: boolean;
}): string {
  const settings: Record<string, unknown> = {};

  if (opts.colorScheme !== undefined || opts.schemes !== undefined) {
    settings.profiles = {
      defaults: opts.colorScheme
        ? { colorScheme: opts.colorScheme }
        : undefined,
    };
  }

  if (opts.schemes) {
    settings.schemes = opts.schemes;
  }

  let json = JSON.stringify(settings, null, 2);

  if (opts.comments) {
    json = '// This is a comment\n' + json;
  }

  if (opts.blockComments) {
    json = '/* block comment */\n' + json;
  }

  return json;
}

describe('detectWindowsTerminalTheme', () => {
  it('returns null on non-win32 platform', () => {
    Object.defineProperty(process, 'platform', {
      value: 'darwin',
      configurable: true,
    });
    expect(detectWindowsTerminalTheme()).toBeNull();
  });

  it('returns null when LOCALAPPDATA is not set', () => {
    Object.defineProperty(process, 'platform', {
      value: 'win32',
      configurable: true,
    });
    delete process.env.LOCALAPPDATA;
    expect(detectWindowsTerminalTheme()).toBeNull();
  });

  it('returns null when settings.json is not found', () => {
    setWin32();
    // testDir exists but no settings.json files are written
    expect(detectWindowsTerminalTheme()).toBeNull();
  });

  it('parses dark color scheme background', () => {
    setWin32();
    const settings = makeWTSettings({
      colorScheme: 'MyDarkScheme',
      schemes: [{ name: 'MyDarkScheme', background: '#1e1e1e' }],
    });
    writeSettingsJson(settings);
    const result = detectWindowsTerminalTheme();
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('dark');
    expect(result!.confidence).toBe('medium');
    expect(result!.method).toContain('MyDarkScheme');
  });

  it('parses light color scheme background', () => {
    setWin32();
    const settings = makeWTSettings({
      colorScheme: 'MyLightScheme',
      schemes: [{ name: 'MyLightScheme', background: '#ffffff' }],
    });
    writeSettingsJson(settings);
    const result = detectWindowsTerminalTheme();
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('light');
    expect(result!.confidence).toBe('medium');
  });

  it('handles JSON with // line comments', () => {
    setWin32();
    const settings = makeWTSettings({
      colorScheme: 'MyDark',
      schemes: [{ name: 'MyDark', background: '#000000' }],
      comments: true,
    });
    writeSettingsJson(settings);
    const result = detectWindowsTerminalTheme();
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('dark');
  });

  it('handles JSON with /* */ block comments', () => {
    setWin32();
    const settings = makeWTSettings({
      colorScheme: 'MyDark',
      schemes: [{ name: 'MyDark', background: '#0a0a0a' }],
      blockComments: true,
    });
    writeSettingsJson(settings);
    const result = detectWindowsTerminalTheme();
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('dark');
  });

  it('falls back to built-in Campbell scheme as dark', () => {
    setWin32();
    // No colorScheme set -> defaults to 'Campbell', no matching scheme in list
    const settings = JSON.stringify({
      profiles: { defaults: {} },
      schemes: [],
    });
    writeSettingsJson(settings);
    const result = detectWindowsTerminalTheme();
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('dark');
    expect(result!.method).toContain('builtin');
    expect(result!.method).toContain('Campbell');
  });

  it('falls back to built-in One Half Light scheme as light', () => {
    setWin32();
    const settings = JSON.stringify({
      profiles: { defaults: { colorScheme: 'One Half Light' } },
      schemes: [],
    });
    writeSettingsJson(settings);
    const result = detectWindowsTerminalTheme();
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('light');
    expect(result!.method).toContain('builtin');
  });

  it('returns light for unknown scheme name containing "light"', () => {
    setWin32();
    const settings = JSON.stringify({
      profiles: { defaults: { colorScheme: 'My Custom Light Theme' } },
      schemes: [],
    });
    writeSettingsJson(settings);
    const result = detectWindowsTerminalTheme();
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('light');
    expect(result!.confidence).toBe('low');
  });

  it('returns dark for unknown scheme name containing "dark"', () => {
    setWin32();
    const settings = JSON.stringify({
      profiles: { defaults: { colorScheme: 'My Custom Dark Theme' } },
      schemes: [],
    });
    writeSettingsJson(settings);
    const result = detectWindowsTerminalTheme();
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('dark');
    expect(result!.confidence).toBe('low');
  });

  it('returns null for unknown scheme without dark/light keyword', () => {
    setWin32();
    const settings = JSON.stringify({
      profiles: { defaults: { colorScheme: 'Monokai' } },
      schemes: [],
    });
    writeSettingsJson(settings);
    expect(detectWindowsTerminalTheme()).toBeNull();
  });

  it('uses second settings.json path when first fails', () => {
    setWin32();
    // Only write to the second (alt) path
    const settings = makeWTSettings({
      colorScheme: 'SecondPath',
      schemes: [{ name: 'SecondPath', background: '#fafafa' }],
    });
    writeSettingsJsonAlt(settings);
    const result = detectWindowsTerminalTheme();
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('light');
  });

  it('performs case-insensitive scheme name matching', () => {
    setWin32();
    const settings = makeWTSettings({
      colorScheme: 'MYDARK',
      schemes: [{ name: 'mydark', background: '#1e1e1e' }],
    });
    writeSettingsJson(settings);
    const result = detectWindowsTerminalTheme();
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('dark');
  });
});

describe('detectWindowsConsoleBackground', () => {
  beforeEach(() => {
    setWin32();
    process.env.SystemRoot = 'C:\\Windows';
  });

  // --- detection logic (driven through the injected execFileSync) ---

  it('returns dark for Black output', () => {
    execFileSyncMock.mockImplementation(() => 'Black\n');
    const result = detectWindowsConsoleBackground(consoleDeps);
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('dark');
    expect(result!.method).toBe('Win-ConsoleBackground');
  });

  it('returns dark for DarkBlue output', () => {
    execFileSyncMock.mockImplementation(() => 'DarkBlue\n');
    const result = detectWindowsConsoleBackground(consoleDeps);
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('dark');
  });

  it('returns light for White output', () => {
    execFileSyncMock.mockImplementation(() => 'White\n');
    const result = detectWindowsConsoleBackground(consoleDeps);
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('light');
  });

  it('returns light for Gray output', () => {
    execFileSyncMock.mockImplementation(() => 'Gray\n');
    const result = detectWindowsConsoleBackground(consoleDeps);
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('light');
  });

  it('returns null when the PowerShell command fails', () => {
    execFileSyncMock.mockImplementation(() => {
      throw new Error('powershell not found');
    });
    expect(detectWindowsConsoleBackground(consoleDeps)).toBeNull();
  });

  it('returns null for unrecognized color output', () => {
    execFileSyncMock.mockImplementation(() => 'SomeUnknownColor\n');
    expect(detectWindowsConsoleBackground(consoleDeps)).toBeNull();
  });

  // --- security hardening (CWE-426) ---

  test('invokes powershell.exe by absolute System32 path with shell:false', () => {
    execFileSyncMock.mockImplementation(() => 'Black\n');

    const result = detectWindowsConsoleBackground(consoleDeps);
    expect(result?.theme).toBe('dark');

    expect(execFileSyncMock).toHaveBeenCalledTimes(1);
    const [file, args, opts] = execFileSyncMock.mock.calls[0] as [
      string,
      string[],
      { shell?: boolean },
    ];
    expect(file).toBe(
      'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe'
    );
    expect(file).not.toBe('powershell');
    expect(args).toEqual([
      '-NoProfile',
      '-Command',
      '$Host.UI.RawUI.BackgroundColor',
    ]);
    expect(opts.shell).toBe(false);
  });

  test('falls back to the C:\\Windows System32 path when SystemRoot is unset', () => {
    delete process.env.SystemRoot;
    execFileSyncMock.mockImplementation(() => 'Black');
    detectWindowsConsoleBackground(consoleDeps);
    const [file] = execFileSyncMock.mock.calls[0] as [string];
    expect(file).toBe(
      'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe'
    );
  });
});

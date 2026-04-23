import { describe, it, expect, beforeEach, afterEach, mock } from 'bun:test';
import { mkdirSync, writeFileSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';

// Only mock child_process (safe -- Node built-in). Do NOT mock 'fs' as it
// persists across test files and breaks file-search.test.ts, sessions.test.ts, etc.
const mockExecSync = mock((): string => '');
mock.module('child_process', () => ({
  execSync: mockExecSync,
}));

const { detectWindowsTerminalTheme, detectWindowsConsoleBackground } =
  await import('../windows-theme');

const originalPlatform = process.platform;
const originalLocalAppData = process.env.LOCALAPPDATA;
let testDir: string;

beforeEach(() => {
  testDir = join(
    tmpdir(),
    `wt-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
  mkdirSync(testDir, { recursive: true });
  mockExecSync.mockReset();
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
  });

  it('returns dark for Black output', () => {
    mockExecSync.mockImplementation(() => 'Black\n');
    const result = detectWindowsConsoleBackground();
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('dark');
    expect(result!.method).toBe('Win-ConsoleBackground');
  });

  it('returns dark for DarkBlue output', () => {
    mockExecSync.mockImplementation(() => 'DarkBlue\n');
    const result = detectWindowsConsoleBackground();
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('dark');
  });

  it('returns light for White output', () => {
    mockExecSync.mockImplementation(() => 'White\n');
    const result = detectWindowsConsoleBackground();
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('light');
  });

  it('returns light for Gray output', () => {
    mockExecSync.mockImplementation(() => 'Gray\n');
    const result = detectWindowsConsoleBackground();
    expect(result).not.toBeNull();
    expect(result!.theme).toBe('light');
  });

  it('returns null when PowerShell command fails', () => {
    mockExecSync.mockImplementation(() => {
      throw new Error('powershell not found');
    });
    expect(detectWindowsConsoleBackground()).toBeNull();
  });

  it('returns null for unrecognized color output', () => {
    mockExecSync.mockImplementation(() => 'SomeUnknownColor\n');
    expect(detectWindowsConsoleBackground()).toBeNull();
  });
});

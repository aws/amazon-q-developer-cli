/**
 * Tests for `/settings terminal` implementation.
 *
 * Coverage strategy:
 *   - Pure helpers (JSONC insertion) — unit tests with no I/O.
 *   - Terminal detection — manipulate env vars.
 *   - Per-terminal setup — real filesystem writes under a tmpdir home,
 *     env vars set to target each terminal. Idempotency and backup behavior
 *     verified on filesystem, not via mocks.
 *   - Apple Terminal is covered at the integration level only for the
 *     "non-macOS rejection" case (we don't mock PlistBuddy).
 *   - Tmux note appended when `TMUX` is set under native-support terminals.
 */

import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'fs';
import { join } from 'path';
import { tmpdir, platform } from 'os';
import { insertIntoJSONCArray, setupTerminal } from '../terminal-setup.js';

let testDir: string;
const envKeys = [
  'HOME',
  'TERM_PROGRAM',
  'TERM',
  'KITTY_WINDOW_ID',
  'ALACRITTY_LOG',
  'TMUX',
  'XDG_CONFIG_HOME',
  'APPDATA',
  'VSCODE_GIT_ASKPASS_MAIN',
  'PATH',
  // Secondary detection — terminals that leak identifying env vars
  // through tmux.
  'LC_TERMINAL',
  'WEZTERM_PANE',
  'WEZTERM_EXECUTABLE',
  'GHOSTTY_RESOURCES_DIR',
  '__CFBundleIdentifier',
] as const;

let originalEnv: Partial<Record<(typeof envKeys)[number], string | undefined>> =
  {};

beforeEach(() => {
  testDir = join(
    tmpdir(),
    `terminal-setup-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
  mkdirSync(testDir, { recursive: true });

  originalEnv = {};
  for (const k of envKeys) {
    originalEnv[k] = process.env[k];
    delete process.env[k];
  }
  // Ensure config paths resolve under tmpdir.
  process.env.HOME = testDir;
  // Keep PATH minimal so execFile calls (PlistBuddy etc) can still find
  // system binaries when needed by Apple Terminal tests.
  process.env.PATH = originalEnv.PATH;
});

afterEach(() => {
  for (const k of envKeys) {
    const v = originalEnv[k];
    if (v === undefined) {
      delete process.env[k];
    } else {
      process.env[k] = v;
    }
  }
  try {
    rmSync(testDir, { recursive: true, force: true });
  } catch {
    // ignore
  }
});

// ---------------------------------------------------------------------------
// insertIntoJSONCArray
// ---------------------------------------------------------------------------

describe('insertIntoJSONCArray', () => {
  it('inserts into an empty array', () => {
    const result = insertIntoJSONCArray('[]', '{"key":"test"}');
    expect(result).toContain('{"key":"test"}');
    expect(result.trim().startsWith('[')).toBe(true);
    expect(result.trim().endsWith(']')).toBe(true);
  });

  it('appends to an array with existing entries, adding a comma', () => {
    const existing = `[
    {"key":"ctrl+a","command":"foo"}
]`;
    const result = insertIntoJSONCArray(existing, '{"key":"test"}');
    expect(result).toContain('{"key":"ctrl+a","command":"foo"}');
    expect(result).toContain('{"key":"test"}');
    // Comma separator between entries.
    expect(result).toMatch(/},\s*\n\{"key":"test"\}/);
  });

  it('preserves JSONC comments', () => {
    const existing = '[\n    // my comment\n    {"key":"a"}\n]';
    const result = insertIntoJSONCArray(existing, '{"key":"b"}');
    expect(result).toContain('// my comment');
    expect(result).toContain('{"key":"a"}');
    expect(result).toContain('{"key":"b"}');
  });

  it('wraps non-array content into an array', () => {
    const result = insertIntoJSONCArray('not an array', '{"key":"test"}');
    expect(result.trim().startsWith('[')).toBe(true);
    expect(result.trim().endsWith(']')).toBe(true);
    expect(result).toContain('{"key":"test"}');
  });
});

// ---------------------------------------------------------------------------
// Unknown terminal
// ---------------------------------------------------------------------------

describe('setupTerminal - unknown terminal', () => {
  it('returns an error message mentioning Ctrl+J', async () => {
    process.env.TERM_PROGRAM = 'totally-made-up-terminal';
    const result = await setupTerminal();
    expect(result.success).toBe(false);
    expect(result.message).toContain('totally-made-up-terminal');
    expect(result.message).toContain('Ctrl+J');
  });

  it("doesn't touch the filesystem", async () => {
    process.env.TERM_PROGRAM = 'totally-made-up-terminal';
    await setupTerminal();
    // No VS Code / Alacritty / Zed config should have been created.
    expect(existsSync(join(testDir, '.config'))).toBe(false);
    expect(existsSync(join(testDir, 'Library'))).toBe(false);
    expect(existsSync(join(testDir, 'AppData'))).toBe(false);
  });

  it('adds a tmux recovery hint when unknown and TMUX is set', async () => {
    // Final fallback when secondary detection also fails — point the user
    // at exiting tmux rather than silently failing.
    process.env.TERM_PROGRAM = 'tmux';
    process.env.TMUX = '/tmp/tmux-501/default,1234,0';
    const result = await setupTerminal();
    expect(result.success).toBe(false);
    expect(result.message).toContain("If you're in tmux");
    expect(result.message).toContain('exit tmux first');
  });
});

// ---------------------------------------------------------------------------
// Secondary detection (tmux / screen scenarios)
// ---------------------------------------------------------------------------

describe('setupTerminal - secondary env detection', () => {
  // Tmux rewrites TERM_PROGRAM to 'tmux' but passes through other
  // terminal-identifying env vars. We should recover from that and
  // route to the right terminal branch.

  it('detects iTerm2 via LC_TERMINAL when TERM_PROGRAM=tmux', async () => {
    process.env.TERM_PROGRAM = 'tmux';
    process.env.TMUX = '/tmp/tmux-501/default,1234,0';
    process.env.LC_TERMINAL = 'iTerm2';
    const result = await setupTerminal();
    expect(result.success).toBe(true);
    expect(result.message).toContain('iTerm2');
    expect(result.message).toContain('natively supported');
    // Since we're in tmux, the note should be appended.
    expect(result.message).toContain('running in tmux');
  });

  it('detects WezTerm via WEZTERM_PANE when TERM_PROGRAM=tmux', async () => {
    process.env.TERM_PROGRAM = 'tmux';
    process.env.TMUX = '/tmp/tmux-501/default,1234,0';
    process.env.WEZTERM_PANE = '0';
    const result = await setupTerminal();
    expect(result.success).toBe(true);
    expect(result.message).toContain('WezTerm');
  });

  it('detects Ghostty via GHOSTTY_RESOURCES_DIR when TERM_PROGRAM=tmux', async () => {
    process.env.TERM_PROGRAM = 'tmux';
    process.env.TMUX = '/tmp/tmux-501/default,1234,0';
    process.env.GHOSTTY_RESOURCES_DIR = '/opt/ghostty/resources';
    const result = await setupTerminal();
    expect(result.success).toBe(true);
    expect(result.message).toContain('Ghostty');
  });

  it('does NOT trust __CFBundleIdentifier inside tmux (stale-env risk)', async () => {
    // tmux sessions capture env at creation time. A tmux session originally
    // created from Apple Terminal and later attached from iTerm2 would still
    // carry `__CFBundleIdentifier=com.apple.Terminal`. Since routing to the
    // Apple Terminal branch modifies the user's plist, we only trust this
    // marker outside tmux.
    process.env.TERM_PROGRAM = 'tmux';
    process.env.TMUX = '/tmp/tmux-501/default,1234,0';
    process.env.__CFBundleIdentifier = 'com.apple.Terminal';
    const result = await setupTerminal();
    // Falls through to unknown + tmux hint, NOT Apple Terminal plist setup.
    expect(result.success).toBe(false);
    expect(result.message).toContain("If you're in tmux");
  });
});

// ---------------------------------------------------------------------------
// Native-support terminals
// ---------------------------------------------------------------------------

describe('setupTerminal - native-support terminals', () => {
  it.each([
    ['iTerm.app', 'iTerm2'],
    ['WezTerm', 'WezTerm'],
    ['ghostty', 'Ghostty'],
    ['WarpTerminal', 'Warp'],
  ])('returns no-setup-needed for %s', async (termProgram, displayName) => {
    process.env.TERM_PROGRAM = termProgram;
    const result = await setupTerminal();
    expect(result.success).toBe(true);
    expect(result.message).toContain(displayName);
    expect(result.message).toContain('natively supported');
  });

  it('detects Kitty via KITTY_WINDOW_ID', async () => {
    process.env.KITTY_WINDOW_ID = '42';
    const result = await setupTerminal();
    expect(result.success).toBe(true);
    expect(result.message).toContain('Kitty');
  });
});

// ---------------------------------------------------------------------------
// Tmux note
// ---------------------------------------------------------------------------

describe('setupTerminal - tmux note', () => {
  it('appends the tmux reminder for native terminals when TMUX is set', async () => {
    process.env.TERM_PROGRAM = 'iTerm.app';
    process.env.TMUX = '/tmp/tmux-501/default,1234,0';
    const result = await setupTerminal();
    expect(result.message).toContain('running in tmux');
    expect(result.message).toContain('extended-keys');
    expect(result.message).toContain("'xterm*:extkeys'");
  });

  it('includes a stale-session disclaimer in the tmux note', async () => {
    // tmux sessions inherit env at creation time. Users who attach from a
    // different terminal can see an incorrectly-identified terminal name.
    // The note should tell them how to refresh detection.
    process.env.TERM_PROGRAM = 'iTerm.app';
    process.env.TMUX = '/tmp/tmux-501/default,1234,0';
    const result = await setupTerminal();
    expect(result.message).toContain('Wrong terminal');
    expect(result.message).toContain('Exit and reopen tmux');
  });

  it('does not append the tmux reminder when TMUX is unset', async () => {
    process.env.TERM_PROGRAM = 'iTerm.app';
    delete process.env.TMUX;
    const result = await setupTerminal();
    expect(result.message).not.toContain('running in tmux');
  });
});

// ---------------------------------------------------------------------------
// VS Code family
// ---------------------------------------------------------------------------

function vscodeKeybindingsDir(configDir: string): string {
  if (platform() === 'win32') {
    return join(testDir, 'AppData', 'Roaming', configDir, 'User');
  }
  if (platform() === 'darwin') {
    return join(testDir, 'Library', 'Application Support', configDir, 'User');
  }
  return join(testDir, '.config', configDir, 'User');
}

describe('setupTerminal - VS Code family', () => {
  it('creates keybindings.json with a shift+enter entry', async () => {
    process.env.TERM_PROGRAM = 'vscode';
    const result = await setupTerminal();

    expect(result.success).toBe(true);
    expect(result.message).toContain('VS Code');

    const path = join(vscodeKeybindingsDir('Code'), 'keybindings.json');
    expect(existsSync(path)).toBe(true);
    const content = readFileSync(path, 'utf-8');
    expect(content).toContain('"shift+enter"');
    expect(content).toContain('workbench.action.terminal.sendSequence');
  });

  it('is idempotent when binding already exists', async () => {
    process.env.TERM_PROGRAM = 'cursor';
    const dir = vscodeKeybindingsDir('Cursor');
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, 'keybindings.json'),
      '[{"key":"shift+enter","command":"workbench.action.terminal.sendSequence"}]',
      'utf-8'
    );

    const result = await setupTerminal();
    expect(result.success).toBe(true);
    expect(result.message).toContain('already installed');
  });

  it('backs up an existing file before modifying it', async () => {
    process.env.TERM_PROGRAM = 'windsurf';
    const dir = vscodeKeybindingsDir('Windsurf');
    mkdirSync(dir, { recursive: true });
    const path = join(dir, 'keybindings.json');
    // Content that does NOT already contain the binding.
    writeFileSync(path, '[{"key":"ctrl+a","command":"foo"}]', 'utf-8');

    await setupTerminal();

    const backups = readdirSync(dir).filter((f) => f.endsWith('.bak'));
    expect(backups.length).toBe(1);
  });

  it('refuses to install over a VS Code Remote SSH session', async () => {
    process.env.TERM_PROGRAM = 'vscode';
    process.env.VSCODE_GIT_ASKPASS_MAIN =
      '/home/user/.vscode-server/bin/xxx/foo.js';

    const result = await setupTerminal();
    expect(result.success).toBe(false);
    expect(result.message).toContain('remote');
  });
});

// ---------------------------------------------------------------------------
// Alacritty
// ---------------------------------------------------------------------------

describe('setupTerminal - Alacritty', () => {
  it('creates alacritty.toml with a Shift+Return binding', async () => {
    process.env.TERM_PROGRAM = 'alacritty';
    const result = await setupTerminal();

    expect(result.success).toBe(true);
    expect(result.message).toContain('Alacritty');

    const path = join(testDir, '.config', 'alacritty', 'alacritty.toml');
    expect(existsSync(path)).toBe(true);
    const content = readFileSync(path, 'utf-8');
    expect(content).toContain('[[keyboard.bindings]]');
    expect(content).toContain('key = "Return"');
    expect(content).toContain('mods = "Shift"');
  });

  it('detects Alacritty via ALACRITTY_LOG', async () => {
    process.env.ALACRITTY_LOG = '/tmp/alacritty.log';
    const result = await setupTerminal();
    expect(result.success).toBe(true);
    expect(result.message).toContain('Alacritty');
  });

  it('is idempotent when binding already exists', async () => {
    process.env.TERM_PROGRAM = 'alacritty';
    const dir = join(testDir, '.config', 'alacritty');
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, 'alacritty.toml'),
      '[[keyboard.bindings]]\nkey = "Return"\nmods = "Shift"\nchars = "\\u001B\\r"\n',
      'utf-8'
    );

    const result = await setupTerminal();
    expect(result.success).toBe(true);
    expect(result.message).toContain('already installed');
  });
});

// ---------------------------------------------------------------------------
// Zed
// ---------------------------------------------------------------------------

describe('setupTerminal - Zed', () => {
  it('creates keymap.json with a shift-enter entry in Terminal context', async () => {
    process.env.TERM_PROGRAM = 'zed';
    const result = await setupTerminal();

    expect(result.success).toBe(true);
    expect(result.message).toContain('Zed');

    const path = join(testDir, '.config', 'zed', 'keymap.json');
    expect(existsSync(path)).toBe(true);
    const content = readFileSync(path, 'utf-8');
    expect(content).toContain('shift-enter');
    expect(content).toContain('Terminal');
    // Should be valid JSON.
    expect(() => JSON.parse(content)).not.toThrow();
  });

  it('is idempotent when binding already exists', async () => {
    process.env.TERM_PROGRAM = 'zed';
    const dir = join(testDir, '.config', 'zed');
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, 'keymap.json'),
      '[{"context":"Terminal","bindings":{"shift-enter":["terminal::SendText","\\u001b\\r"]}}]',
      'utf-8'
    );

    const result = await setupTerminal();
    expect(result.success).toBe(true);
    expect(result.message).toContain('already installed');
  });

  it('fails loud on unparseable keymap instead of overwriting', async () => {
    // If the user has JSONC comments or trailing commas, JSON.parse will
    // throw. We must not silently reset the file to just our binding —
    // that would wipe out all their other bindings.
    process.env.TERM_PROGRAM = 'zed';
    const dir = join(testDir, '.config', 'zed');
    mkdirSync(dir, { recursive: true });
    const path = join(dir, 'keymap.json');
    const original =
      '// user comment\n[\n  {"context":"Editor","bindings":{"ctrl-x":"editor::Cut"}}\n]';
    writeFileSync(path, original, 'utf-8');

    const result = await setupTerminal();
    expect(result.success).toBe(false);
    expect(result.message).toContain('parse');
    // User's original file stays intact (only the .bak copy was written).
    const after = readFileSync(path, 'utf-8');
    expect(after).toBe(original);
  });

  it('fails loud when keymap is valid JSON but not an array', async () => {
    // Top-level object or `null` is valid JSON but not a Zed keymap shape.
    // Silently overwriting it (as an earlier version of this code did)
    // would destroy the user's content.
    process.env.TERM_PROGRAM = 'zed';
    const dir = join(testDir, '.config', 'zed');
    mkdirSync(dir, { recursive: true });
    const path = join(dir, 'keymap.json');
    const original = '{"mistake": true}';
    writeFileSync(path, original, 'utf-8');

    const result = await setupTerminal();
    expect(result.success).toBe(false);
    expect(result.message).toContain("isn't a JSON array");
    // Original content must be preserved.
    expect(readFileSync(path, 'utf-8')).toBe(original);
  });

  it('treats an empty keymap.json as an empty array', async () => {
    // `touch keymap.json` or accidentally clearing the file shouldn't
    // produce a misleading "comments or trailing commas" error.
    process.env.TERM_PROGRAM = 'zed';
    const dir = join(testDir, '.config', 'zed');
    mkdirSync(dir, { recursive: true });
    const path = join(dir, 'keymap.json');
    writeFileSync(path, '', 'utf-8');

    const result = await setupTerminal();
    expect(result.success).toBe(true);
    const content = readFileSync(path, 'utf-8');
    expect(content).toContain('shift-enter');
    expect(() => JSON.parse(content)).not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// Apple Terminal
// ---------------------------------------------------------------------------

describe('setupTerminal - Apple Terminal', () => {
  it('refuses to run on non-macOS', async () => {
    if (platform() === 'darwin') {
      // Can't cleanly test the rejection path on macOS itself — skip.
      return;
    }
    process.env.TERM_PROGRAM = 'Apple_Terminal';
    const result = await setupTerminal();
    expect(result.success).toBe(false);
    expect(result.message).toContain('macOS');
  });
});

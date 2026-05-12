/**
 * `/settings terminal` — configure the user's terminal for Shift+Enter /
 * Option+Enter as newlines in the Kiro prompt.
 *
 * This module replaces the Rust implementation that used to live in
 * `crates/chat-cli-v2/src/agent/acp/commands/settings_terminal.rs`. Running
 * the setup locally in the TUI avoids an ACP round-trip and keeps this
 * TUI-specific logic out of the shared `agent` crate.
 *
 * Flow:
 *   1. {@link detectTerminal} inspects TERM_PROGRAM / TERM / KITTY_WINDOW_ID
 *      / ALACRITTY_LOG to classify the current terminal.
 *   2. A terminal-specific setup function writes the appropriate config
 *      (keybinding JSON, alacritty TOML, Zed keymap JSON, or Apple Terminal
 *      plist) — backing up any existing file first.
 *   3. If the user is running inside tmux under a native-protocol terminal
 *      (iTerm2 / Kitty / etc.), a reminder about tmux's `extended-keys` is
 *      appended to the result message.
 */

import { randomBytes } from 'crypto';
import { copyFile, mkdir, readFile, writeFile, stat } from 'fs/promises';
import { homedir, platform } from 'os';
import { dirname, join } from 'path';
import { execFile as execFileCb } from 'child_process';
import { promisify } from 'util';
import type { CommandResult } from '../types/commands.js';
import { isKitty } from './terminal-detection.js';

const execFile = promisify(execFileCb);

/**
 * Resolve the user's home directory, preferring `HOME` / `USERPROFILE` over
 * `os.homedir()` so tests (and any tool that overrides these env vars) can
 * redirect filesystem writes. Matches the convention used elsewhere in the
 * TUI (see `kiro-home.ts`).
 */
function userHome(): string {
  return process.env.HOME || process.env.USERPROFILE || homedir();
}

// --- Terminal classification ---

type Terminal =
  | { kind: 'native'; name: string }
  | {
      kind: 'vscode';
      name: 'VS Code' | 'Cursor' | 'Windsurf';
      configDir: string;
    }
  | { kind: 'alacritty' }
  | { kind: 'zed' }
  | { kind: 'apple-terminal' }
  | { kind: 'unknown'; name: string };

/** Human-readable terminal name for messaging. */
function terminalDisplayName(t: Terminal): string {
  switch (t.kind) {
    case 'native':
      return t.name;
    case 'vscode':
      return t.name;
    case 'alacritty':
      return 'Alacritty';
    case 'zed':
      return 'Zed';
    case 'apple-terminal':
      return 'Apple Terminal';
    case 'unknown':
      return t.name;
  }
}

function detectTerminal(): Terminal {
  // Kitty's protocol is auto-detected via KITTY_WINDOW_ID even when TERM_PROGRAM
  // is set to something else (e.g. tmux rewrites TERM_PROGRAM=tmux). Mirrors
  // the Rust check.
  if (isKitty()) {
    return { kind: 'native', name: 'Kitty' };
  }

  // Alacritty doesn't always set TERM_PROGRAM on macOS; fall back to env vars
  // it does set.
  if (
    'ALACRITTY_LOG' in process.env ||
    process.env.TERM?.includes('alacritty')
  ) {
    return { kind: 'alacritty' };
  }

  // Primary detection via TERM_PROGRAM.
  const termProgram = process.env.TERM_PROGRAM ?? '';
  switch (termProgram) {
    case 'iTerm.app':
      return { kind: 'native', name: 'iTerm2' };
    case 'WezTerm':
      return { kind: 'native', name: 'WezTerm' };
    case 'ghostty':
      return { kind: 'native', name: 'Ghostty' };
    case 'WarpTerminal':
      return { kind: 'native', name: 'Warp' };
    case 'alacritty':
      return { kind: 'alacritty' };
    case 'zed':
      return { kind: 'zed' };
    case 'vscode':
      return { kind: 'vscode', name: 'VS Code', configDir: 'Code' };
    case 'cursor':
      return { kind: 'vscode', name: 'Cursor', configDir: 'Cursor' };
    case 'windsurf':
      return { kind: 'vscode', name: 'Windsurf', configDir: 'Windsurf' };
    case 'Apple_Terminal':
      return { kind: 'apple-terminal' };
  }

  // Secondary detection: terminals that leak identifying env vars which
  // survive through tmux / screen (LC_* is inherited by default; the others
  // are commonly passed through). This is the primary way tmux users hit the
  // correct branch, since tmux rewrites TERM_PROGRAM to "tmux".
  if (process.env.LC_TERMINAL === 'iTerm2') {
    return { kind: 'native', name: 'iTerm2' };
  }
  if ('WEZTERM_PANE' in process.env || 'WEZTERM_EXECUTABLE' in process.env) {
    return { kind: 'native', name: 'WezTerm' };
  }
  if ('GHOSTTY_RESOURCES_DIR' in process.env) {
    return { kind: 'native', name: 'Ghostty' };
  }
  // Apple Terminal: __CFBundleIdentifier is injected by macOS for GUI-launched
  // apps. It survives through tmux because it's a plain inherited env var —
  // but that's a double-edged sword: a tmux session originally created from
  // Apple Terminal and later attached from a different terminal (say iTerm2)
  // will still carry `com.apple.Terminal`. Since routing to the wrong
  // terminal writes to the user's plist, we only trust this marker when the
  // user is NOT inside tmux. Apple Terminal + tmux users fall through to
  // unknown + tmux hint; they can exit tmux once to run the plist setup.
  if (
    !('TMUX' in process.env) &&
    process.env.__CFBundleIdentifier === 'com.apple.Terminal'
  ) {
    return { kind: 'apple-terminal' };
  }

  return {
    kind: 'unknown',
    name: termProgram || 'your current terminal',
  };
}

/**
 * Detect whether the TUI is running inside a VS Code Remote SSH session.
 * Keybinding files there belong to the remote server, not the user's local
 * machine — installing would modify the wrong file.
 */
function isVSCodeRemoteSSH(): boolean {
  const askpass = process.env.VSCODE_GIT_ASKPASS_MAIN ?? '';
  const path = process.env.PATH ?? '';
  const markers = ['.vscode-server', '.cursor-server', '.windsurf-server'];
  return markers.some((m) => askpass.includes(m) || path.includes(m));
}

// --- Result builders ---

function success(message: string): CommandResult {
  return { success: true, message };
}

function error(message: string): CommandResult {
  return { success: false, message };
}

// --- tmux note (appended only for native-protocol terminals) ---

/**
 * tmux's default behavior swallows CSI u / Kitty extended-key sequences,
 * which is how native-support terminals deliver Shift+Enter. Terminals that
 * install a keybinding file use plain `ESC CR` which tmux forwards
 * transparently, so this note is only relevant for native-support terminals.
 *
 * We only surface the hint; we don't modify the user's tmux config because
 * those setups tend to be highly personalized.
 *
 * The "detected terminal may be stale" caveat is important: tmux captures
 * env vars at session creation. A session created from terminal A and later
 * attached from terminal B still carries A's env, so our identification can
 * be wrong until the user refreshes (kill-server + re-open).
 */
const TMUX_NOTE =
  "\n\nNote: you're running in tmux. If Shift+Enter doesn't work, add the " +
  'following to your ~/.tmux.conf and reload tmux (`tmux source-file ~/.tmux.conf`):\n\n' +
  '    set -s extended-keys on\n' +
  "    set -as terminal-features 'xterm*:extkeys'\n\n" +
  '(Wrong terminal? Exit and reopen tmux from your current terminal.)';

function maybeAppendTmuxNote(result: CommandResult): CommandResult {
  if (!('TMUX' in process.env)) return result;
  return { ...result, message: result.message + TMUX_NOTE };
}

// --- execFile without throwing ---

interface ExecResult {
  code: number;
  stdout: string;
}

async function execFileNoThrow(
  cmd: string,
  args: string[]
): Promise<ExecResult> {
  try {
    const { stdout } = await execFile(cmd, args);
    return { code: 0, stdout };
  } catch (e) {
    const err = e as { code?: number; stdout?: string | Buffer };
    return {
      code: typeof err.code === 'number' ? err.code : 1,
      stdout:
        typeof err.stdout === 'string'
          ? err.stdout
          : (err.stdout?.toString('utf-8') ?? ''),
    };
  }
}

// --- JSONC array append ---

/**
 * Insert a JSON object into a JSONC array (the VS Code keybindings format)
 * while preserving any comments and whitespace. Finds the last `]` and
 * inserts before it.
 */
export function insertIntoJSONCArray(content: string, entry: string): string {
  const trimmed = content.trim();
  const closing = trimmed.lastIndexOf(']');
  if (closing === -1) {
    // Not a valid array — wrap it.
    return `[\n${entry}\n]\n`;
  }
  const before = trimmed.slice(0, closing).trimEnd();
  const needsComma = before.endsWith('}');
  const separator = needsComma ? ',\n' : '\n';
  return `${before}${separator}${entry}\n]\n`;
}

async function fileExists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

async function backupFile(path: string): Promise<void> {
  const suffix = randomBytes(4).toString('hex');
  const backup = `${path}.${suffix}.bak`;
  await copyFile(path, backup);
}

// --- Native-support terminals ---

async function setupNative(name: string): Promise<CommandResult> {
  return maybeAppendTmuxNote(
    success(`Shift+Enter is natively supported in ${name}. No setup needed.`)
  );
}

// --- VS Code / Cursor / Windsurf ---

const VSCODE_BINDING =
  '{"key":"shift+enter","command":"workbench.action.terminal.sendSequence",' +
  '"args":{"text":"\\u001b\\r"},"when":"terminalFocus"}';

function vscodeKeybindingsPath(configDir: string): string {
  const home = userHome();
  if (platform() === 'win32') {
    return join(
      home,
      'AppData',
      'Roaming',
      configDir,
      'User',
      'keybindings.json'
    );
  }
  if (platform() === 'darwin') {
    return join(
      home,
      'Library',
      'Application Support',
      configDir,
      'User',
      'keybindings.json'
    );
  }
  return join(home, '.config', configDir, 'User', 'keybindings.json');
}

async function setupVSCode(
  name: 'VS Code' | 'Cursor' | 'Windsurf',
  configDir: string
): Promise<CommandResult> {
  if (isVSCodeRemoteSSH()) {
    return error(
      `Cannot install keybindings from a remote ${name} session. Run ` +
        `/settings terminal from ${name} on your local machine.`
    );
  }

  const path = vscodeKeybindingsPath(configDir);
  await mkdir(dirname(path), { recursive: true });

  let content = '[]';
  const exists = await fileExists(path);
  if (exists) {
    content = await readFile(path, 'utf-8');
  }

  // Idempotent: skip if binding already present.
  if (
    content.includes('shift+enter') &&
    content.includes('workbench.action.terminal.sendSequence')
  ) {
    return success(
      `Shift+Enter keybinding already installed for ${name}.\nSee ${path}`
    );
  }

  if (exists) {
    await backupFile(path);
  }

  const updated = insertIntoJSONCArray(content, VSCODE_BINDING);
  await writeFile(path, updated, 'utf-8');

  return success(`Installed ${name} Shift+Enter keybinding.\nSee ${path}`);
}

// --- Alacritty ---

const ALACRITTY_BINDING =
  '[[keyboard.bindings]]\nkey = "Return"\nmods = "Shift"\nchars = "\\u001B\\r"\n';

function alacrittyConfigPaths(): string[] {
  const paths: string[] = [];
  const xdg = process.env.XDG_CONFIG_HOME;
  if (xdg) {
    paths.push(join(xdg, 'alacritty', 'alacritty.toml'));
  } else {
    paths.push(join(userHome(), '.config', 'alacritty', 'alacritty.toml'));
  }
  if (platform() === 'win32') {
    const appdata = process.env.APPDATA;
    if (appdata) {
      paths.push(join(appdata, 'alacritty', 'alacritty.toml'));
    }
  }
  return paths;
}

async function setupAlacritty(): Promise<CommandResult> {
  const candidates = alacrittyConfigPaths();
  let configPath: string | null = null;
  let content = '';
  let existed = false;

  // Prefer the first existing config; otherwise use the first candidate.
  for (const p of candidates) {
    if (await fileExists(p)) {
      configPath = p;
      content = await readFile(p, 'utf-8');
      existed = true;
      break;
    }
  }
  if (!configPath) {
    configPath = candidates[0] ?? null;
  }
  if (!configPath) {
    return error('Could not determine Alacritty config path');
  }

  // Idempotent: skip if a Shift+Return binding already exists.
  if (
    content.includes('mods = "Shift"') &&
    content.includes('key = "Return"')
  ) {
    return success(
      `Shift+Enter keybinding already installed for Alacritty.\nSee ${configPath}`
    );
  }

  if (existed) {
    await backupFile(configPath);
  } else {
    await mkdir(dirname(configPath), { recursive: true });
  }

  let updated = content;
  if (updated && !updated.endsWith('\n')) {
    updated += '\n';
  }
  updated += '\n' + ALACRITTY_BINDING;

  await writeFile(configPath, updated, 'utf-8');

  return success(
    `Installed Alacritty Shift+Enter keybinding.\n` +
      `You may need to restart Alacritty for changes to take effect.\n` +
      `See ${configPath}`
  );
}

// --- Zed ---

async function setupZed(): Promise<CommandResult> {
  const zedDir = join(userHome(), '.config', 'zed');
  const path = join(zedDir, 'keymap.json');
  await mkdir(zedDir, { recursive: true });

  let content = '[]';
  const exists = await fileExists(path);
  if (exists) {
    content = await readFile(path, 'utf-8');
  }

  if (content.includes('shift-enter')) {
    return success(
      `Shift+Enter keybinding already installed for Zed.\nSee ${path}`
    );
  }

  if (exists) {
    await backupFile(path);
  }

  // Zed's keymap.json technically supports JSONC; if the user has comments or
  // trailing commas we can't round-trip through JSON.parse without losing
  // them. Rather than silently overwrite the whole file (even though we have
  // a backup), bail out and tell the user to add the binding manually.
  //
  // An empty file is treated as an empty array (Zed considers this a valid
  // starting point). A file that parses as valid JSON but isn't an array
  // (e.g. `null` or a top-level object) is treated the same as parse failure:
  // we refuse to overwrite user content we can't meaningfully merge into.
  let parsed: Array<{
    context?: string;
    bindings?: Record<string, string | string[]>;
  }>;
  if (content.trim() === '') {
    parsed = [];
  } else {
    try {
      const raw = JSON.parse(content);
      if (!Array.isArray(raw)) {
        return error(
          `${path} exists but isn't a JSON array. ` +
            `Zed keymap should be an array of context blocks — not overwriting. ` +
            `Add this binding manually:\n\n` +
            `  { "context": "Terminal", "bindings": { "shift-enter": ["terminal::SendText", "\\u001b\\r"] } }`
        );
      }
      parsed = raw;
    } catch {
      return error(
        `Couldn't parse ${path} as JSON (it may contain comments or trailing commas). ` +
          `Add this binding manually:\n\n` +
          `  { "context": "Terminal", "bindings": { "shift-enter": ["terminal::SendText", "\\u001b\\r"] } }`
      );
    }
  }

  parsed.push({
    context: 'Terminal',
    bindings: {
      'shift-enter': ['terminal::SendText', '\u001b\r'],
    },
  });

  await writeFile(path, JSON.stringify(parsed, null, 2) + '\n', 'utf-8');

  return success(`Installed Zed Shift+Enter keybinding.\nSee ${path}`);
}

// --- Apple Terminal ---

function terminalPlistPath(): string {
  return join(userHome(), 'Library', 'Preferences', 'com.apple.Terminal.plist');
}

/**
 * Set a boolean key on a Terminal.app profile via PlistBuddy. Tries `Add`
 * first (in case the key doesn't exist), falls back to `Set` (to replace an
 * existing value). Returns whether either succeeded.
 */
async function setPlistBool(
  plist: string,
  profile: string,
  key: string,
  value: boolean
): Promise<boolean> {
  const typed = `bool ${value}`;
  const add = await execFileNoThrow('/usr/libexec/PlistBuddy', [
    '-c',
    `Add :'Window Settings':'${profile}':${key} ${typed}`,
    plist,
  ]);
  if (add.code === 0) return true;
  const set = await execFileNoThrow('/usr/libexec/PlistBuddy', [
    '-c',
    `Set :'Window Settings':'${profile}':${key} ${value}`,
    plist,
  ]);
  return set.code === 0;
}

async function readTerminalAppProfile(key: string): Promise<string | null> {
  const { code, stdout } = await execFileNoThrow('defaults', [
    'read',
    'com.apple.Terminal',
    key,
  ]);
  if (code !== 0) return null;
  const trimmed = stdout.trim();
  return trimmed.length > 0 ? trimmed : null;
}

async function setupAppleTerminal(): Promise<CommandResult> {
  if (platform() !== 'darwin') {
    return error('Terminal.app setup is only available on macOS');
  }

  const plist = terminalPlistPath();
  const backup = `${plist}.bak`;

  // Full-plist backup via `defaults export` (not a file copy) so it stays
  // consistent even with the preferences cache.
  const exportResult = await execFileNoThrow('defaults', [
    'export',
    'com.apple.Terminal',
    backup,
  ]);
  if (exportResult.code !== 0) {
    return error('Failed to back up Terminal.app preferences');
  }

  const defaultProfile = await readTerminalAppProfile(
    'Default Window Settings'
  );
  const startupProfile = await readTerminalAppProfile(
    'Startup Window Settings'
  );

  const profiles: string[] = [];
  if (defaultProfile) profiles.push(defaultProfile);
  if (startupProfile && startupProfile !== defaultProfile) {
    profiles.push(startupProfile);
  }

  if (profiles.length === 0) {
    return error('Failed to read Terminal.app profile');
  }

  let anyChange = false;
  for (const profile of profiles) {
    const metaOk = await setPlistBool(
      plist,
      profile,
      'useOptionAsMetaKey',
      true
    );
    const bellOk = await setPlistBool(plist, profile, 'Bell', false);
    if (metaOk || bellOk) {
      anyChange = true;
    }
  }

  if (!anyChange) {
    return error('Failed to configure Terminal.app profiles');
  }

  // Flush the preferences cache so Terminal.app re-reads the plist.
  await execFileNoThrow('killall', ['cfprefsd']);

  return success(
    'Configured Terminal.app:\n' +
      '- Enabled "Use Option as Meta key"\n' +
      '- Switched to visual bell\n\n' +
      'Option+Enter will now enter a newline.\n' +
      'You must restart Terminal.app for changes to take effect.'
  );
}

// --- Entry point ---

/**
 * Detect the current terminal and install the appropriate Shift+Enter
 * (or Option+Enter) binding so Kiro receives a newline instead of submit.
 *
 * Returns a {@link CommandResult} that the caller can surface as a
 * transient notification.
 */
export async function setupTerminal(): Promise<CommandResult> {
  const terminal = detectTerminal();

  switch (terminal.kind) {
    case 'native':
      return setupNative(terminal.name);
    case 'vscode':
      return setupVSCode(terminal.name, terminal.configDir);
    case 'alacritty':
      return setupAlacritty();
    case 'zed':
      return setupZed();
    case 'apple-terminal':
      return setupAppleTerminal();
    case 'unknown': {
      const base = `Shift+Enter setup isn't available for ${terminalDisplayName(terminal)} yet. You can still use Ctrl+J to insert a newline.`;
      // If we couldn't identify the terminal and the user is inside tmux,
      // tmux is probably what hid the host terminal from us (TERM_PROGRAM is
      // rewritten to 'tmux'). Point them at exiting tmux rather than
      // pretending nothing's available.
      if ('TMUX' in process.env) {
        return error(
          base +
            "\n\nIf you're in tmux, exit tmux first and re-run " +
            '/settings terminal — tmux hides the host terminal from detection.'
        );
      }
      return error(base);
    }
  }
}

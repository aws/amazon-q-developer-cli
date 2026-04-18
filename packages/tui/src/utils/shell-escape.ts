import { spawn, spawnSync, execFileSync } from 'child_process';
import {
  SHOW_CURSOR,
  HIDE_CURSOR,
  ENABLE_BRACKETED_PASTE,
} from './terminal-sequences';

/**
 * Re-enable terminal modes that a child process (vim, less, etc.) may have
 * reset. Called after executeShellEscapeTTY restores raw mode and cursor.
 *
 * Sequences:
 * - Bracketed paste (\x1b[?2004h)
 * - Kitty keyboard protocol (\x1b[>1u) — flag 1 = disambiguateEscapeCodes
 * - xterm modifyOtherKeys level 1 (\x1b[>4;1m) — fallback for non-Kitty terminals
 *
 * Sending both Kitty and modifyOtherKeys is safe: terminals ignore sequences
 * they don't support, and Kitty-capable terminals already supersede
 * modifyOtherKeys when the Kitty protocol is active.
 */
export function restoreTerminalModes(): void {
  process.stdout.write(ENABLE_BRACKETED_PASTE);
  process.stdout.write('\x1b[>1u'); // Kitty keyboard protocol (flags=1)
  process.stdout.write('\x1b[>4;1m'); // xterm modifyOtherKeys level 1
}

/**
 * Detect the appropriate shell on Windows.
 * Mirrors the Rust `detect_windows_shell()` in `agent/src/agent/util/shell.rs`:
 * checks PSModulePath to detect PowerShell sessions, preferring pwsh (7+) over
 * powershell (5.1), falling back to cmd.exe.
 */
function detectWindowsShell(): { shell: string; flag: string } {
  if (process.env.PSModulePath) {
    try {
      execFileSync('where.exe', ['pwsh'], { stdio: 'ignore' });
      return { shell: 'pwsh', flag: '-Command' };
    } catch {
      // pwsh not found, fall back to powershell 5.1
    }
    return { shell: 'powershell', flag: '-Command' };
  }
  return { shell: 'cmd', flag: '/C' };
}

/** Cached result of Windows shell detection. */
let _windowsShell: { shell: string; flag: string } | undefined;

/** Returns the shell and flag for the current platform. Cached on first call. */
function getShellAndFlag(): { shell: string; flag: string } {
  if (process.platform !== 'win32') {
    return { shell: 'bash', flag: '-c' };
  }
  _windowsShell ??= detectWindowsShell();
  return _windowsShell;
}

/** Commands that require direct terminal access (TTY) */
const TTY_COMMANDS = new Set([
  'vim',
  'vi',
  'nvim',
  'nano',
  'emacs',
  'less',
  'more',
  'most',
  'top',
  'htop',
  'btop',
  'tmux',
  'screen',
  'ssh',
]);

/** Commands that clear/reset the terminal. Handled by writing escape sequences directly. */
const CLEAR_COMMANDS = new Set(['clear', 'reset']);

export interface ShellEscapeResult {
  exitCode: number;
  error?: string;
}

/**
 * Check if a command needs direct TTY access.
 */
function needsTTY(command: string): boolean {
  const firstWord = command.trim().split(/\s/)[0] || '';
  return TTY_COMMANDS.has(firstWord);
}

/**
 * Check if a command is a terminal clear/reset.
 * Only enabled under twinki renderer for now.
 */
function isClearCommand(command: string): boolean {
  if (process.env.KIRO_RENDERER === 'ink') return false;
  const firstWord = command.trim().split(/\s/)[0] || '';
  return CLEAR_COMMANDS.has(firstWord);
}

/**
 * Execute a clear/reset by writing escape sequences directly to stdout.
 * The rendering engine detects these and does a full redraw.
 */
function executeClearCommand(): void {
  process.stdout.write('\x1b[3J\x1b[2J\x1b[H');
}

/**
 * On macOS, Bun raises RLIMIT_NOFILE to 2^63-1 which overflows Python's sh library.
 * Wrap commands with ulimit to reset to a sane value.
 */
function wrapWithFdLimit(command: string): string {
  if (process.platform === 'darwin') {
    return `ulimit -n 10240 2>/dev/null; ${command}`;
  }
  return command;
}

/**
 * Execute a TTY command with inherited stdio.
 * Used for interactive programs like vim, top, ssh.
 */
export function executeShellEscapeTTY(command: string): ShellEscapeResult {
  try {
    const wasRaw = process.stdin.isRaw;
    if (wasRaw) process.stdin.setRawMode(false);
    // Show cursor for the editor (Ink hides it)
    process.stdout.write(SHOW_CURSOR);

    // Enter alternate screen buffer so the editor doesn't pollute Ink's output
    process.stdout.write('\x1b[?1049h');

    const { shell, flag } = getShellAndFlag();
    const cmd =
      process.platform === 'win32' ? command : wrapWithFdLimit(command);
    const result = spawnSync(shell, [flag, cmd], {
      stdio: 'inherit',
      cwd: process.cwd(),
      env: process.env,
    });

    // Leave alternate screen buffer to restore Ink's output
    process.stdout.write('\x1b[?1049l');
    // Hide cursor again before returning to Ink
    process.stdout.write(HIDE_CURSOR);
    if (wasRaw) process.stdin.setRawMode(true);
    // Re-enable terminal modes the child process may have reset
    restoreTerminalModes();

    return { exitCode: result.status ?? 1, error: result.error?.message };
  } catch (err) {
    try {
      process.stdout.write('\x1b[?1049l');
      process.stdout.write(HIDE_CURSOR);
      process.stdin.setRawMode(true);
      restoreTerminalModes();
    } catch {
      // stdin may not be a TTY, ignore
    }
    return {
      exitCode: 1,
      error: err instanceof Error ? err.message : 'Unknown error',
    };
  }
}

/**
 * Execute a shell command with piped stdio, streaming output via callback.
 * Returns a promise that resolves when the command completes.
 */
export function executeShellEscapeStreaming(
  command: string,
  onData: (chunk: string) => void
): { promise: Promise<ShellEscapeResult>; kill: () => void } {
  const { shell, flag } = getShellAndFlag();
  const cmd = process.platform === 'win32' ? command : wrapWithFdLimit(command);

  const child = spawn(shell, [flag, cmd], {
    stdio: ['ignore', 'pipe', 'pipe'],
    cwd: process.cwd(),
    env: process.env,
  });

  child.stdout?.on('data', (data: Buffer) => onData(data.toString()));
  child.stderr?.on('data', (data: Buffer) => onData(data.toString()));

  const promise = new Promise<ShellEscapeResult>((resolve) => {
    child.on('error', (err) => resolve({ exitCode: 1, error: err.message }));
    child.on('close', (code) => resolve({ exitCode: code ?? 0 }));
  });

  const kill = () => {
    try {
      child.kill('SIGTERM');
    } catch {
      // process may have already exited, ignore
    }
    setTimeout(() => {
      try {
        child.kill('SIGKILL');
      } catch {
        // already dead, ignore
      }
    }, 2000);
  };

  return { promise, kill };
}

export { needsTTY, isClearCommand, executeClearCommand };

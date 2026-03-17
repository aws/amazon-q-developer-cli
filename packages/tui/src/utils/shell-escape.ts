import { spawn, spawnSync } from 'child_process';

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
  if (process.env.KIRO_RENDERER !== 'twinki') return false;
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
 * Execute a TTY command with inherited stdio.
 * Used for interactive programs like vim, top, ssh.
 */
export function executeShellEscapeTTY(command: string): ShellEscapeResult {
  try {
    const wasRaw = process.stdin.isRaw;
    if (wasRaw) process.stdin.setRawMode(false);
    // Show cursor for the editor (Ink hides it)
    process.stdout.write('\x1b[?25h');

    // Enter alternate screen buffer so the editor doesn't pollute Ink's output
    process.stdout.write('\x1b[?1049h');

    const shell = process.platform === 'win32' ? 'cmd' : 'bash';
    const args =
      process.platform === 'win32' ? ['/C', command] : ['-c', command];
    const result = spawnSync(shell, args, {
      stdio: 'inherit',
      cwd: process.cwd(),
      env: process.env,
    });

    // Leave alternate screen buffer to restore Ink's output
    process.stdout.write('\x1b[?1049l');
    // Hide cursor again before returning to Ink
    process.stdout.write('\x1b[?25l');
    if (wasRaw) process.stdin.setRawMode(true);

    return { exitCode: result.status ?? 1, error: result.error?.message };
  } catch (err) {
    try {
      process.stdout.write('\x1b[?1049l');
      process.stdout.write('\x1b[?25l');
      process.stdin.setRawMode(true);
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
  const shell = process.platform === 'win32' ? 'cmd' : 'bash';
  const args = process.platform === 'win32' ? ['/C', command] : ['-c', command];

  const child = spawn(shell, args, {
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
  };

  return { promise, kill };
}

export { needsTTY, isClearCommand, executeClearCommand };

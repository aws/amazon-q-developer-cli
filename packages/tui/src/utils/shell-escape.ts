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
 * Execute a TTY command with inherited stdio.
 * Used for interactive programs like vim, top, ssh.
 */
export function executeShellEscapeTTY(command: string): ShellEscapeResult {
  try {
    const wasRaw = process.stdin.isRaw;
    if (wasRaw) process.stdin.setRawMode(false);
    process.stdout.write('\n');

    const shell = process.platform === 'win32' ? 'cmd' : 'bash';
    const args =
      process.platform === 'win32' ? ['/C', command] : ['-c', command];
    const result = spawnSync(shell, args, {
      stdio: 'inherit',
      cwd: process.cwd(),
      env: process.env,
    });

    process.stdout.write('\n');
    if (wasRaw) process.stdin.setRawMode(true);

    return { exitCode: result.status ?? 1, error: result.error?.message };
  } catch (err) {
    try {
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

export { needsTTY };

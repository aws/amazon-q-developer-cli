import { describe, it, expect, beforeEach, afterEach, mock } from 'bun:test';
import { EventEmitter } from 'events';

// --- Mock child_process before importing the module under test ---

const mockSpawnSync = mock(
  (): { status: number | null; error?: { message: string } } => ({
    status: 0,
    error: undefined,
  })
);

function createMockChild(exitCode = 0) {
  const stdout = new EventEmitter();
  const stderr = new EventEmitter();
  const child = new EventEmitter() as EventEmitter & {
    stdout: EventEmitter;
    stderr: EventEmitter;
    kill: ReturnType<typeof mock>;
  };
  child.stdout = stdout;
  child.stderr = stderr;
  child.kill = mock(() => {});

  // Schedule close event
  setTimeout(() => child.emit('close', exitCode), 10);

  return child;
}

let currentMockChild: ReturnType<typeof createMockChild>;
const mockSpawn = mock(() => {
  currentMockChild = createMockChild(0);
  return currentMockChild;
});

mock.module('child_process', () => ({
  spawn: mockSpawn,
  spawnSync: mockSpawnSync,
  execFileSync: mock(() => ''),
}));

const {
  needsTTY,
  isClearCommand,
  executeClearCommand,
  executeShellEscapeTTY,
  executeShellEscapeStreaming,
  restoreTerminalModes,
} = await import('../shell-escape');

let writtenData: string[];
let originalWrite: typeof process.stdout.write;
let savedEnv: NodeJS.ProcessEnv;
let originalSetRawMode: typeof process.stdin.setRawMode;
let originalIsRaw: boolean | undefined;
let originalPlatform: string;

beforeEach(() => {
  writtenData = [];
  originalWrite = process.stdout.write;
  process.stdout.write = ((chunk: any) => {
    writtenData.push(String(chunk));
    return true;
  }) as any;

  savedEnv = { ...process.env };
  delete process.env.KIRO_RENDERER;

  originalIsRaw = process.stdin.isRaw;
  originalSetRawMode = process.stdin.setRawMode;

  Object.defineProperty(process.stdin, 'isRaw', {
    value: false,
    writable: true,
    configurable: true,
  });
  (process.stdin as any).setRawMode = mock(() => process.stdin);

  originalPlatform = process.platform;

  mockSpawnSync.mockReset();
  mockSpawnSync.mockImplementation(() => ({ status: 0, error: undefined }));
  mockSpawn.mockReset();
  mockSpawn.mockImplementation(() => {
    currentMockChild = createMockChild(0);
    return currentMockChild;
  });
});

afterEach(() => {
  process.stdout.write = originalWrite;
  process.env = savedEnv;
  Object.defineProperty(process.stdin, 'isRaw', {
    value: originalIsRaw,
    writable: true,
    configurable: true,
  });
  (process.stdin as any).setRawMode = originalSetRawMode;
  Object.defineProperty(process, 'platform', {
    value: originalPlatform,
    configurable: true,
  });
});

describe('needsTTY', () => {
  it('returns true for vim', () => {
    expect(needsTTY('vim')).toBe(true);
  });

  it('returns true for vi', () => {
    expect(needsTTY('vi')).toBe(true);
  });

  it('returns true for nvim', () => {
    expect(needsTTY('nvim')).toBe(true);
  });

  it('returns true for nano', () => {
    expect(needsTTY('nano')).toBe(true);
  });

  it('returns true for ssh', () => {
    expect(needsTTY('ssh')).toBe(true);
  });

  it('returns true for top, htop, btop', () => {
    expect(needsTTY('top')).toBe(true);
    expect(needsTTY('htop')).toBe(true);
    expect(needsTTY('btop')).toBe(true);
  });

  it('returns true for tmux and screen', () => {
    expect(needsTTY('tmux')).toBe(true);
    expect(needsTTY('screen')).toBe(true);
  });

  it('returns true for less, more, most', () => {
    expect(needsTTY('less')).toBe(true);
    expect(needsTTY('more')).toBe(true);
    expect(needsTTY('most')).toBe(true);
  });

  it('returns true for emacs', () => {
    expect(needsTTY('emacs')).toBe(true);
  });

  it('returns false for ls, echo, cat, grep, pwd', () => {
    expect(needsTTY('ls')).toBe(false);
    expect(needsTTY('echo')).toBe(false);
    expect(needsTTY('cat')).toBe(false);
    expect(needsTTY('grep')).toBe(false);
    expect(needsTTY('pwd')).toBe(false);
  });

  it('handles commands with arguments', () => {
    expect(needsTTY('vim file.txt')).toBe(true);
    expect(needsTTY('ssh user@host')).toBe(true);
    expect(needsTTY('ls -la')).toBe(false);
  });

  it('handles empty and whitespace strings', () => {
    expect(needsTTY('')).toBe(false);
    expect(needsTTY('   ')).toBe(false);
  });
});

describe('isClearCommand', () => {
  it('returns true for "clear"', () => {
    expect(isClearCommand('clear')).toBe(true);
  });

  it('returns true for "reset"', () => {
    expect(isClearCommand('reset')).toBe(true);
  });

  it('returns false for non-clear commands', () => {
    expect(isClearCommand('ls')).toBe(false);
    expect(isClearCommand('echo hello')).toBe(false);
  });

  it('handles leading whitespace', () => {
    expect(isClearCommand('  clear')).toBe(true);
  });
});

describe('executeClearCommand', () => {
  it('writes the correct escape sequence', () => {
    executeClearCommand();
    expect(writtenData).toEqual(['\x1b[3J\x1b[2J\x1b[H']);
  });
});

describe('restoreTerminalModes', () => {
  it('writes bracketed paste, kitty keyboard, and modifyOtherKeys sequences', () => {
    restoreTerminalModes();
    expect(writtenData).toContain('\x1b[?2004h'); // ENABLE_BRACKETED_PASTE
    expect(writtenData).toContain('\x1b[>1u'); // Kitty keyboard protocol
    expect(writtenData).toContain('\x1b[>4;1m'); // modifyOtherKeys level 1
  });
});

describe('executeShellEscapeTTY', () => {
  it('returns exit code from spawnSync', () => {
    mockSpawnSync.mockImplementation(() => ({
      status: 0,
      error: undefined,
    }));
    const result = executeShellEscapeTTY('echo hello');
    expect(result.exitCode).toBe(0);
  });

  it('returns non-zero exit code', () => {
    mockSpawnSync.mockImplementation(() => ({
      status: 127,
      error: undefined,
    }));
    const result = executeShellEscapeTTY('nonexistent');
    expect(result.exitCode).toBe(127);
  });

  it('returns exit code 1 when status is null', () => {
    mockSpawnSync.mockImplementation(() => ({
      status: null,
      error: undefined,
    }));
    const result = executeShellEscapeTTY('echo hello');
    expect(result.exitCode).toBe(1);
  });

  it('returns error message from spawnSync', () => {
    mockSpawnSync.mockImplementation(() => ({
      status: 1,
      error: { message: 'spawn ENOENT' },
    }));
    const result = executeShellEscapeTTY('badcmd');
    expect(result.error).toBe('spawn ENOENT');
  });

  it('writes cursor show/hide and alt screen sequences', () => {
    mockSpawnSync.mockImplementation(() => ({
      status: 0,
      error: undefined,
    }));
    executeShellEscapeTTY('echo hello');
    // Should show cursor before spawn
    expect(writtenData).toContain('\x1b[?25h');
    // Should enter alternate screen
    expect(writtenData).toContain('\x1b[?1049h');
    // Should leave alternate screen after spawn
    expect(writtenData).toContain('\x1b[?1049l');
    // Should hide cursor after spawn
    expect(writtenData).toContain('\x1b[?25l');
  });

  it('restores raw mode if it was enabled', () => {
    Object.defineProperty(process.stdin, 'isRaw', {
      value: true,
      writable: true,
      configurable: true,
    });
    mockSpawnSync.mockImplementation(() => ({
      status: 0,
      error: undefined,
    }));
    executeShellEscapeTTY('echo hello');
    // setRawMode should be called with false (before) and true (after)
    const setRawModeCalls = (process.stdin.setRawMode as any).mock.calls;
    expect(setRawModeCalls.length).toBeGreaterThanOrEqual(2);
    expect(setRawModeCalls[0][0]).toBe(false);
    expect(setRawModeCalls[setRawModeCalls.length - 1][0]).toBe(true);
  });

  it('handles exception from spawnSync gracefully', () => {
    mockSpawnSync.mockImplementation(() => {
      throw new Error('catastrophic failure');
    });
    const result = executeShellEscapeTTY('echo hello');
    expect(result.exitCode).toBe(1);
    expect(result.error).toBe('catastrophic failure');
  });
});

describe('executeShellEscapeStreaming', () => {
  beforeEach(() => {
    Object.defineProperty(process, 'platform', {
      value: 'win32',
      configurable: true,
    });
  });

  afterEach(() => {
    Object.defineProperty(process, 'platform', {
      value: originalPlatform,
      configurable: true,
    });
  });

  it('returns promise and kill function', () => {
    const { promise, kill } = executeShellEscapeStreaming(
      'echo hello',
      () => {}
    );
    expect(promise).toBeInstanceOf(Promise);
    expect(typeof kill).toBe('function');
  });

  it('promise resolves with exit code', async () => {
    const { promise } = executeShellEscapeStreaming('echo hello', () => {});
    const result = await promise;
    expect(result.exitCode).toBe(0);
  });

  it('streams stdout data via onData callback', async () => {
    const chunks: string[] = [];
    const { promise } = executeShellEscapeStreaming('echo hello', (chunk) => {
      chunks.push(chunk);
    });

    // Emit some data on the mock child stdout
    currentMockChild.stdout.emit('data', Buffer.from('hello world'));

    await promise;
    expect(chunks).toContain('hello world');
  });

  it('streams stderr data via onData callback', async () => {
    const chunks: string[] = [];
    const { promise } = executeShellEscapeStreaming('echo hello', (chunk) => {
      chunks.push(chunk);
    });

    currentMockChild.stderr.emit('data', Buffer.from('error output'));

    await promise;
    expect(chunks).toContain('error output');
  });

  it('kill function calls child.kill', () => {
    const { kill } = executeShellEscapeStreaming('long-running', () => {});
    kill();
    expect(currentMockChild.kill).toHaveBeenCalled();
  });

  it('resolves with error when spawn emits error', async () => {
    mockSpawn.mockImplementation(() => {
      const stdout = new EventEmitter();
      const stderr = new EventEmitter();
      const child = new EventEmitter() as EventEmitter & {
        stdout: EventEmitter;
        stderr: EventEmitter;
        kill: ReturnType<typeof mock>;
      };
      child.stdout = stdout;
      child.stderr = stderr;
      child.kill = mock(() => {});
      // Emit error instead of close
      setTimeout(() => child.emit('error', new Error('spawn error')), 10);
      currentMockChild = child;
      return child;
    });

    const { promise } = executeShellEscapeStreaming('bad-cmd', () => {});
    const result = await promise;
    expect(result.exitCode).toBe(1);
    expect(result.error).toBe('spawn error');
  });
});

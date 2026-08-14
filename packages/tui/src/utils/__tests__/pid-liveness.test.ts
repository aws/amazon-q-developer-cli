import { describe, expect, it } from 'bun:test';
import { isLockOwnerProcessName, isPidAlive } from '../pid-liveness';

describe('isLockOwnerProcessName', () => {
  it('accepts the binaries that take session locks', () => {
    expect(isLockOwnerProcessName('kiro-cli')).toBe(true);
    expect(isLockOwnerProcessName('chat_cli')).toBe(true);
    expect(isLockOwnerProcessName('bun')).toBe(true);
    expect(isLockOwnerProcessName('/usr/local/bin/kiro-cli\n')).toBe(true);
  });

  it('rejects unrelated binaries', () => {
    expect(isLockOwnerProcessName('sleep')).toBe(false);
    expect(isLockOwnerProcessName('/usr/bin/postgres')).toBe(false);
    expect(isLockOwnerProcessName('')).toBe(false);
  });
});

describe('isPidAlive', () => {
  it('reports this process alive', () => {
    expect(isPidAlive(process.pid)).toBe(true);
  });

  it('reports a dead pid as not alive', () => {
    // Above the default pid_max on the platforms we run on.
    expect(isPidAlive(4_194_304, () => 'kiro-cli')).toBe(false);
  });

  it.skipIf(process.platform === 'win32')(
    'treats a recycled pid owned by an unrelated binary as stale',
    () => {
      expect(isPidAliveForeign(process.pid, 'sleep')).toBe(false);
      expect(isPidAliveForeign(process.pid, 'kiro-cli')).toBe(true);
    }
  );

  it.skipIf(process.platform === 'win32')(
    'keeps the lock when the name cannot be read',
    () => {
      expect(isPidAliveForeign(process.pid, '')).toBe(true);
    }
  );
});

/**
 * isPidAlive short-circuits for our own pid, so foreign-owner behavior is
 * exercised through a pid that is alive but reported under another name.
 */
function isPidAliveForeign(livePid: number, name: string): boolean {
  const original = process.pid;
  Object.defineProperty(process, 'pid', { value: original + 1_000_000 });
  try {
    return isPidAlive(livePid, () => name);
  } finally {
    Object.defineProperty(process, 'pid', { value: original });
  }
}

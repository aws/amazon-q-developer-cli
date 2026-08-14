import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  rmSync,
  existsSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { writeFileFully } from '../bounded-json';
import {
  acquireSessionLock,
  beginSessionLockTransfer,
  commitSessionLockTransfer,
  releaseSessionLock,
  rollbackSessionLockTransfer,
  isSessionLocked,
} from '../session-lock';

let root: string;
let priorRoot: string | undefined;

function kasDir(id: string, hash = 'hash1'): string {
  const dir = join(root, hash, `sess_${id}`);
  mkdirSync(dir, { recursive: true });
  writeFileSync(
    join(dir, 'session.json'),
    JSON.stringify({ id: `sess_${id}` })
  );
  return dir;
}

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'session-lock-'));
  priorRoot = process.env.KIRO_TEST_SESSIONS_ROOT;
  process.env.KIRO_TEST_SESSIONS_ROOT = root;
});

afterEach(() => {
  releaseSessionLock();
  if (priorRoot === undefined) delete process.env.KIRO_TEST_SESSIONS_ROOT;
  else process.env.KIRO_TEST_SESSIONS_ROOT = priorRoot;
  rmSync(root, { recursive: true, force: true });
});

describe('complete descriptor writes', () => {
  it('retries positive short writes until the payload is complete', () => {
    const source = Buffer.from('complete lock payload');
    const target = Buffer.alloc(source.length);
    let targetOffset = 0;
    let calls = 0;

    writeFileFully(123, source, (_fd, buffer, offset, length) => {
      const written = Math.min(3, length);
      buffer.copy(target, targetOffset, offset, offset + written);
      targetOffset += written;
      calls++;
      return written;
    });

    expect(target).toEqual(source);
    expect(calls).toBeGreaterThan(1);
  });

  it('fails instead of publishing a partial payload when writes stop', () => {
    expect(() => writeFileFully(123, Buffer.from('payload'), () => 0)).toThrow(
      'Unable to complete descriptor write'
    );
  });
});

describe('acquireSessionLock', () => {
  it('writes a lock file into the session dir', () => {
    const dir = kasDir('a');
    acquireSessionLock('a');
    expect(existsSync(join(dir, '.lock'))).toBe(true);
  });

  it('writes lock files into every physical session copy', () => {
    const first = kasDir('a', 'hash1');
    const second = kasDir('a', 'hash2');

    acquireSessionLock('a');

    expect(existsSync(join(first, '.lock'))).toBe(true);
    expect(existsSync(join(second, '.lock'))).toBe(true);
    releaseSessionLock();
    expect(existsSync(join(first, '.lock'))).toBe(false);
    expect(existsSync(join(second, '.lock'))).toBe(false);
  });

  it('detects a lock held on a non-first physical copy', () => {
    kasDir('a', 'hash1');
    const second = kasDir('a', 'hash2');
    writeFileSync(
      join(second, '.lock'),
      JSON.stringify({ pid: 1, startedAt: new Date().toISOString() })
    );

    expect(isSessionLocked('a')).toMatchObject({ locked: true, pid: 1 });
  });

  it('writes started_at (snake_case) and reads both key spellings', () => {
    // The Rust agent parses these lock files with a snake_case schema; a
    // camelCase-only lock reads to it as unlocked.
    const dir = kasDir('interop');
    acquireSessionLock('interop');
    const written = JSON.parse(readFileSync(join(dir, '.lock'), 'utf-8'));
    expect(written.started_at).toBeString();
    expect(written.startedAt).toBeUndefined();
    releaseSessionLock();

    const stamp = new Date().toISOString();
    writeFileSync(
      join(dir, '.lock'),
      JSON.stringify({ pid: 1, started_at: stamp })
    );
    expect(isSessionLocked('interop')).toMatchObject({
      locked: true,
      pid: 1,
      startedAt: stamp,
    });
  });

  it('checks only the selected physical store and skips remote rows', () => {
    const dir = kasDir('shared');
    writeFileSync(
      join(dir, '.lock'),
      JSON.stringify({ pid: 1, startedAt: new Date().toISOString() })
    );

    expect(
      isSessionLocked('shared', { engine: 'v3', source: 'local' })
    ).toMatchObject({ locked: true, pid: 1 });
    expect(
      isSessionLocked('shared', { engine: 'v2', source: 'local' })
    ).toBeNull();
    expect(
      isSessionLocked('shared', { engine: 'v3', source: 'remote' })
    ).toBeNull();
  });

  it('rolls back earlier copy locks when a later copy cannot be acquired', () => {
    const first = kasDir('a', 'hash1');
    const second = kasDir('a', 'hash2');
    writeFileSync(
      join(second, '.lock'),
      JSON.stringify({ pid: 1, startedAt: new Date().toISOString() })
    );

    expect(() => acquireSessionLock('a')).toThrow('another terminal');
    expect(existsSync(join(first, '.lock'))).toBe(false);
  });

  it('probing your own lock reads as unlocked (same pid re-acquires)', () => {
    kasDir('a');
    acquireSessionLock('a');
    expect(isSessionLocked('a')).toBeNull();
  });

  it('throws when another live process holds the lock', () => {
    const dir = kasDir('a');
    // PID 1 is always alive (EPERM from the probe means alive).
    writeFileSync(
      join(dir, '.lock'),
      JSON.stringify({ pid: 1, startedAt: new Date().toISOString() })
    );
    expect(() => acquireSessionLock('a')).toThrow('another terminal');
  });

  it('steals a dead-PID lock', () => {
    const dir = kasDir('a');
    writeFileSync(
      join(dir, '.lock'),
      JSON.stringify({ pid: 999999999, startedAt: '' })
    );
    acquireSessionLock('a');
    expect(isSessionLocked('a')).toBeNull(); // ours now
  });

  it('fails closed for malformed and partial lock files', () => {
    const dir = kasDir('a');
    writeFileSync(join(dir, '.lock'), '{"pid":');
    expect(isSessionLocked('a')).toEqual({
      locked: true,
      startedAt: '',
      state: 'unknown',
    });
    expect(() => acquireSessionLock('a')).toThrow('malformed or unreadable');
  });

  it('recovers an empty acquisition directory abandoned by a crash', () => {
    const dir = kasDir('a');
    const lockPath = join(dir, '.lock');
    const guardDir = `${lockPath}.acquire`;
    mkdirSync(guardDir);

    acquireSessionLock('a');

    expect(existsSync(lockPath)).toBe(true);
    expect(existsSync(guardDir)).toBe(false);
  });

  it('reclaims a dead acquisition contender', () => {
    const dir = kasDir('a');
    const guardDir = `${join(dir, '.lock')}.acquire`;
    mkdirSync(guardDir);
    writeFileSync(
      join(guardDir, 'dead.lock'),
      JSON.stringify({ pid: 999999999, startedAt: '' })
    );

    acquireSessionLock('a');

    expect(existsSync(guardDir)).toBe(false);
  });

  it('rejects an acquisition guard held by a live process', () => {
    const dir = kasDir('a');
    const guardDir = `${join(dir, '.lock')}.acquire`;
    mkdirSync(guardDir);
    writeFileSync(
      join(guardDir, 'live.lock'),
      JSON.stringify({ pid: 1, startedAt: new Date().toISOString() })
    );

    expect(() => acquireSessionLock('a')).toThrow(
      'acquisition is already in progress'
    );
    expect(existsSync(guardDir)).toBe(true);
  });

  it('fails closed for a malformed acquisition guard', () => {
    const dir = kasDir('a');
    const guardDir = `${join(dir, '.lock')}.acquire`;
    mkdirSync(guardDir);
    writeFileSync(join(guardDir, 'malformed.lock'), '{"pid":');

    expect(() => acquireSessionLock('a')).toThrow('malformed or unreadable');
    expect(existsSync(guardDir)).toBe(true);
  });

  it('rolls back a transfer without releasing the active session', () => {
    const dirA = kasDir('a');
    const dirB = kasDir('b');
    acquireSessionLock('a');

    const transfer = beginSessionLockTransfer('b');
    expect(existsSync(join(dirA, '.lock'))).toBe(true);
    expect(existsSync(join(dirB, '.lock'))).toBe(true);

    rollbackSessionLockTransfer(transfer);
    expect(existsSync(join(dirA, '.lock'))).toBe(true);
    expect(existsSync(join(dirB, '.lock'))).toBe(false);
  });

  it('commits a transfer only after the caller reports load success', () => {
    const dirA = kasDir('a');
    const dirB = kasDir('b');
    acquireSessionLock('a');

    const transfer = beginSessionLockTransfer('b');
    commitSessionLockTransfer(transfer);

    expect(existsSync(join(dirA, '.lock'))).toBe(false);
    expect(existsSync(join(dirB, '.lock'))).toBe(true);
  });

  it('keeps the active lock when a remote transfer ignores a same-id local copy', () => {
    const dirA = kasDir('a');
    const localCollision = kasDir('remote-only');
    writeFileSync(
      join(localCollision, '.lock'),
      JSON.stringify({ pid: 1, startedAt: new Date().toISOString() })
    );
    acquireSessionLock('a');

    const transfer = beginSessionLockTransfer('remote-only', 'remote');
    rollbackSessionLockTransfer(transfer);

    expect(existsSync(join(dirA, '.lock'))).toBe(true);
    expect(existsSync(join(localCollision, '.lock'))).toBe(true);
  });

  it('switching sessions swaps the held lock', () => {
    const dirA = kasDir('a');
    const dirB = kasDir('b');
    acquireSessionLock('a');
    acquireSessionLock('b');
    expect(existsSync(join(dirA, '.lock'))).toBe(false);
    expect(existsSync(join(dirB, '.lock'))).toBe(true);
  });

  it('switching to a session with NO local KAS dir releases the held lock', () => {
    // Cloud/remote sessions have no local dir; holding the previous
    // session's lock would block its resume/delete from other terminals
    // for the process lifetime.
    const dirA = kasDir('a');
    acquireSessionLock('a');
    expect(existsSync(join(dirA, '.lock'))).toBe(true);
    acquireSessionLock('cloud-only-session');
    expect(existsSync(join(dirA, '.lock'))).toBe(false);
  });
});

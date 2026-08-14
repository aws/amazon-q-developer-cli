/**
 * Whether a PID from a session lock belongs to a live Kiro process.
 *
 * A bare signal-0 probe cannot tell a live owner from a recycled PID now used
 * by something unrelated, and that is the one lock failure mode with no
 * self-healing: the lock reads live forever, so resume and delete refuse
 * until the unrelated process exits. Matching the owner's process name
 * against the binaries that take these locks lets a recycled PID go stale.
 *
 * The verdict is deliberately NOT cached: a PID can change owner within one
 * TUI lifetime, and a remembered "stale" would then let a live lock be taken
 * over. Callers are per-keypress or bounded loops that already do file I/O,
 * so the name lookup is affordable. Windows is exempt (its handle-based
 * liveness check has no name lookup either), and a name that cannot be read
 * keeps the lock — fail closed.
 */
import { spawnSync } from 'node:child_process';

const LOCK_OWNER_NAMES = ['kiro-cli', 'chat_cli', 'bun', 'node'];

/** True when a process name is one of the binaries that hold these locks. */
export function isLockOwnerProcessName(name: string): boolean {
  const base = name.trim().split('/').pop() ?? '';
  return LOCK_OWNER_NAMES.some((owner) => base.startsWith(owner));
}

/** Reads a process name, or '' when it cannot be determined. */
export type ProcessNameProbe = (pid: number) => string;

const defaultProbe: ProcessNameProbe = (pid) => {
  try {
    const result = spawnSync('ps', ['-o', 'comm=', '-p', String(pid)], {
      encoding: 'utf-8',
      timeout: 2000,
    });
    return result.status === 0 ? result.stdout : '';
  } catch {
    return '';
  }
};

export function isPidAlive(
  pid: number,
  probe: ProcessNameProbe = defaultProbe
): boolean {
  if (pid === process.pid) return true;
  try {
    process.kill(pid, 0);
  } catch (err) {
    // EPERM: the process exists but belongs to another user.
    if ((err as NodeJS.ErrnoException).code !== 'EPERM') return false;
  }
  if (process.platform === 'win32') return true;
  const name = probe(pid);
  return name.trim() ? isLockOwnerProcessName(name) : true;
}

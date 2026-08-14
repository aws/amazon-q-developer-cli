/**
 * A real child process whose pid passes the lock-owner liveness check.
 *
 * Lock tests need a pid that is alive, is not this process, and resolves to
 * an allowlisted binary name under the real `ps` probe. A sentinel like pid 1
 * fails that last requirement (it is systemd/launchd), so tests spawn a bun
 * child that sleeps until killed. Spawned via the Bun global rather than
 * node:child_process so a process-global module mock cannot hand back a fake
 * child with a fabricated pid.
 */
export interface LiveLockOwner {
  pid: number;
  stop(): void;
}

export function spawnLiveLockOwner(): LiveLockOwner {
  const child = Bun.spawn(
    [process.execPath, '-e', 'setTimeout(() => {}, 120000)'],
    {
      stdin: 'ignore',
      stdout: 'ignore',
      stderr: 'ignore',
    }
  );
  return {
    pid: child.pid,
    stop() {
      try {
        child.kill(9);
      } catch {
        /* already exited */
      }
    },
  };
}

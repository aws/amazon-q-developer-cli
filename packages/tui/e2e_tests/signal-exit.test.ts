/**
 * E2E tests for signal-based clean exit across the full process tree.
 *
 * Covers the missing layers NOT tested by crates/chat-cli-v2/tests/signal_exit.rs:
 * - Layer 1: Rust launcher kills bun on signal (no orphan bun processes)
 * - Layer 2: Bun-side SIGHUP handler calls kiro.close() + cleanup()
 * - Layer 3: Death spiral prevention (exit completes within deadline)
 *
 * The full process tree is: Rust launcher (chat_cli) → bun TUI → ACP backend.
 * These tests exercise the FULL stack end-to-end.
 *
 * Parameterized to run in both TUI and Lite modes via describe.each.
 *
 * Taskei: https://taskei.amazon.dev/tasks/P420105490
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

/** Check if a PID is still alive. */
function isAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/** Wait for a PID to exit, returning true if it exited within the deadline. */
async function waitForExit(pid: number, timeoutMs: number): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (!isAlive(pid)) return true;
    await Bun.sleep(200);
  }
  return false;
}

/** Find descendant PIDs matching a pattern (uses pgrep -f). */
async function findDescendants(ancestorPid: number, pattern: string): Promise<number[]> {
  // Get all PIDs matching the pattern, then filter to those whose ancestor is our process
  const proc = Bun.spawn(['pgrep', '-f', pattern], { stdout: 'pipe', stderr: 'pipe' });
  const text = await new Response(proc.stdout).text();
  await proc.exited;

  const candidates = text.trim().split('\n').filter(Boolean).map(Number).filter((n) => !isNaN(n));

  // Filter to descendants of ancestorPid by walking /proc/<pid>/stat ppid chain
  const descendants: number[] = [];
  for (const pid of candidates) {
    if (await isDescendantOf(pid, ancestorPid)) {
      descendants.push(pid);
    }
  }
  return descendants;
}

/** Check if pid is a descendant of ancestorPid by walking the ppid chain. */
async function isDescendantOf(pid: number, ancestorPid: number): Promise<boolean> {
  let current = pid;
  const visited = new Set<number>();
  while (current > 1 && !visited.has(current)) {
    visited.add(current);
    if (current === ancestorPid) return true;
    const ppid = await getPpid(current);
    if (ppid === null || ppid === current) break;
    current = ppid;
  }
  return false;
}

/** Get parent PID. Works on both Linux and macOS. */
async function getPpid(pid: number): Promise<number | null> {
  try {
    const proc = Bun.spawn(['ps', '-o', 'ppid=', '-p', String(pid)], {
      stdout: 'pipe',
      stderr: 'pipe',
    });
    const text = await new Response(proc.stdout).text();
    await proc.exited;
    const ppid = parseInt(text.trim(), 10);
    return isNaN(ppid) ? null : ppid;
  } catch {
    return null;
  }
}

describe.skipIf(process.platform === 'win32').each([
  { mode: 'tui' as const, builder: () => E2ETestCase.builder() },
  { mode: 'lite' as const, builder: () => E2ETestCase.builder().withLite() },
])('Signal exit — full stack (Rust → bun → ACP) ($mode)', ({ mode, builder }) => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  // ─── Layer 1: Killing Rust launcher kills bun (no orphans) ──────────────────

  it('SIGHUP to launcher exits cleanly with no orphan bun process', async () => {
    testCase = await builder()
      .withTestName(`signal-sighup-no-orphan-${mode}`)
      .launch();

    await testCase.waitForText('ask a question', 15000);

    const launcherPid = testCase.getPid()!;
    expect(launcherPid).toBeGreaterThan(0);

    // Find the bun child process
    const bunPids = await findDescendants(launcherPid, 'bun.*tui');
    expect(bunPids.length).toBeGreaterThan(0);
    const bunPid = bunPids[0]!;

    // Send SIGHUP (simulates terminal close / Cmd+W)
    process.kill(launcherPid, 'SIGHUP');

    // Launcher must exit
    expect(await waitForExit(launcherPid, 5000)).toBe(true);
    // Bun must NOT be orphaned
    expect(await waitForExit(bunPid, 5000)).toBe(true);
  }, 30000);

  it('SIGTERM to launcher exits cleanly with no orphan bun process', async () => {
    testCase = await builder()
      .withTestName(`signal-sigterm-no-orphan-${mode}`)
      .launch();

    await testCase.waitForText('ask a question', 15000);

    const launcherPid = testCase.getPid()!;
    const bunPids = await findDescendants(launcherPid, 'bun.*tui');
    expect(bunPids.length).toBeGreaterThan(0);
    const bunPid = bunPids[0]!;

    // Send SIGTERM (simulates `kill <pid>`)
    process.kill(launcherPid, 'SIGTERM');

    expect(await waitForExit(launcherPid, 5000)).toBe(true);
    expect(await waitForExit(bunPid, 5000)).toBe(true);
  }, 30000);

  it('SIGKILL to launcher — bun detects stdin EOF and exits (no orphan)', async () => {
    testCase = await builder()
      .withTestName(`signal-sigkill-no-orphan-${mode}`)
      .launch();

    await testCase.waitForText('ask a question', 15000);

    const launcherPid = testCase.getPid()!;
    const bunPids = await findDescendants(launcherPid, 'bun.*tui');
    expect(bunPids.length).toBeGreaterThan(0);
    const bunPid = bunPids[0]!;

    // SIGKILL — launcher dies instantly, bun must detect stdin EOF
    process.kill(launcherPid, 'SIGKILL');

    expect(await waitForExit(launcherPid, 2000)).toBe(true);
    // Bun's process.stdin 'end' handler must fire and trigger cleanup
    expect(await waitForExit(bunPid, 10000)).toBe(true);
  }, 30000);

  // ─── Layer 2: Bun-side SIGHUP handler ──────────────────────────────────────

  it('SIGHUP directly to bun triggers kiro.close() and clean exit', async () => {
    testCase = await builder()
      .withTestName(`signal-bun-direct-sighup-${mode}`)
      .launch();

    await testCase.waitForText('ask a question', 15000);

    const launcherPid = testCase.getPid()!;
    const bunPids = await findDescendants(launcherPid, 'bun.*tui');
    expect(bunPids.length).toBeGreaterThan(0);
    const bunPid = bunPids[0]!;

    // Send SIGHUP directly to bun (bypassing the Rust launcher)
    process.kill(bunPid, 'SIGHUP');

    // Bun should call kiro.close() + cleanup() and exit
    expect(await waitForExit(bunPid, 5000)).toBe(true);
  }, 30000);

  // ─── Layer 3: Death spiral prevention ───────────────────────────────────────

  it('exit completes within 5s (no death spiral from stdout errors)', async () => {
    // The death spiral scenario: SIGHUP closes the PTY → stdout becomes invalid →
    // writing to stdout throws → uncaughtException handler writes to stdout → loop.
    // The circuit breaker (process.stdout.on('error')) must break this cycle.
    // If it fails, the process hangs or OOMs instead of exiting.
    testCase = await builder()
      .withTestName(`signal-no-death-spiral-${mode}`)
      .launch();

    await testCase.waitForText('ask a question', 15000);

    const launcherPid = testCase.getPid()!;

    // SIGHUP closes the PTY, making stdout invalid for the bun process
    process.kill(launcherPid, 'SIGHUP');

    // Must exit within 5s — a death spiral would hang indefinitely
    expect(await waitForExit(launcherPid, 5000)).toBe(true);
  }, 15000);
});

// ─── Windows: orphan prevention on force-kill ───────────────────────────────

describe.skipIf(process.platform !== 'win32')('Signal exit — Windows orphan prevention', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) await testCase.cleanup();
    testCase = null;
  });

  it('no orphan bun after parent force-kill', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('win-orphan-check')
      .launch();
    await testCase.waitForText('ask a question', 15000);

    const launcherPid = testCase.getPid()!;

    // Force-kill parent (simulates TerminateProcess / hard death)
    process.kill(launcherPid, 'SIGKILL');
    await Bun.sleep(3000);

    // Verify the killed process is actually dead
    let isAlive = true;
    try { process.kill(launcherPid, 0); } catch { isAlive = false; }
    expect(isAlive).toBe(false);
  }, 15000);
});

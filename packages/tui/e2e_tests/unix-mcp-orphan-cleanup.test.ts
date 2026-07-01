/**
 * E2E tests for Unix MCP process group cleanup.
 *
 * Validates that MCP server child processes are killed when the TUI process
 * is force-killed (simulating a crash). This is the Unix-side counterpart to
 * windows-mcp-no-console.test.ts — it ensures the `detached: true` +
 * process-group kill mechanism actually prevents orphan processes.
 *
 * The fix in P460297924 makes `detached: true` Unix-only. These tests ensure
 * the orphan-prevention behavior is preserved on Unix after the Windows fix.
 *
 * Ticket: https://t.corp.amazon.com/P460297924
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

/** Find PIDs matching a pattern via pgrep. */
async function findProcessesByPattern(pattern: string): Promise<number[]> {
  const proc = Bun.spawn(['pgrep', '-f', pattern], {
    stdout: 'pipe',
    stderr: 'pipe',
  });
  const text = await new Response(proc.stdout).text();
  await proc.exited;
  return text
    .trim()
    .split('\n')
    .filter(Boolean)
    .map(Number)
    .filter((n) => !isNaN(n));
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

/** Check if pid is a descendant of ancestorPid by walking the ppid chain. */
async function isDescendantOf(
  pid: number,
  ancestorPid: number
): Promise<boolean> {
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

/** Find descendant PIDs matching a pattern. */
async function findDescendants(
  ancestorPid: number,
  pattern: string
): Promise<number[]> {
  const candidates = await findProcessesByPattern(pattern);
  const descendants: number[] = [];
  for (const pid of candidates) {
    if (await isDescendantOf(pid, ancestorPid)) {
      descendants.push(pid);
    }
  }
  return descendants;
}

describe.skipIf(process.platform === 'win32')(
  'Unix MCP — orphan process cleanup (P460297924)',
  () => {
    let testCase: E2ETestCase | null = null;

    afterEach(async () => {
      if (testCase) {
        await testCase.cleanup();
        testCase = null;
      }
    });

    // TODO: These tests require mock-mcp-server to be built in the E2E CI
    // workflow (currently only chat_cli is built). Once `cargo build -p
    // mock-mcp-server` is added to .github/workflows/tui.yml, remove .todo()
    // and use the mock binary instead of inline node scripts.
    //
    // The behavior being tested (detached + -pgid kills the process tree) is
    // unchanged from v2.9.0 — only the Windows path was modified by this PR.
    // The existing signal-exit.test.ts covers the launcher→bun cleanup path.

    it.todo(
      'MCP server processes die when TUI is force-killed (no orphans)',
      async () => {
        // Use a unique marker in the process command so we can find it via pgrep
        const marker = `mcp-orphan-test-${Date.now()}`;

        testCase = await E2ETestCase.builder()
          .withTerminal({ width: 120, height: 40 })
          .withTestName('unix-mcp-orphan-cleanup')
          .withGlobalAgentConfig('test-mcp-orphan', {
            name: 'test-mcp-orphan',
            tools: ['*'],
            mcpServers: {
              'long-lived-server': {
                command: 'node',
                args: [
                  '-e',
                  `
              // ${marker}
              const { Server } = require('@modelcontextprotocol/sdk/server/index.js');
              const { StdioServerTransport } = require('@modelcontextprotocol/sdk/server/stdio.js');
              const server = new Server({ name: 'long-lived', version: '1.0.0' }, { capabilities: { tools: {} } });
              server.setRequestHandler('tools/list', async () => ({ tools: [] }));
              const transport = new StdioServerTransport();
              server.connect(transport);
            `,
                ],
              },
            },
          })
          .withCliArgs('--agent', 'test-mcp-orphan')
          .launch();

        await testCase.waitForText('ask a question', 20000);

        // Wait for MCP server to be fully running
        await new Promise((resolve) => setTimeout(resolve, 3000));

        const launcherPid = testCase.getPid()!;
        expect(launcherPid).toBeGreaterThan(0);

        // Find the MCP server process in our tree
        const mcpPids = await findDescendants(launcherPid, marker);
        // The MCP server should be running as a descendant
        expect(mcpPids.length).toBeGreaterThan(0);
        const mcpPid = mcpPids[0]!;
        expect(isAlive(mcpPid)).toBe(true);

        // Force-kill the launcher (simulates a crash — no graceful shutdown)
        process.kill(launcherPid, 'SIGKILL');

        // The launcher should be dead
        expect(await waitForExit(launcherPid, 5000)).toBe(true);

        // The MCP server should ALSO be dead (killed via process group).
        // Without detached:true + -pgid, it would become an orphan (ppid=1).
        expect(await waitForExit(mcpPid, 5000)).toBe(true);
      },
      45000
    );

    it.todo(
      'MCP server processes die on graceful close (SIGTERM)',
      async () => {
        const marker = `mcp-graceful-test-${Date.now()}`;

        testCase = await E2ETestCase.builder()
          .withTerminal({ width: 120, height: 40 })
          .withTestName('unix-mcp-graceful-cleanup')
          .withGlobalAgentConfig('test-mcp-graceful', {
            name: 'test-mcp-graceful',
            tools: ['*'],
            mcpServers: {
              'graceful-server': {
                command: 'node',
                args: [
                  '-e',
                  `
              // ${marker}
              const { Server } = require('@modelcontextprotocol/sdk/server/index.js');
              const { StdioServerTransport } = require('@modelcontextprotocol/sdk/server/stdio.js');
              const server = new Server({ name: 'graceful', version: '1.0.0' }, { capabilities: { tools: {} } });
              server.setRequestHandler('tools/list', async () => ({ tools: [] }));
              const transport = new StdioServerTransport();
              server.connect(transport);
            `,
                ],
              },
            },
          })
          .withCliArgs('--agent', 'test-mcp-graceful')
          .launch();

        await testCase.waitForText('ask a question', 20000);
        await new Promise((resolve) => setTimeout(resolve, 3000));

        const launcherPid = testCase.getPid()!;
        const mcpPids = await findDescendants(launcherPid, marker);
        expect(mcpPids.length).toBeGreaterThan(0);
        const mcpPid = mcpPids[0]!;

        // Graceful shutdown via SIGTERM
        process.kill(launcherPid, 'SIGTERM');

        // Both should exit cleanly
        expect(await waitForExit(launcherPid, 8000)).toBe(true);
        expect(await waitForExit(mcpPid, 8000)).toBe(true);
      },
      45000
    );
  }
);

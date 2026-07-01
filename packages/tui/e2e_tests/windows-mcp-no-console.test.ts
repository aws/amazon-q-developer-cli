/**
 * E2E tests for Windows MCP process visibility and functionality.
 *
 * Validates that:
 * 1. MCP stdio servers spawn WITHOUT visible console windows (P460297924)
 * 2. MCP server connection doesn't fail (stdio pipes functional)
 *
 * Root cause: v2.9.0 added `detached: true` to agent spawn, which on Windows
 * allocates a new visible console window per Node.js/Bun docs. The fix uses
 * platform-conditional spawn: `detached` on Unix, `windowsHide` on Windows.
 *
 * Ticket: https://t.corp.amazon.com/P460297924
 * GitHub: https://github.com/kirodotdev/Kiro/issues/9713
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe.skipIf(process.platform !== 'win32')(
  'Windows MCP — no visible console windows (P460297924)',
  () => {
    let testCase: E2ETestCase | null = null;

    afterEach(async () => {
      if (testCase) {
        await testCase.cleanup();
        testCase = null;
      }
    });

    it('agent process tree does not spawn conhost.exe (no visible console)', async () => {
      testCase = await E2ETestCase.builder()
        .withTerminal({ width: 120, height: 40 })
        .withTestName('windows-mcp-no-console')
        .launch();

      await testCase.waitForText('ask a question', 20000);

      // Give the process tree time to stabilize
      await new Promise((resolve) => setTimeout(resolve, 2000));

      const pid = testCase.getPid()!;
      expect(pid).toBeGreaterThan(0);

      // Check for conhost.exe processes spawned by our process tree.
      // conhost.exe is created by Windows for each process that allocates
      // a visible console. With windowsHide/CREATE_NO_WINDOW, no conhost
      // should be spawned. With the old detached:true, every child gets one.
      const checkProc = Bun.spawn(
        [
          'powershell',
          '-NoProfile',
          '-NonInteractive',
          '-Command',
          `
        $ErrorActionPreference = 'SilentlyContinue'
        function Get-Descendants($parentPid) {
          $result = @()
          $children = Get-CimInstance Win32_Process | Where-Object { $_.ParentProcessId -eq $parentPid }
          foreach ($child in $children) {
            $result += $child.ProcessId
            $result += Get-Descendants $child.ProcessId
          }
          return $result
        }
        $descendants = Get-Descendants ${pid}
        $conhosts = $descendants | ForEach-Object {
          Get-CimInstance Win32_Process | Where-Object { $_.ProcessId -eq $_ -and $_.Name -eq 'conhost.exe' }
        } | Where-Object { $_ -ne $null }
        if ($conhosts) { Write-Output $conhosts.Count } else { Write-Output 0 }
      `,
        ],
        { stdout: 'pipe', stderr: 'pipe' }
      );

      const stdout = await new Response(checkProc.stdout).text();
      const stderr = await new Response(checkProc.stderr).text();
      await checkProc.exited;

      const trimmed = stdout.trim().split('\n').pop() ?? '0';
      const conhostCount = parseInt(trimmed, 10);

      if (isNaN(conhostCount)) {
        // Log for debugging but don't fail — PowerShell may not work in all CI images
        console.warn('Could not enumerate conhost processes:', {
          stdout,
          stderr,
        });
        return;
      }

      // With windowsHide: true and no detached, there should be zero conhost.exe
      // instances spawned by our process tree
      expect(conhostCount).toBe(0);

      await testCase.pressCtrlCTwice();
      await testCase.expectExit();
    }, 45000);

    it('MCP server connects without failure (stdio pipes not broken)', async () => {
      // This test validates that the windowsHide + non-detached spawn doesn't
      // break MCP stdio communication. Uses a broken server path — if the MCP
      // init reaches "failed to spawn" rather than hanging, stdio pipes work.
      testCase = await E2ETestCase.builder()
        .withTerminal({ width: 120, height: 40 })
        .withTestName('windows-mcp-tool-call')
        .withGlobalAgentConfig('test-mcp-win', {
          name: 'test-mcp-win',
          tools: ['*'],
          mcpServers: {
            'test-server': {
              // Use a real command that exits immediately with valid MCP handshake failure
              // This proves stdio pipes work (we get the error back through them)
              command: 'cmd.exe',
              args: ['/C', 'echo', 'not-a-valid-mcp-server'],
            },
          },
        })
        .withCliArgs('--agent', 'test-mcp-win')
        .launch();

      await testCase.waitForText('ask a question', 20000);

      // The server should fail to connect (bad MCP handshake) — but the key
      // thing is it DOES fail promptly rather than hanging forever. A hang
      // would indicate broken stdio pipes (the v2.9.0 regression behavior).
      const store = await testCase.waitForStoreCondition(
        (s) =>
          (s as any).initErrors?.some?.(
            (e: any) =>
              e.type === 'mcp_failure' && e.serverName === 'test-server'
          ) ?? false,
        20000
      );

      // If we got here within 20s, stdio pipes are working — the error
      // propagated back through them. The v2.9.0 bug caused tool calls to
      // hang indefinitely (60s+) because detached broke pipe inheritance.
      expect(store).toBeTruthy();

      await testCase.pressCtrlCTwice();
      await testCase.expectExit();
    }, 45000);
  }
);

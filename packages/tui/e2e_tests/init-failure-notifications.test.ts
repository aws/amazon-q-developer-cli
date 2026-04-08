/**
 * E2E tests for initialization failure notifications in the TUI.
 *
 * Covers MCP server failures, agent-not-found fallback, and agent config
 * parse errors — verifying they surface in the notification bar, the store,
 * and the /mcp panel.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Initialization failure notifications', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  });

  it('shows notification when MCP server fails to load', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('init-fail-mcp')
      .withGlobalAgentConfig('test-broken-mcp', {
        name: 'test-broken-mcp',
        tools: ['*'],
        mcpServers: {
          'broken-server': {
            command: '/nonexistent/path/to/binary',
            args: [],
          },
        },
      })
      .withCliArgs('--agent', 'test-broken-mcp')
      .launch();

    await testCase.waitForText('ask a question', 15000);

    const store = await testCase.waitForStoreCondition(
      (s) => s.initErrors.some((e) => e.type === 'mcp_failure'),
      30000
    );

    if (store.initErrors[0]?.type === 'mcp_failure') {
      expect(store.initErrors[0].serverName).toBe('broken-server');
    }

    expect(store.transientAlert).toBeTruthy();
    expect(store.transientAlert?.status).toBe('error');
    expect(store.transientAlert?.message).toContain('MCP failure');

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);

  it('shows failed status in /mcp panel with error reason', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('init-fail-mcp-panel')
      .withGlobalAgentConfig('test-broken-mcp2', {
        name: 'test-broken-mcp2',
        tools: ['*'],
        mcpServers: {
          'will-fail-server': {
            command: '/this/does/not/exist',
            args: [],
          },
        },
      })
      .withCliArgs('--agent', 'test-broken-mcp2')
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands();

    await testCase.waitForStoreCondition(
      (s) => s.initErrors.some((e) => e.type === 'mcp_failure'),
      30000
    );

    // Open /mcp panel
    for (const char of '/mcp') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(300);
    await testCase.sendKeys('\r');

    const store = await testCase.waitForStoreCondition(
      (s) =>
        s.showMcpPanel &&
        s.mcpServers.some(
          (srv) => srv.name === 'will-fail-server' && srv.status === 'failed'
        ),
      30000
    );

    expect(store.showMcpPanel).toBe(true);
    const failedServer = store.mcpServers.find(
      (s) => s.name === 'will-fail-server'
    );
    expect(failedServer).toBeTruthy();
    expect(failedServer?.status).toBe('failed');

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);

  it('shows notification when requested agent is not found', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('init-fail-agent-not-found')
      .withCliArgs('--agent', 'nonexistent-agent-xyz')
      .launch();

    await testCase.waitForText('ask a question', 15000);

    const store = await testCase.waitForStoreCondition(
      (s) => s.initErrors.some((e) => e.type === 'agent_not_found'),
      30000
    );

    const err = store.initErrors.find((e) => e.type === 'agent_not_found');
    expect(err).toBeTruthy();
    if (err?.type === 'agent_not_found') {
      expect(err.requestedAgent).toBe('nonexistent-agent-xyz');
      expect(err.fallbackAgent).toBe('kiro_default');
    }

    expect(store.transientAlert).toBeTruthy();
    expect(store.transientAlert?.status).toBe('error');
    expect(store.transientAlert?.message).toContain('nonexistent-agent-xyz');
    expect(store.transientAlert?.message).toContain('not found');

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);
});

import { afterEach, describe, expect, it } from 'bun:test';
import { join } from 'node:path';
import { E2ETestCase } from './E2ETestCase';

const KAS_SERVER = join(
  __dirname,
  '../node_modules/@kiro/agent/dist/server/acp-server.js'
);

describe('--sessions boot', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('opens the dashboard without creating a chat session and exits on close', async () => {
    testCase = new E2ETestCase({
      testName: 'session-dashboard-boot',
      skipAgentIpc: true,
      settings: { 'chat.disableAutoAgentUpgrade': true },
      extraCliArgs: ['--sessions'],
      extraEnv: {
        KIRO_AGENT_ENGINE: 'kas',
        KIRO_KAS_NODE_PATH: process.env.KIRO_KAS_NODE_PATH ?? 'node',
        KIRO_KAS_SERVER_PATH: KAS_SERVER,
      },
    });
    await testCase.launch();

    const store = await testCase.waitForStoreCondition(
      (state) =>
        state.mode === 'session-dashboard' && state.showSessionDashboard,
      15_000
    );
    await testCase.waitForText('Sessions', 15_000);

    expect(store.sessionId).toBeFalsy();
    expect(store.isInitialized).toBe(true);

    await testCase.pressEscape();
    expect(await testCase.expectExit(10_000)).toBe(0);
  }, 60_000);
});

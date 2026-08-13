/**
 * E2E tests for the /config panel (cloud config UX) against the full stack:
 * real TUI + real Rust agent.
 *
 * Off-cohort absence is NOT tested here: under KIRO_TEST_MODE the Rust
 * launcher enables every rollout feature before spawning the TUI, so the
 * darkship control arm is unreachable through the real launch path. That
 * arm is pinned by config-command-gating.test.ts (unit) and
 * integ_tests/config-panel.test.ts (direct TUI spawn with authoritative
 * KIRO_ENABLED_FEATURES). These tests pin the on-cohort experience:
 * registration, the category table, typed subcommands, and close behavior.
 *
 * KIRO_ENABLED_FEATURES is passed explicitly because the Windows harness
 * spawns the TUI directly, bypassing the Rust launcher's feature export
 * (same reason E2ETestCase pins KIRO_LITE_ROLLOUT_ENABLED on win32). On
 * mac/linux the launcher merges this as a user-override extra — either
 * way the TUI sees cloud_config enabled.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

async function typeCommand(tc: E2ETestCase, cmd: string): Promise<void> {
  for (const ch of cmd) {
    await tc.sendKeys(ch);
    await tc.sleepMs(30);
  }
  await tc.sleepMs(300);
  await tc.sendKeys('\r');
}

async function waitForConfigPanel(
  tc: E2ETestCase,
  open: boolean
): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < 15_000) {
    const store = await tc.getStore();
    if (store.showConfigPanel === open) return;
    await tc.sleepMs(100);
  }
  throw new Error(`showConfigPanel never became ${open}`);
}

describe('/config command (e2e)', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  });

  it('opens the category table and closes on Escape', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('config-panel-e2e')
      .withEnv({ KIRO_ENABLED_FEATURES: '["cloud_config"]' })
      .launch();

    await testCase.waitForText('ask a question', 10_000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    const store = await testCase.getStore();
    expect(
      store.slashCommands.find((c: { name: string }) => c.name === '/config')
    ).toBeDefined();

    await typeCommand(testCase, '/config');
    await waitForConfigPanel(testCase, true);
    await testCase.waitForText('Category', 10_000);
    await testCase.waitForText('MCP servers', 10_000);

    await testCase.pressEscape();
    await waitForConfigPanel(testCase, false);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60_000);

  it('typed subcommand opens the category page directly', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('config-subcommand-e2e')
      .withEnv({ KIRO_ENABLED_FEATURES: '["cloud_config"]' })
      .launch();

    await testCase.waitForText('ask a question', 10_000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    await typeCommand(testCase, '/config env');
    await waitForConfigPanel(testCase, true);
    await testCase.waitForText('/config — environment variables', 10_000);

    const store = await testCase.getStore();
    expect(store.configPanelCategory).toBe('env');

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60_000);
});

/**
 * E2E tests for the /config panel (cloud config UX) against the full stack:
 * real TUI + real published KAS server (local session). /config is KAS-only
 * — its data (config-origin descriptors, powers/steering/hooks pushes) is
 * all KAS-fed — so these run the KAS engine, spawning @kiro/agent from
 * node_modules like the cloud suites do.
 *
 * Off-cohort absence is NOT tested here: under KIRO_TEST_MODE the Rust
 * launcher enables every rollout feature before spawning the TUI, so the
 * darkship control arm is unreachable through the real launch path. That
 * arm — and the V2-unregistered arm — are pinned by
 * config-command-gating.test.ts (unit) and integ_tests/config-panel.test.ts
 * (direct TUI spawn with authoritative KIRO_ENABLED_FEATURES). These tests
 * pin the on-cohort KAS experience: registration, the category table,
 * typed subcommands, and close behavior.
 *
 * KIRO_ENABLED_FEATURES is passed explicitly because the KAS-engine harness
 * spawns the TUI directly, bypassing the Rust launcher's feature export.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { E2ETestCase } from './E2ETestCase';

const KAS_SERVER = path.join(
  __dirname,
  '../node_modules/@kiro/agent/dist/server/acp-server.js'
);
// win32: real KAS spawns are unreliable under the Windows PTY harness.
// The missing-server warning fires only where the server's absence is the
// reason tests won't run, so a local run without registry access isn't
// silently green.
const kasServerMissing =
  process.platform !== 'win32' && !fs.existsSync(KAS_SERVER);
if (kasServerMissing) {
  console.warn(
    `SKIPPING /config E2E: published KAS server not found at ${KAS_SERVER}`
  );
}
const skip = process.platform === 'win32' || kasServerMissing;

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

/**
 * Wait for BOTH facts these tests need: /config's TUI-local registration,
 * and a KAS-sourced fact (the session's agents cache, populated from the
 * spawned server's session/new modes) so the real-KAS spawn is
 * load-bearing — a server that stalls in init fails here rather than
 * leaving the suite green on local-only behavior. The generic
 * waitForSlashCommands is unsuitable: it waits for a backend-sourced
 * command, which the KAS local flow may register later than the agents
 * cache, and /config itself never needs it.
 */
async function waitForConfigAndKas(tc: E2ETestCase): Promise<void> {
  const start = Date.now();
  let configSeen = false;
  let kasSeen = false;
  while (Date.now() - start < 30_000) {
    const store = await tc.getStore();
    configSeen ||= store.slashCommands.some(
      (c: { name: string }) => c.name === '/config'
    );
    kasSeen ||= store.kas.availableAgents.length > 0;
    if (configSeen && kasSeen) return;
    await tc.sleepMs(150);
  }
  throw new Error(
    `timed out: /config registered=${configSeen}, KAS agents=${kasSeen}`
  );
}

async function launchKas(testName: string): Promise<E2ETestCase> {
  return E2ETestCase.builder()
    .withTerminal({ width: 120, height: 40 })
    .withTestName(testName)
    .withKasEngine()
    .withEnv({
      KIRO_AGENT_ENGINE: 'kas',
      KIRO_KAS_SERVER_PATH: KAS_SERVER,
      KIRO_KAS_NODE_PATH: process.env.KIRO_KAS_NODE_PATH ?? 'node',
      KIRO_ENABLED_FEATURES: '["cloud_config"]',
    })
    .launch();
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

  it.skipIf(skip)(
    'opens the category table and closes on Escape',
    async () => {
      testCase = await launchKas('config-panel-e2e');

      await testCase.waitForText('ask a question', 20_000);
      await waitForConfigAndKas(testCase);
      await testCase.sleepMs(500);

      await typeCommand(testCase, '/config');
      await waitForConfigPanel(testCase, true);
      await testCase.waitForText('Category', 10_000);
      await testCase.waitForText('MCP servers', 10_000);

      await testCase.pressEscape();
      await waitForConfigPanel(testCase, false);

      await testCase.pressCtrlCTwice();
      await testCase.expectExit();
    },
    90_000
  );

  it.skipIf(skip)(
    'typed subcommand opens the category page directly',
    async () => {
      testCase = await launchKas('config-subcommand-e2e');

      await testCase.waitForText('ask a question', 20_000);
      await waitForConfigAndKas(testCase);
      await testCase.sleepMs(500);

      await typeCommand(testCase, '/config steering');
      await waitForConfigPanel(testCase, true);
      await testCase.waitForText('/config — steering', 10_000);

      const store = await testCase.getStore();
      expect(store.configPanelCategory).toBe('steering');

      await testCase.pressCtrlCTwice();
      await testCase.expectExit();
    },
    90_000
  );
});

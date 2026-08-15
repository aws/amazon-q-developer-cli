/**
 * Integration tests for the /config panel (cloud config UX, dark-shipped
 * behind the cloud_config rollout feature, KAS-only).
 *
 * Covers, against the real TUI in a PTY with the mocked KAS ACP backend:
 * - darkship gating: /config is unregistered and non-dispatching off-cohort
 * - engine gating: /config is unregistered on V2 even inside the cohort
 * - bare /config opens the category table overlay
 * - typed subcommand (/config steering) opens the category page directly
 * - in-panel navigation: Enter drills into a page, ESC walks back, ESC
 *   again closes
 */

import { afterEach, describe, expect, it } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { TestCase } from '../src/test-utils/TestCase';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import { defaultKasModes } from '../acp_integ_tests/shared/default-agent';
import { Feature } from '../src/features';

const ESC = '\x1b';
const ENTER = '\r';

async function typeSlowly(tc: TestCase | AcpTestCase, text: string) {
  for (const char of text) {
    await tc.sendKeys(char);
    await tc.sleepMs(30);
  }
  await tc.sleepMs(200);
}

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: 'config-panel-session',
    modes: defaultKasModes(),
  }));
  tc.mock.on('session/set_config_option', () => ({}));
}

async function launchKasWithCloudConfig(
  testName: string
): Promise<AcpTestCase> {
  const tc = new AcpTestCase({
    testName,
    extraEnv: {
      KIRO_ENABLED_FEATURES: JSON.stringify([Feature.CloudConfig]),
    },
  });
  setupHandshake(tc);
  await tc.launch();
  await tc.mock.awaitConnection();
  await tc.waitForVisibleText('ask a question', 15_000);
  return tc;
}

describe('/config panel (integ)', () => {
  let testCase: TestCase | AcpTestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('is unregistered and does not dispatch off-cohort (KAS)', async () => {
    const tc = new AcpTestCase({ testName: 'config-gated-off' });
    testCase = tc;
    setupHandshake(tc);
    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 15_000);

    let state = await tc.getStore();
    expect(
      state.slashCommands.find((c: { name: string }) => c.name === '/config')
    ).toBeUndefined();

    await typeSlowly(tc, '/config');
    await tc.sendKeys(ENTER);
    await tc.sleepMs(300);

    state = await tc.getStore();
    expect(state.showConfigPanel).toBe(false);
  }, 30_000);

  it('is unregistered on V2 even inside the cohort (KAS-only)', async () => {
    // V2 spawn: the panel's data is all KAS-fed, so /config must not
    // register — neither autocomplete nor dispatch.
    const tc = await TestCase.builder()
      .withTestName('config-v2-gated')
      .withTimeout(15_000)
      .withEnv({
        KIRO_ENABLED_FEATURES: JSON.stringify([Feature.CloudConfig]),
      })
      .launch();
    testCase = tc;
    await tc.waitForVisibleText('ask a question', 15_000);

    let state = await tc.getStore();
    expect(
      state.slashCommands.find((c: { name: string }) => c.name === '/config')
    ).toBeUndefined();

    await typeSlowly(tc, '/config');
    await tc.sendKeys(ENTER);
    await tc.sleepMs(300);

    state = await tc.getStore();
    expect(state.showConfigPanel).toBe(false);
  }, 30_000);

  it('bare /config opens the category table', async () => {
    testCase = await launchKasWithCloudConfig('config-bare-open');

    await typeSlowly(testCase, '/config');
    await testCase.sendKeys(ENTER);
    await testCase.waitForVisibleText('Category', 10_000);

    const state = await testCase.getStore();
    expect(state.showConfigPanel).toBe(true);
    expect(state.configPanelCategory).toBeNull();
    await testCase.waitForVisibleText('MCP servers', 5_000);
    await testCase.waitForVisibleText('steering', 5_000);
  }, 30_000);

  it('typed subcommand opens the category page directly', async () => {
    testCase = await launchKasWithCloudConfig('config-typed-subcommand');

    await typeSlowly(testCase, '/config steering');
    await testCase.sendKeys(ENTER);
    await testCase.waitForVisibleText('/config — steering', 10_000);

    const state = await testCase.getStore();
    expect(state.showConfigPanel).toBe(true);
    expect(state.configPanelCategory).toBe('steering');
  }, 30_000);

  it('Enter drills into a page, ESC walks back, ESC closes', async () => {
    testCase = await launchKasWithCloudConfig('config-drill-esc');

    await typeSlowly(testCase, '/config');
    await testCase.sendKeys(ENTER);
    await testCase.waitForVisibleText('Category', 10_000);

    // Down to steering (agents/mcp/hooks route to other views; powers row
    // is second) — steering is the third row and opens an in-panel page.
    await testCase.sendKeys('\x1b[B'); // down: mcp
    await testCase.sendKeys('\x1b[B'); // down: powers
    await testCase.sendKeys('\x1b[B'); // down: steering
    await testCase.sleepMs(200);
    await testCase.sendKeys(ENTER);
    await testCase.waitForVisibleText('/config — steering', 10_000);

    // ESC from the page walks back to the category table.
    await testCase.sendKeys(ESC);
    await testCase.waitForVisibleText('Category', 10_000);
    let state = await testCase.getStore();
    expect(state.showConfigPanel).toBe(true);

    // ESC from the table closes the overlay.
    await testCase.sendKeys(ESC);
    await testCase.waitForStore(
      (value: { showConfigPanel: boolean }) => !value.showConfigPanel
    );
    state = await testCase.getStore();
    expect(state.showConfigPanel).toBe(false);
  }, 30_000);
});

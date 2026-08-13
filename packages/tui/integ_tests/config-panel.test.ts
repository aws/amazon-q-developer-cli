/**
 * Integration tests for the /config panel (cloud config UX, dark-shipped
 * behind the cloud_config rollout feature).
 *
 * Covers, against the real TUI in a PTY with the mocked ACP backend:
 * - darkship gating: /config is unregistered and non-dispatching off-cohort
 * - bare /config opens the category table overlay
 * - typed subcommand (/config steering) opens the category page directly
 * - in-panel navigation: Enter drills into a page, ESC walks back, ESC
 *   again closes
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { Feature } from '../src/features';

const ESC = '\x1b';
const ENTER = '\r';

async function typeSlowly(tc: TestCase, text: string) {
  for (const char of text) {
    await tc.sendKeys(char);
    await tc.sleepMs(30);
  }
  await tc.sleepMs(200);
}

async function launchWithCloudConfig(testName: string): Promise<TestCase> {
  const testCase = await TestCase.builder()
    .withTestName(testName)
    .withTimeout(15_000)
    .withEnv({
      KIRO_ENABLED_FEATURES: JSON.stringify([Feature.CloudConfig]),
    })
    .launch();
  await testCase.waitForVisibleText('ask a question', 15_000);
  return testCase;
}

describe('/config panel (integ)', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('is unregistered and does not dispatch off-cohort', async () => {
    testCase = await TestCase.builder()
      .withTestName('config-gated-off')
      .withTimeout(15_000)
      .launch();
    await testCase.waitForVisibleText('ask a question', 15_000);

    let state = await testCase.getStore();
    expect(
      state.slashCommands.find((c: { name: string }) => c.name === '/config')
    ).toBeUndefined();

    await typeSlowly(testCase, '/config');
    await testCase.sendKeys(ENTER);
    await testCase.sleepMs(300);

    state = await testCase.getStore();
    expect(state.showConfigPanel).toBe(false);
  }, 30_000);

  it('bare /config opens the category table', async () => {
    testCase = await launchWithCloudConfig('config-bare-open');

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
    testCase = await launchWithCloudConfig('config-typed-subcommand');

    await typeSlowly(testCase, '/config steering');
    await testCase.sendKeys(ENTER);
    await testCase.waitForVisibleText('/config — steering', 10_000);

    const state = await testCase.getStore();
    expect(state.showConfigPanel).toBe(true);
    expect(state.configPanelCategory).toBe('steering');
  }, 30_000);

  it('Enter drills into a page, ESC walks back, ESC closes', async () => {
    testCase = await launchWithCloudConfig('config-drill-esc');

    await typeSlowly(testCase, '/config');
    await testCase.sendKeys(ENTER);
    await testCase.waitForVisibleText('Category', 10_000);

    // First row is agents — Enter opens its page.
    await testCase.sendKeys(ENTER);
    await testCase.waitForVisibleText('/config — agents', 10_000);

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

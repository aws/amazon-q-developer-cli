/**
 * E2E tests for shadow text autocomplete on /model and /agent arguments.
 *
 * These tests use the real backend (E2ETestCase) to verify:
 * - Shadow text appears for /model <partial> and /agent <partial>
 * - Tab accepts shadow text completion
 * - Right arrow accepts shadow text completion
 * - No shadow text for top-level command menu
 * - Tab on selection command args doesn't trigger file completion
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

async function typeSlowly(tc: E2ETestCase, text: string) {
  for (const char of text) {
    await tc.sendKeys(char);
    await tc.sleepMs(40);
  }
  await tc.sleepMs(200);
}

describe('Shadow text autocomplete', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('/model <partial> shows shadow text and Tab accepts', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('shadow-model-tab')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(3000);

    await typeSlowly(testCase, '/model clau');

    // Wait for shadow text to appear (backend fetch + cache)
    const state = await testCase.waitForStoreCondition(
      (s) => s.commandShadowText !== null,
      10000
    );
    expect(state.commandShadowText!.length).toBeGreaterThan(0);

    // Tab accepts the shadow text
    await testCase.sendKeys('\t');
    await testCase.sleepMs(500);

    const after = await testCase.getStore();
    // Input should now be longer than what we typed
    expect(after.commandInputValue.length).toBeGreaterThan(
      '/model clau'.length
    );
    expect(after.commandInputValue).toStartWith('/model clau');
    expect(after.commandShadowText).toBeNull();
  }, 60000);

  it('/model <partial> right arrow accepts shadow text', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('shadow-model-arrow')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(3000);

    await typeSlowly(testCase, '/model clau');

    await testCase.waitForStoreCondition(
      (s) => s.commandShadowText !== null,
      10000
    );

    // Right arrow accepts
    await testCase.sendKeys('\x1b[C');
    await testCase.sleepMs(500);

    const after = await testCase.getStore();
    expect(after.commandInputValue.length).toBeGreaterThan(
      '/model clau'.length
    );
    expect(after.commandInputValue).toStartWith('/model clau');
  }, 60000);

  it('/agent <partial> shows shadow text when agents available', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('shadow-agent-tab')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(3000);

    // Type /agent then Enter to check if agents are available
    await typeSlowly(testCase, '/agent');
    await testCase.sendKeys('\r');
    await testCase.sleepMs(2000);

    const store = await testCase.getStore();
    // If selection menu opened, agents are available
    if (!store.activeCommand) {
      console.log('No agents available in test environment — skipping');
      return;
    }

    // Get first agent name to use as partial
    const firstAgent = store.activeCommand.options[0]?.label;
    if (!firstAgent || firstAgent.length < 3) {
      console.log('No usable agent name — skipping');
      return;
    }

    // Escape, clear, type partial
    await testCase.sendKeys('\x1b'); // Escape
    await testCase.sleepMs(300);
    await testCase.sendKeys('\x15'); // Ctrl+U
    await testCase.sleepMs(300);

    const partial = firstAgent.slice(0, 3);
    await typeSlowly(testCase, `/agent ${partial}`);

    const state = await testCase.waitForStoreCondition(
      (s) => s.commandShadowText !== null,
      10000
    );
    expect(state.commandShadowText!.length).toBeGreaterThan(0);

    // Tab accepts
    await testCase.sendKeys('\t');
    await testCase.sleepMs(500);

    const after = await testCase.getStore();
    expect(after.commandInputValue).toStartWith(`/agent ${partial}`);
    expect(after.commandInputValue.length).toBeGreaterThan(
      `/agent ${partial}`.length
    );
    expect(after.commandShadowText).toBeNull();
  }, 60000);

  it('no shadow text for top-level /ag (dropdown handles it)', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('shadow-no-toplevel')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(1000);

    await typeSlowly(testCase, '/ag');

    await testCase.waitForText('/agent', 3000);
    const state = await testCase.getStore();
    expect(state.commandShadowText).toBeNull();
  }, 30000);

  it('/agent swap <partial> shows shadow text and Tab accepts', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('shadow-agent-swap-tab')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(3000);

    // Discover available agents via the picker
    await typeSlowly(testCase, '/agent');
    await testCase.sendKeys('\r');
    await testCase.sleepMs(2000);

    const store = await testCase.getStore();
    if (!store.activeCommand) {
      console.log('No agents available in test environment — skipping');
      return;
    }

    const firstAgent = store.activeCommand.options[0]?.label;
    if (!firstAgent || firstAgent.length < 3) {
      console.log('No usable agent name — skipping');
      return;
    }

    // Escape picker, clear input
    await testCase.sendKeys('\x1b');
    await testCase.sleepMs(300);
    await testCase.sendKeys('\x15');
    await testCase.sleepMs(300);

    // Type /agent swap <partial>
    const partial = firstAgent.slice(0, 3);
    await typeSlowly(testCase, `/agent swap ${partial}`);

    const state = await testCase.waitForStoreCondition(
      (s) => s.commandShadowText !== null,
      10000
    );
    expect(state.commandShadowText!.length).toBeGreaterThan(0);

    // Tab accepts the shadow text
    await testCase.sendKeys('\t');
    await testCase.sleepMs(500);

    const after = await testCase.getStore();
    expect(after.commandInputValue).toStartWith(`/agent swap ${partial}`);
    expect(after.commandInputValue.length).toBeGreaterThan(
      `/agent swap ${partial}`.length
    );
    expect(after.commandShadowText).toBeNull();
  }, 60000);

  it('Tab on /agent swap <partial> does not reopen subcommand dropdown', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('shadow-agent-swap-no-dropdown')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(3000);

    // Type /agent swap x (unlikely to match any agent)
    await typeSlowly(testCase, '/agent swap x');
    await testCase.sleepMs(500);

    // Tab should NOT open the subcommand dropdown
    await testCase.sendKeys('\t');
    await testCase.sleepMs(500);

    const state = await testCase.getStore();
    // activeCommand should be null (no dropdown opened)
    expect(state.activeCommand).toBeNull();
    // Input should remain unchanged
    expect(state.commandInputValue).toBe('/agent swap x');
  }, 30000);

  it('Tab on /model <partial> does not show filenames', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('shadow-no-files')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(3000);

    await typeSlowly(testCase, '/model src');
    await testCase.sleepMs(500);

    await testCase.sendKeys('\t');
    await testCase.sleepMs(500);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).not.toContain('.ts');
    expect(snap).not.toContain('.tsx');
    expect(snap).not.toContain('node_modules');

    const state = await testCase.getStore();
    expect(state.commandInputValue).toBe('/model src');
  }, 30000);
});

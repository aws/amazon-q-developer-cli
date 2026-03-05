/**
 * E2E tests for /mcp and /tools panel commands.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('MCP Panel', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    await new Promise(resolve => setTimeout(resolve, 500));
  });

  it('shows MCP panel on /mcp command', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('mcp-panel-show')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    // Type /mcp command
    for (const char of '/mcp') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(300);
    await testCase.sendKeys('\r');

    // Wait for panel to appear by checking store state
    const start = Date.now();
    while (Date.now() - start < 15000) {
      const store = await testCase.getStore();
      if (store.showMcpPanel) break;
      await testCase.sleepMs(100);
    }

    // Verify store state
    const store = await testCase.getStore();
    expect(store.showMcpPanel).toBe(true);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);

  it('closes MCP panel on Escape', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('mcp-panel-escape')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    // Open MCP panel
    for (const char of '/mcp') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(300);
    await testCase.sendKeys('\r');

    // Wait for panel to appear by checking store state
    const start = Date.now();
    while (Date.now() - start < 15000) {
      const store = await testCase.getStore();
      if (store.showMcpPanel) break;
      await testCase.sleepMs(100);
    }

    let store = await testCase.getStore();
    expect(store.showMcpPanel).toBe(true);

    // Press Escape to close (press twice in case search has focus)
    await testCase.pressEscape();
    await testCase.sleepMs(200);
    await testCase.pressEscape();
    await testCase.sleepMs(500);

    store = await testCase.getStore();
    expect(store.showMcpPanel).toBe(false);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);
});

describe('Tools Panel', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    await new Promise(resolve => setTimeout(resolve, 500));
  });

  it('shows tools panel on /tools command', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('tools-panel-show')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    // Type /tools command
    for (const char of '/tools') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(300);
    await testCase.sendKeys('\r');

    // Wait for panel to appear by checking store state
    const start = Date.now();
    while (Date.now() - start < 15000) {
      const store = await testCase.getStore();
      if (store.showToolsPanel) break;
      await testCase.sleepMs(100);
    }

    // Verify store state
    const store = await testCase.getStore();
    expect(store.showToolsPanel).toBe(true);
    expect(store.toolsList.length).toBeGreaterThan(0);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);

  it('closes tools panel on Escape', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('tools-panel-escape')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    // Open tools panel
    for (const char of '/tools') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(300);
    await testCase.sendKeys('\r');

    // Wait for panel to appear by checking store state
    const start = Date.now();
    while (Date.now() - start < 15000) {
      const store = await testCase.getStore();
      if (store.showToolsPanel) break;
      await testCase.sleepMs(100);
    }

    let store = await testCase.getStore();
    expect(store.showToolsPanel).toBe(true);

    // Press Escape to close (press twice in case search has focus)
    await testCase.pressEscape();
    await testCase.sleepMs(200);
    await testCase.pressEscape();
    await testCase.sleepMs(500);

    store = await testCase.getStore();
    expect(store.showToolsPanel).toBe(false);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);

  it('shows tool count in panel header', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('tools-panel-count')
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    for (const char of '/tools') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(300);
    await testCase.sendKeys('\r');

    // Wait for panel to appear by checking store state
    const start = Date.now();
    while (Date.now() - start < 15000) {
      const store = await testCase.getStore();
      if (store.showToolsPanel) break;
      await testCase.sleepMs(100);
    }

    const store = await testCase.getStore();
    expect(store.showToolsPanel).toBe(true);
    expect(store.toolsList.length).toBeGreaterThan(0);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);
});

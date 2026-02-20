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
    await testCase.sleepMs(200);

    // Type /mcp command
    for (const char of '/mcp') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    await testCase.sendKeys('\r');
    await testCase.sleepMs(1000);

    // Verify panel appears with /mcp header
    await testCase.waitForText('/mcp', 10000);
    await testCase.waitForText('ESC', 5000);

    // Verify store state
    const store = await testCase.getStore();
    expect(store.showMcpPanel).toBe(true);

    console.log('MCP Panel Snapshot:\n' + testCase.getSnapshotFormatted());

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
    await testCase.sleepMs(200);

    // Open MCP panel
    for (const char of '/mcp') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    await testCase.sendKeys('\r');
    await testCase.sleepMs(1000);

    await testCase.waitForText('/mcp', 10000);

    let store = await testCase.getStore();
    expect(store.showMcpPanel).toBe(true);

    // Press Escape to close
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
    await testCase.sleepMs(200);

    // Type /tools command
    for (const char of '/tools') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    await testCase.sendKeys('\r');
    await testCase.sleepMs(1000);

    // Verify panel appears with /tools header and table headers
    await testCase.waitForText('/tools', 10000);
    await testCase.waitForText('Name', 5000);
    await testCase.waitForText('Source', 5000);
    await testCase.waitForText('Description', 5000);
    await testCase.waitForText('ESC', 5000);

    // Verify store state
    const store = await testCase.getStore();
    expect(store.showToolsPanel).toBe(true);
    expect(store.toolsList.length).toBeGreaterThan(0);

    // Verify built-in tools are present
    const toolNames = store.toolsList.map(t => t.name);
    expect(toolNames).toContain('read');
    expect(toolNames).toContain('write');

    // Verify built-in tools show on screen
    await testCase.waitForText('built-in', 5000);

    console.log('Tools Panel Snapshot:\n' + testCase.getSnapshotFormatted());

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
    await testCase.sleepMs(200);

    // Open tools panel
    for (const char of '/tools') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    await testCase.sendKeys('\r');
    await testCase.sleepMs(1000);

    await testCase.waitForText('/tools', 10000);

    let store = await testCase.getStore();
    expect(store.showToolsPanel).toBe(true);

    // Press Escape to close
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
    await testCase.sleepMs(200);

    for (const char of '/tools') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    await testCase.sendKeys('\r');
    await testCase.sleepMs(1000);

    // Verify the header shows tool count (e.g. "/tools · 12 tools")
    await testCase.waitForText('tools', 10000);

    const store = await testCase.getStore();
    const count = store.toolsList.length;
    await testCase.waitForText(`${count} tool`, 5000);

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);
});

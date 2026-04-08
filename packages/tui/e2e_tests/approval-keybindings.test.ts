/**
 * E2E tests for approval menu keybindings.
 *
 * Validates that Esc, Right/Left arrow, and Tab correctly navigate
 * the approval dropdown ↔ drill-in modes without regressions.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

// ANSI escape sequences for arrow keys
const RIGHT_ARROW = '\x1b[C';
const LEFT_ARROW = '\x1b[D';

/** Push a write tool-use that triggers approval, plus a follow-up response. */
async function setupApproval(tc: E2ETestCase, filePath: string) {
  await tc.pushSendMessageResponse([
    {
      kind: 'event',
      data: {
        kind: 'ToolUseEvent',
        data: {
          tool_use_id: 'tool-write-approval',
          name: 'write',
          input: JSON.stringify({
            command: 'create',
            path: filePath,
            content: 'hello',
          }),
          stop: true,
        },
      },
    },
  ]);
  await tc.pushSendMessageResponse(null);
  await tc.pushSendMessageResponse([
    {
      kind: 'event',
      data: {
        kind: 'AssistantResponseEvent',
        data: { content: 'Done.' },
      },
    },
  ]);
  await tc.pushSendMessageResponse(null);
}

describe('Approval Keybindings', () => {
  let testCase: E2ETestCase | null = null;
  let tempDir = '';

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    if (tempDir) {
      try {
        fs.rmSync(tempDir, { recursive: true });
      } catch {
        /* ignore */
      }
      tempDir = '';
    }
  });

  /** Launch TUI, trigger a write tool, and wait for the approval dialog. */
  async function launchWithApproval(testName: string): Promise<E2ETestCase> {
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-e2e-approval-'));
    const filePath = path.join(tempDir, 'test.txt');

    const tc = await E2ETestCase.builder()
      .withTestName(testName)
      .withTerminal({ width: 120, height: 40 })
      .launch();

    await tc.waitForText('ask a question', 10000);
    await tc.getSessionId();
    await setupApproval(tc, filePath);

    await tc.sendKeys('create test file');
    await tc.sleepMs(100);
    await tc.pressEnter();

    await tc.waitForText('requires approval', 15000);
    return tc;
  }

  it('Esc dismisses the approval menu', async () => {
    testCase = await launchWithApproval('approval-esc-dismiss');

    const storeBefore = await testCase.getStore();
    expect(storeBefore.pendingApproval).not.toBeNull();

    await testCase.pressEscape();

    const storeAfter = await testCase.waitForStoreCondition(
      (s) => s.pendingApproval === null,
      5000
    );
    expect(storeAfter.pendingApproval).toBeNull();
  }, 30000);

  it('Right arrow enters drill-in mode from dropdown', async () => {
    testCase = await launchWithApproval('approval-right-drill-in');

    const storeBefore = await testCase.getStore();
    expect(storeBefore.approvalMode).toBe('dropdown');

    await testCase.sendKeys(RIGHT_ARROW);

    const storeAfter = await testCase.waitForStoreCondition(
      (s) => s.approvalMode === 'drill-in',
      5000
    );
    expect(storeAfter.approvalMode).toBe('drill-in');

    // Wait for render to catch up, then verify title
    await testCase.waitForText('Modify request', 5000);
  }, 30000);

  it('Left arrow returns from drill-in to dropdown', async () => {
    testCase = await launchWithApproval('approval-left-arrow-back');

    await testCase.sendKeys(RIGHT_ARROW);
    await testCase.waitForStoreCondition(
      (s) => s.approvalMode === 'drill-in',
      5000
    );

    await testCase.sendKeys(LEFT_ARROW);

    const storeAfter = await testCase.waitForStoreCondition(
      (s) => s.approvalMode === 'dropdown',
      5000
    );
    expect(storeAfter.approvalMode).toBe('dropdown');
  }, 30000);

  it('Tab switches from dropdown to drill-in', async () => {
    testCase = await launchWithApproval('approval-tab-toggle');

    expect((await testCase.getStore()).approvalMode).toBe('dropdown');

    // Tab → drill-in
    await testCase.sendKeys('\t');
    const s = await testCase.waitForStoreCondition(
      (s) => s.approvalMode === 'drill-in',
      5000
    );
    expect(s.approvalMode).toBe('drill-in');
  }, 30000);

  it('Esc from drill-in returns to dropdown, not dismiss', async () => {
    testCase = await launchWithApproval('approval-esc-from-drill-in');

    await testCase.sendKeys(RIGHT_ARROW);
    await testCase.waitForStoreCondition(
      (s) => s.approvalMode === 'drill-in',
      5000
    );

    await testCase.pressEscape();

    const store = await testCase.waitForStoreCondition(
      (s) => s.approvalMode === 'dropdown',
      5000
    );
    expect(store.approvalMode).toBe('dropdown');
    expect(store.pendingApproval).not.toBeNull();
  }, 30000);

  it('Right arrow is a no-op when already in drill-in', async () => {
    testCase = await launchWithApproval('approval-right-noop-drill-in');

    await testCase.sendKeys(RIGHT_ARROW);
    await testCase.waitForStoreCondition(
      (s) => s.approvalMode === 'drill-in',
      5000
    );

    await testCase.sendKeys(RIGHT_ARROW);
    await testCase.sleepMs(300);

    const store = await testCase.getStore();
    expect(store.approvalMode).toBe('drill-in');
    expect(store.pendingApproval).not.toBeNull();
  }, 30000);
});

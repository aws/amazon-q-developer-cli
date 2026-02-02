import { test, expect } from '@playwright/test';
import { typeCommand, waitForText } from '../helpers';
import { readFileSync, unlinkSync, existsSync } from 'fs';

const LOG_FILE = '/tmp/pretooluse-hook-log.txt';
const TEST_DIR = 'tests/test-pretooluse-hook';

test.beforeEach(() => {
  if (existsSync(LOG_FILE)) {
    unlinkSync(LOG_FILE);
  }
});

test('preToolUse hook matchers support both "read" and "fs_read" aliases', async ({ page }) => {
  await page.goto('/shell.html');
  await expect(page.locator('#status')).toContainText('Connected', { timeout: 10000 });

  await waitForText(page, '$', 10000);

  await typeCommand(page, `cd ${TEST_DIR}`);
  await waitForText(page, '$', 5000);
  
  await typeCommand(page, '../../../../target/debug/chat_cli chat --agent hook-test-agent');
  await waitForText(page, '[hook-test-agent]', 30000);

  await page.keyboard.type('read the contents of test-file.txt');
  await page.keyboard.press('Enter');

  // Verify both hooks executed (shows "2 of 2 hooks finished")
  await waitForText(page, '2 of 2 hooks finished', 30000);

  await waitForText(page, 'Test file', 30000);

  const logContent = readFileSync(LOG_FILE, 'utf-8');
  console.log('=== HOOK LOG ===');
  console.log(logContent);
  console.log('================');

  // Both hooks should have triggered for the same fs_read tool use
  expect(logContent).toContain('read_hook_triggered');
  expect(logContent).toContain('fs_read_hook_triggered');

  // Take screenshot at end of test
  await page.screenshot({ path: 'test-results/pretooluse-hook.png' });
});

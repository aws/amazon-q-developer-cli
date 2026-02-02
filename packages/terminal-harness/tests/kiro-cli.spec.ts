import { test, expect } from '@playwright/test';
import { typeCommand, getScreenContent, waitForText } from './helpers';

test('kiro-cli answers ultimate question', async ({ page }) => {
  await page.goto('/shell.html');
  await expect(page.locator('#status')).toContainText('Connected', { timeout: 10000 });

  // Wait for shell prompt
  await waitForText(page, '$', 10000);
  
  await typeCommand(page, 'kiro-cli chat');
  
  // Wait for kiro-cli to start
  await waitForText(page, '>', 30000);

  // Ask the question
  await page.keyboard.type('whats the ultimate answer to life universe and everything');
  await page.keyboard.press('Enter');

  // Wait for response containing 42
  await waitForText(page, '42', 60000);

  const content = await getScreenContent(page);
  console.log('=== SCREEN CONTENT ===');
  console.log(content);
  console.log('======================');
  expect(content).toContain('42');
});

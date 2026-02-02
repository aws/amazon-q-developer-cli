import { test, expect } from '@playwright/test';
import { typeCommand, getScreenContent } from './helpers';

test('terminal connects and runs echo command', async ({ page }) => {
  page.on('console', msg => console.log('BROWSER:', msg.text()));
  
  await page.goto('/shell.html');
  await expect(page.locator('#status')).toContainText('Connected', { timeout: 10000 });

  await page.waitForTimeout(1000); // wait for shell prompt
  await typeCommand(page, 'echo hello-terminal-harness');
  await page.waitForTimeout(2000); // wait for output

  const content = await getScreenContent(page);
  console.log('SCREEN:', content);
  expect(content).toContain('hello-terminal-harness');
});

import { Page, expect } from '@playwright/test';

export async function waitForConnection(page: Page) {
  // Log browser console for debugging
  page.on('console', msg => console.log('BROWSER:', msg.text()));
  await expect(page.locator('#status')).toContainText('Connected', { timeout: 10000 });
}

export async function typeCommand(page: Page, command: string) {
  await page.locator('#terminal').click();
  await page.keyboard.type(command);
  await page.keyboard.press('Enter');
  await page.waitForTimeout(500);
}

export async function getScreenContent(page: Page): Promise<string> {
  return page.evaluate(() => {
    const term = (window as any).term;
    const lines: string[] = [];
    const buffer = term.buffer.active;
    for (let i = 0; i < term.rows; i++) {
      lines.push(buffer.getLine(i)?.translateToString() || '');
    }
    return lines.join('\n');
  });
}

export async function waitForText(page: Page, text: string, timeout = 10000) {
  const start = Date.now();
  while (Date.now() - start < timeout) {
    const content = await getScreenContent(page);
    if (content.includes(text)) return;
    await page.waitForTimeout(100);
  }
  throw new Error(`Timeout waiting for text: "${text}"`);
}

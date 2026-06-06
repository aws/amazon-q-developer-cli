import type { TestCase } from '../../../src/test-utils/TestCase';
import type { E2ETestCase } from '../../E2ETestCase';
import { CMD_LITE, CMD_TUI } from './commands';

export async function switchToLite(tc: E2ETestCase | TestCase): Promise<void> {
  for (const ch of CMD_LITE + ' ') {
    await tc.sendKeys(ch);
    await tc.sleepMs(30);
  }
  await tc.sleepMs(200);
  await tc.sendKeys('\r');
  await tc.sleepMs(800);
}

export async function switchToTui(tc: E2ETestCase | TestCase): Promise<void> {
  for (const ch of CMD_TUI + ' ') {
    await tc.sendKeys(ch);
    await tc.sleepMs(30);
  }
  await tc.sleepMs(200);
  await tc.sendKeys('\r');
  await tc.sleepMs(800);
}

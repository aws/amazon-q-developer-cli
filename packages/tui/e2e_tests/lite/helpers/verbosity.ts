import type { TestCase } from '../../../src/test-utils/TestCase';
import type { E2ETestCase } from '../../E2ETestCase';
import { CMD_VERBOSITY } from './commands';

/**
 * Navigate to the verbosity density preset menu and select a preset.
 * Drives via visible-text navigation so a future rename of /verbosity to
 * /settings verbosity requires updating only this helper.
 */
export async function setLiteDensity(
  tc: E2ETestCase | TestCase,
  preset: 'minimal' | 'lean' | 'default' | 'full'
): Promise<void> {
  for (const ch of CMD_VERBOSITY + ' ') {
    await tc.sendKeys(ch);
    await tc.sleepMs(30);
  }
  await tc.sleepMs(200);
  await tc.sendKeys('\r');
  await tc.sleepMs(500);
  // Navigate to the preset — use arrow keys + Enter
  // The menu order is: minimal, lean, default, full
  const presetOrder = ['minimal', 'lean', 'default', 'full'];
  const targetIdx = presetOrder.indexOf(preset);
  for (let i = 0; i < targetIdx; i++) {
    await tc.sendKeys('\x1b[B'); // down arrow
    await tc.sleepMs(100);
  }
  await tc.sendKeys('\r');
  await tc.sleepMs(300);
}

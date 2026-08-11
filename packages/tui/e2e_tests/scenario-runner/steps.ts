import type { TestHarness } from './types';

const DEFAULT_STEP_TIMEOUT = 45_000;
const TYPE_CHAR_DELAY = 30;

async function typeWithDelay(harness: TestHarness, text: string): Promise<void> {
  for (const char of text) {
    await harness.sendKeys(char);
    await harness.sleepMs(TYPE_CHAR_DELAY);
  }
}

export function isFrameStep(step: string): boolean {
  return step.startsWith('frame:');
}

export function frameLabel(step: string): string {
  return step.substring('frame:'.length);
}

export async function executeStep(
  harness: TestHarness,
  step: string
): Promise<void> {
  const colonIndex = step.indexOf(':');
  const command = colonIndex === -1 ? step : step.substring(0, colonIndex);
  const arg = colonIndex === -1 ? '' : step.substring(colonIndex + 1);

  switch (command) {
    case 'type':
      await typeWithDelay(harness, arg);
      return;
    case 'enter':
      await harness.pressEnter();
      return;
    case 'waitForText':
      await harness.waitForText(arg, DEFAULT_STEP_TIMEOUT);
      return;
    case 'waitForIdle':
      await harness.waitForIdle(60_000);
      return;
    case 'prompt':
      await typeWithDelay(harness, arg);
      await harness.pressEnter();
      return;
    case 'ctrlc':
      await harness.pressCtrlC();
      return;
    case 'ctrlc-twice':
      await harness.pressCtrlCTwice();
      return;
    case 'ctrlj':
      await harness.sendKeys([0x0a]);
      return;
    case 'ctrls':
      await harness.sendKeys([0x13]);
      return;
    case 'arrowUp':
      await harness.sendKeys('\x1b[A');
      return;
    case 'arrowDown':
      await harness.sendKeys('\x1b[B');
      return;
    case 'escape':
      await harness.pressEscape();
      return;
    case 'tab':
      await harness.sendKeys('\t');
      return;
    case 'sleep': {
      const ms = parseInt(arg, 10);
      if (Number.isNaN(ms) || ms < 0) {
        throw new Error(`Invalid sleep duration: "${arg}"`);
      }
      await harness.sleepMs(ms);
      return;
    }
    case 'frame':
      return;
    default:
      throw new Error(`Unknown step command: "${command}" in step "${step}"`);
  }
}

import type { TestHarness } from './types';

const DEFAULT_STEP_TIMEOUT = 45_000;
const TYPE_CHAR_DELAY = 30;

/**
 * Verbs that are nothing but a keypress, as the bytes a terminal sends for that
 * chord. Kept as data rather than branches so adding a key does not grow the
 * dispatcher — the ones that remain in the switch below need more than bytes.
 */
const KEYPRESS_STEPS: Record<string, string | number[]> = {
  ctrlj: [0x0a],
  ctrls: [0x13],
  ctrla: [0x01],
  ctrle: [0x05],
  ctrlk: [0x0b],
  ctrlu: [0x15],
  ctrlw: [0x17],
  ctrlf: [0x06],
  ctrlo: [0x0f],
  ctrlx: [0x18],
  backspace: [0x7f],
  tab: '\t',
  arrowUp: '\x1b[A',
  arrowDown: '\x1b[B',
  arrowLeft: '\x1b[D',
  arrowRight: '\x1b[C',
};

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

  const keys = Object.hasOwn(KEYPRESS_STEPS, command)
    ? KEYPRESS_STEPS[command]
    : undefined;
  if (keys !== undefined) {
    await harness.sendKeys(keys);
    return;
  }

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
    case 'escape':
      await harness.pressEscape();
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

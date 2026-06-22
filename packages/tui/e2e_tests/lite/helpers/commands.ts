/**
 * Central slash-command string constants for lite TUI tests.
 * When a command is renamed (e.g. /verbosity -> /settings verbosity),
 * update only this file -- all tests import from here.
 */

import type { TestCase } from '../../../src/test-utils/TestCase';
import type { E2ETestCase } from '../../E2ETestCase';

/**
 * Type a slash command char-by-char (the per-char delay avoids the
 * autocomplete menu intercepting Enter), then submit.
 *
 * Defaults match the e2e usage (no trailing space, 200ms settle, no extra
 * post-submit wait). The integ TestCase path is behaviorally load-bearing on
 * `trailingSpace: true` + a longer `postEnterMs` settle — pass those.
 */
export async function typeSlashCommand(
  tc: E2ETestCase | TestCase,
  command: string,
  opts: { trailingSpace?: boolean; postEnterMs?: number } = {}
): Promise<void> {
  const text = opts.trailingSpace ? command + ' ' : command;
  for (const char of text) {
    await tc.sendKeys(char);
    await tc.sleepMs(30);
  }
  await tc.sleepMs(200);
  await tc.pressEnter();
  if (opts.postEnterMs) await tc.sleepMs(opts.postEnterMs);
}

/**
 * Type a user prompt and submit it. E2ETestCase has no typeAndSubmit
 * convenience (unlike integ TestCase), so the type/settle/enter triplet is
 * otherwise hand-rolled at every e2e call site.
 */
export async function sendUserMessage(
  tc: E2ETestCase,
  text: string,
  settleMs = 100
): Promise<void> {
  await tc.sendKeys(text);
  await tc.sleepMs(settleMs);
  await tc.pressEnter();
}

export const CMD_LITE = '/lite';
export const CMD_TUI = '/tui';
export const CMD_CHAT = '/chat';
export const CMD_CHAT_NEW = '/chat new';
export const CMD_CLEAR = '/clear';
export const CMD_THEME_DARK = '/theme bundled:dark';
export const CMD_THEME_LIGHT = '/theme bundled:light';

/**
 * Shared slash-command helpers + string constants for the lite integ tests.
 * When a command is renamed (e.g. /verbosity -> /settings verbosity), update
 * only this file -- all tests import from here.
 */

import type { TestCase } from '../../../src/test-utils/TestCase';

/**
 * Type a slash command char-by-char (the per-char delay avoids the
 * autocomplete menu intercepting Enter), then submit. The integ path is
 * behaviorally load-bearing on `trailingSpace: true` + a longer `postEnterMs`
 * settle — pass those.
 */
export async function typeSlashCommand(
  tc: TestCase,
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

export const CMD_LITE = '/lite';
export const CMD_TUI = '/tui';
export const CMD_CLEAR = '/clear';

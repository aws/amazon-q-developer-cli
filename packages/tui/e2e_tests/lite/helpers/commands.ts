/**
 * Central slash-command string constants for lite TUI tests.
 * When a command is renamed (e.g. /verbosity -> /settings verbosity),
 * update only this file -- all tests import from here.
 */

import type { TestCase } from '../../../src/test-utils/TestCase';
import { E2ETestCase } from '../../E2ETestCase';

type LaunchE2EOpts = {
  terminal?: { width: number; height: number };
  waitTimeout?: number;
  waitForCommands?: boolean;
  getSession?: boolean;
};

/**
 * Launch an e2e E2ETestCase and run the readiness ceremony (wait for prompt,
 * slash-command registry, session id) repeated across every lite e2e test.
 * Mirrors launchLiteInteg on the integ side. lite mode waits for the '>'
 * prompt; tui mode waits for 'ask a question'.
 */
async function launchModeE2E(
  testName: string,
  lite: boolean,
  opts: LaunchE2EOpts
): Promise<E2ETestCase> {
  let builder = E2ETestCase.builder()
    .withTestName(testName)
    .withTerminal(opts.terminal ?? { width: 120, height: 40 });
  if (lite) builder = builder.withLite();
  const tc = await builder.launch();
  await tc.waitForText(
    lite ? '>' : 'ask a question',
    opts.waitTimeout ?? 15000
  );
  if (opts.waitForCommands !== false) await tc.waitForSlashCommands();
  if (opts.getSession !== false) await tc.getSessionId();
  return tc;
}

export const launchLiteE2E = (testName: string, opts: LaunchE2EOpts = {}) =>
  launchModeE2E(testName, true, opts);

export const launchTuiE2E = (testName: string, opts: LaunchE2EOpts = {}) =>
  launchModeE2E(testName, false, opts);

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

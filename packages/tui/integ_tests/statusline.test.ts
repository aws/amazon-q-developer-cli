/**
 * Integration tests for the status-line settings panel.
 *
 * These drive the real TUI through a PTY, so they cover what the unit tests
 * cannot: that the panel is reachable from `/settings`, that it lists every
 * segment, and that a toggle repaints the bar itself and survives to disk.
 *
 * The clock case matters most: it is the only place the surface's own timer runs,
 * and a date that freezes at mount is exactly the staleness this is meant to
 * avoid.
 *
 * Assertions stick to segments the harness actually paints (model, location,
 * branch, clock). Agent, effort and context have no value in a mock session, so
 * hiding them would pass vacuously.
 */

import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { TestCase } from '../src/test-utils/TestCase';

const DOWN_ARROW = '\x1b[B';
const RIGHT_ARROW = '\x1b[C';
const ENTER = '\r';
const ESC = '\x1b';

async function typeSlowly(tc: TestCase, text: string) {
  for (const char of text) {
    await tc.sendKeys(char);
    await tc.sleepMs(30);
  }
  await tc.sleepMs(200);
}

/**
 * Step down until the highlighted row carries `label`, then send `key`.
 *
 * Group headings are not selectable, so walking by highlight rather than by row
 * index keeps this independent of how the list is grouped.
 */
async function moveTo(
  tc: TestCase,
  label: string,
  maxSteps = 20
): Promise<boolean> {
  for (let i = 0; i < maxSteps; i += 1) {
    const highlighted = tc
      .getSnapshot()
      .find((line) => line.includes('❯') && line.includes(label));
    if (highlighted) return true;
    await tc.sendKeys(DOWN_ARROW);
    await tc.sleepMs(140);
  }
  return false;
}

async function activateRow(
  tc: TestCase,
  label: string,
  key: string,
  maxSteps = 20
): Promise<boolean> {
  if (!(await moveTo(tc, label, maxSteps))) return false;
  await tc.sendKeys(key);
  await tc.sleepMs(700);
  return true;
}

/** `/settings` → Display → Status line. */
async function openStatusLinePanel(tc: TestCase) {
  await typeSlowly(tc, '/settings');
  await tc.sendKeys(ENTER);
  await tc.sleepMs(700);
  // A row unique to the settings menu: 'Display' alone also matches the startup
  // tip, which would let this continue before the menu is up.
  await tc.waitForVisibleText('Prompt history scope', 6000);
  if (!(await activateRow(tc, 'Display', ENTER))) {
    throw new Error('Display row not reachable from /settings');
  }
  await tc.waitForVisibleText('Status line', 6000);
  if (!(await activateRow(tc, 'Status line', ENTER))) {
    throw new Error('Status line row not reachable from Display');
  }
  await tc.waitForVisibleText('On by default', 6000);
}

/**
 * ESC back to the chat.
 *
 * Closing a sub-panel returns to `/settings` rather than the chat, so one ESC is
 * not enough to see the bar again.
 */
async function closeToChat(tc: TestCase) {
  for (let i = 0; i < 4; i += 1) {
    await tc.sendKeys(ESC);
    await tc.sleepMs(500);
    if (statusLineText(tc)) return;
  }
}

/**
 * The live status line: the last bar-looking row above the input prompt.
 *
 * Closing a panel leaves the previous frame's rows on screen, so a wider window
 * would let a stale copy of the bar decide an assertion. The bar is recognisable
 * by the working directory, which every configuration shows.
 */
function statusLineText(tc: TestCase): string {
  const lines = tc.getSnapshot();
  const promptIndex = lines.findIndex((line) => line.includes('ask a question'));
  if (promptIndex <= 0) return '';
  for (let i = promptIndex - 1; i >= 0 && i > promptIndex - 8; i -= 1) {
    if (lines[i]?.includes('packages/tui')) return lines[i]!;
  }
  return '';
}

/**
 * Poll the status line until it satisfies `predicate`.
 *
 * A single snapshot taken right after a repaint can still hold the previous
 * frame, which would let a stale line decide the result.
 */
async function waitForStatusLine(
  tc: TestCase,
  predicate: (text: string) => boolean,
  timeoutMs = 6000
): Promise<string> {
  const deadline = Date.now() + timeoutMs;
  let text = statusLineText(tc);
  while (Date.now() < deadline) {
    if (predicate(text)) return text;
    await tc.sleepMs(120);
    text = statusLineText(tc);
  }
  return text;
}

function readSaved(dir: string, surface: 'tui' | 'lite'): unknown {
  const raw = readFileSync(join(dir, 'settings', 'cli.json'), 'utf-8');
  return JSON.parse(raw)[`chat.statusLine.${surface}`];
}

function writeConfig(dir: string, override: Record<string, boolean>): void {
  writeFileSync(
    join(dir, 'settings', 'cli.json'),
    JSON.stringify({ 'chat.statusLine.tui': override })
  );
}

describe('status line settings', () => {
  let testCase: TestCase | null = null;
  let testDir: string;

  beforeEach(() => {
    testDir = join(
      tmpdir(),
      `kiro-statusline-${Date.now()}-${Math.random().toString(36).slice(2)}`
    );
    mkdirSync(join(testDir, 'settings'), { recursive: true });
  });

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    try {
      rmSync(testDir, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
  });

  it('is reachable from /settings and lists every segment', async () => {
    testCase = await TestCase.builder()
      .withTestName('statusline-panel-render')
      .withEnv({ KIRO_HOME: testDir, KIRO_LITE_ROLLOUT_ENABLED: '0' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await openStatusLinePanel(testCase);

    // Walk the whole list so rows past the viewport are seen too.
    // Only rows from the panel: the bar itself shows 'Mock Model', which would
    // otherwise satisfy the 'Model' label on its own.
    const seen = new Set<string>();
    const collect = () => {
      for (const line of testCase!.getSnapshot()) {
        if (/\b(on|off)\b/.test(line) || line.includes('Reset to defaults')) {
          seen.add(line);
        }
      }
    };
    collect();
    for (let i = 0; i < 16; i += 1) {
      await testCase.sendKeys(DOWN_ARROW);
      await testCase.sleepMs(120);
      collect();
    }
    const all = [...seen].join('\n');

    for (const label of [
      'Agent',
      'Autonomous',
      'Model',
      'Effort',
      'Context',
      'Tangent',
      'Code intelligence',
      'Location',
      'Git branch',
      'Goal',
      'Date',
      'Time',
      'Usage',
      'Credits',
      'Reset to defaults',
    ]) {
      expect(all).toContain(label);
    }
    // Grouping states each segment's default; the heading names the surface.
    const screen = testCase.getSnapshot().join('\n');
    expect(screen).toContain('On by default');
    expect(screen).toContain('Off by default');
    expect(screen).toContain('tui status line');

    // Every other panel is in the prompt-bar hint suppression list; a missing
    // entry paints '/copy to clipboard' across this one.
    expect(screen).not.toContain('/copy to clipboard');

    // The bar stays up with the panel open, so a toggle can be seen taking
    // effect. The input is hidden, so the bar is found by its own content.
    const barLine = testCase
      .getSnapshot()
      .find((l) => l.includes('packages/tui') && l.includes('Mock Model'));
    expect(barLine).toBeDefined();
  }, 60000);

  it('enabling the clock puts a live time in the bar', async () => {
    testCase = await TestCase.builder()
      .withTestName('statusline-clock')
      .withEnv({ KIRO_HOME: testDir, KIRO_LITE_ROLLOUT_ENABLED: '0' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);
    expect(statusLineText(testCase)).not.toMatch(/\b\d{2}:\d{2}\b/);

    await openStatusLinePanel(testCase);
    expect(await activateRow(testCase, 'Time', RIGHT_ARROW)).toBe(true);
    await closeToChat(testCase);

    // The surface's own timer supplies this; nothing passes `now` in a real run.
    expect(
      await waitForStatusLine(testCase, (t) => /\b\d{2}:\d{2}\b/.test(t))
    ).toMatch(/\b\d{2}:\d{2}\b/);
    expect(readSaved(testDir, 'tui')).toEqual({ time: true });
  }, 60000);

  it('turning a shown segment off removes it from the bar and persists', async () => {
    testCase = await TestCase.builder()
      .withTestName('statusline-toggle-off')
      .withEnv({ KIRO_HOME: testDir, KIRO_LITE_ROLLOUT_ENABLED: '0' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);
    expect(statusLineText(testCase)).toContain('Mock Model');

    await openStatusLinePanel(testCase);
    expect(await activateRow(testCase, 'Model', RIGHT_ARROW)).toBe(true);
    await closeToChat(testCase);

    // Requiring the cwd keeps an empty capture from satisfying the negative.
    const bar = await waitForStatusLine(
      testCase,
      (t) => t.includes('packages/tui') && !t.includes('Mock Model')
    );
    expect(bar).toContain('packages/tui');
    expect(bar).not.toContain('Mock Model');
    expect(readSaved(testDir, 'tui')).toEqual({ model: false });
  }, 60000);

  it('enter applies the highlighted row and closes', async () => {
    testCase = await TestCase.builder()
      .withTestName('statusline-enter')
      .withEnv({ KIRO_HOME: testDir, KIRO_LITE_ROLLOUT_ENABLED: '0' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await openStatusLinePanel(testCase);
    // Enter both toggles and leaves, so one keypress has to do both.
    expect(await activateRow(testCase, 'Model', ENTER)).toBe(true);

    const bar = await waitForStatusLine(
      testCase,
      (t) => t.includes('packages/tui') && !t.includes('Mock Model')
    );
    expect(bar).not.toContain('Mock Model');
    expect(readSaved(testDir, 'tui')).toEqual({ model: false });
  }, 60000);

  it('resets from a toggle key and names that in the footer', async () => {
    // A row ignoring the toggle keys reads as a broken key, and a footer
    // promising 'toggle' there would be wrong.
    writeConfig(testDir, { model: false, date: true });
    testCase = await TestCase.builder()
      .withTestName('statusline-reset-arrow')
      .withEnv({ KIRO_HOME: testDir, KIRO_LITE_ROLLOUT_ENABLED: '0' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await openStatusLinePanel(testCase);
    expect(await moveTo(testCase, 'Reset to defaults')).toBe(true);
    expect(testCase.getSnapshot().join('\n')).toContain(
      'to restore the defaults'
    );

    await testCase.sendKeys(RIGHT_ARROW);
    await testCase.sleepMs(600);

    expect(readSaved(testDir, 'tui')).toEqual({});
  }, 60000);

  it('reset restores the defaults', async () => {
    writeConfig(testDir, { model: false, date: true });
    testCase = await TestCase.builder()
      .withTestName('statusline-reset')
      .withEnv({ KIRO_HOME: testDir, KIRO_LITE_ROLLOUT_ENABLED: '0' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);
    expect(statusLineText(testCase)).not.toContain('Mock Model');

    await openStatusLinePanel(testCase);
    expect(await activateRow(testCase, 'Reset to defaults', ENTER)).toBe(true);

    const bar = await waitForStatusLine(testCase, (t) =>
      t.includes('Mock Model')
    );
    expect(bar).toContain('Mock Model');
    expect(readSaved(testDir, 'tui')).toEqual({});
  }, 60000);

  it('edits the lite list and reaches the packed bar', async () => {
    // Lite packs one flat run and keeps its own list, so the full TUI passing
    // proves nothing about it. This is also the rollout target.
    testCase = await TestCase.builder()
      .withTestName('statusline-lite')
      .withEnv({ KIRO_HOME: testDir, KIRO_LITE_ROLLOUT_ENABLED: '1' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await typeSlowly(testCase, '/lite');
    await testCase.sendKeys(ENTER);
    await testCase.sleepMs(1500);
    await testCase.waitForVisibleText('ask a question', 10000);
    expect(statusLineText(testCase)).toContain('Mock Model');

    await openStatusLinePanel(testCase);
    // The heading names the surface being edited, which is how a user knows the
    // two lists are separate.
    expect(testCase.getSnapshot().join('\n')).toContain('lite status line');
    // Lite has no code-intelligence renderer, so the row must not be offered.
    expect(testCase.getSnapshot().join('\n')).not.toContain(
      'Code intelligence'
    );

    expect(await activateRow(testCase, 'Model', RIGHT_ARROW)).toBe(true);
    await closeToChat(testCase);

    const bar = await waitForStatusLine(
      testCase,
      (t) => t.includes('packages/tui') && !t.includes('Mock Model')
    );
    expect(bar).not.toContain('Mock Model');
    expect(readSaved(testDir, 'lite')).toEqual({ model: false });
  }, 90000);

  it('a segment hidden in the config never reaches the bar', async () => {
    // Model rather than the branch: the branch chip is absent on a detached
    // checkout, which would make the negative pass without proving anything.
    writeConfig(testDir, { model: false });
    testCase = await TestCase.builder()
      .withTestName('statusline-hidden')
      .withEnv({ KIRO_HOME: testDir, KIRO_LITE_ROLLOUT_ENABLED: '0' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    const bar = statusLineText(testCase);
    expect(bar).toContain('packages/tui');
    expect(bar).not.toContain('Mock Model');
  }, 45000);
});

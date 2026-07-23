/**
 * Integration tests for the Display settings panel (/settings display).
 *
 * Tests cover:
 * - Panel renders with correct items and default values
 * - ASCII art toggle updates and persists
 * - Animations toggle persists
 * - Arrow key navigation highlights items
 * - Pre-existing settings are respected on open
 */

import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { mkdirSync, rmSync, writeFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { TestCase } from '../src/test-utils/TestCase';

const DOWN_ARROW = '\x1b[B';
const RIGHT_ARROW = '\x1b[C';
const ENTER = '\r';

async function typeSlowly(tc: TestCase, text: string) {
  for (const char of text) {
    await tc.sendKeys(char);
    await tc.sleepMs(30);
  }
  await tc.sleepMs(200);
}

async function openDisplaySettings(tc: TestCase) {
  await typeSlowly(tc, '/settings display');
  await tc.sendKeys(ENTER);
  await tc.sleepMs(500);
}

describe('Display settings panel', () => {
  let testCase: TestCase | null = null;
  let testDir: string;

  beforeEach(() => {
    testDir = join(
      tmpdir(),
      `kiro-display-settings-${Date.now()}-${Math.random().toString(36).slice(2)}`
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

  it('renders with correct items and default values', async () => {
    testCase = await TestCase.builder()
      .withTestName('display-settings-render')
      .withEnv({ KIRO_HOME: testDir, KIRO_LITE_ROLLOUT_ENABLED: '0' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await openDisplaySettings(testCase);
    await testCase.waitForVisibleText('Animations', 5000);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('/settings');
    expect(snap).toContain('display');
    expect(snap).toContain('Animations');
    expect(snap).toContain('ASCII art');
    expect(snap).toContain('Icons');
    // Default values: Animations on, ASCII art on (inverted: asciiMode=false shows as on), Icons on
    expect(snap).toMatch(/Animations\s+on/);
    expect(snap).toMatch(/ASCII art\s+on/);
    expect(snap).toMatch(/Icons\s+on/);
  }, 30000);

  it('toggling ASCII art updates UI', async () => {
    testCase = await TestCase.builder()
      .withTestName('display-settings-ascii-toggle')
      .withEnv({ KIRO_HOME: testDir, KIRO_LITE_ROLLOUT_ENABLED: '0' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await openDisplaySettings(testCase);
    await testCase.waitForVisibleText('Animations', 5000);

    // ASCII art is the second item (after Animations); navigate down once then toggle.
    // (The "Default UI" row is gated to the lite rollout cohort, absent here.)
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(200);
    await testCase.sendKeys(RIGHT_ARROW);
    await testCase.sleepMs(500);

    // Verify the toggle changed in the UI (on -> off)
    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toMatch(/ASCII art\s+off/);
  }, 30000);

  it('toggling animations updates UI', async () => {
    testCase = await TestCase.builder()
      .withTestName('display-settings-animations-toggle')
      .withEnv({ KIRO_HOME: testDir, KIRO_LITE_ROLLOUT_ENABLED: '0' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await openDisplaySettings(testCase);
    await testCase.waitForVisibleText('Animations', 5000);

    // Animations is the first item (the gated "Default UI" row is absent here),
    // so it's already highlighted on open; toggle without navigating.
    await testCase.sendKeys(RIGHT_ARROW);
    await testCase.sleepMs(500);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toMatch(/Animations\s+off/);
  }, 30000);

  it('arrow key navigation highlights items with descriptions', async () => {
    testCase = await TestCase.builder()
      .withTestName('display-settings-navigation')
      .withEnv({ KIRO_HOME: testDir, KIRO_LITE_ROLLOUT_ENABLED: '0' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await openDisplaySettings(testCase);
    await testCase.waitForVisibleText('Animations', 5000);

    // First item (Animations) should show its description
    let snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('Spinners, progress bars');

    // Navigate down to ASCII art
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(300);

    snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('Decorative text art');

    // Navigate down to Icons
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(300);

    snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('Symbols for status');

    // Navigate down to Terminal title
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(300);

    snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('update terminal window title');
  }, 30000);

  it('respects pre-existing settings on open', async () => {
    // Write settings before launching: allowAsciiArt=false means display shows "off"
    const settingsPath = join(testDir, 'settings', 'cli.json');
    writeFileSync(
      settingsPath,
      JSON.stringify({ 'chat.allowAsciiArt': false }),
      'utf-8'
    );

    testCase = await TestCase.builder()
      .withTestName('display-settings-preexisting')
      .withEnv({ KIRO_HOME: testDir, KIRO_LITE_ROLLOUT_ENABLED: '0' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await openDisplaySettings(testCase);
    await testCase.waitForVisibleText('Animations', 5000);

    const snap = testCase.getSnapshot().join('\n');
    // ASCII art should show "off" since allowAsciiArt=false
    expect(snap).toMatch(/ASCII art\s+off/);
  }, 30000);

  it('Enter closes the panel entirely', async () => {
    testCase = await TestCase.builder()
      .withTestName('display-settings-enter-closes')
      .withEnv({ KIRO_HOME: testDir, KIRO_LITE_ROLLOUT_ENABLED: '0' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await openDisplaySettings(testCase);
    await testCase.waitForVisibleText('Animations', 5000);

    // Press Enter — should dismiss the panel back to chat
    await testCase.sendKeys(ENTER);
    await testCase.sleepMs(500);

    const snap = testCase.getSnapshot().join('\n');
    // Panel should be gone — its settings items are no longer visible.
    // (Assert on a panel-only item rather than "/settings": the welcome area
    // shows a rotating startup tip that can itself mention "/settings".)
    expect(snap).not.toContain('Animations');
    expect(snap).toContain('ask a question');
  }, 30000);
});

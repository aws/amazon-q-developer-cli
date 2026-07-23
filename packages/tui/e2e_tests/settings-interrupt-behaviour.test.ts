/**
 * E2E test: `/settings` → Terminal → Interrupt behaviour → Queue.
 *
 * Exercises the full overlay flow end-to-end through the real TUI:
 *   1. `/settings` opens the SettingsPanel (Explorer overlay).
 *   2. ↓↓ + Enter selects the Terminal row, opening the terminal sub-screen.
 *   3. ↓ + Enter selects Interrupt behaviour, opening the steer/queue screen.
 *   4. ↓ + Enter selects Queue, persisting the default and closing.
 *
 * Regression coverage the unit tests can't give us: this proves the
 * Interrupt behaviour option is actually reachable by a user clicking
 * through the visual panel (not just present in the pure model), and that
 * the choice persists to `~/.kiro/settings/cli.json`. The option went
 * missing once because the panel's Terminal row jumped straight into the
 * newlines setup — this test fails loudly if that happens again.
 *
 * A second test verifies the inverse: ESC cancels the flow one level at a
 * time and leaves the persisted setting untouched (highlighting a row is
 * not the same as applying it).
 */

import { afterEach, describe, expect, it } from 'bun:test';
import * as fs from 'fs';
import * as path from 'path';
import { E2ETestCase } from './E2ETestCase';

const DOWN_ARROW = '\x1b[B';

describe('/settings → Terminal → Interrupt behaviour', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('navigates to interrupt behaviour and persists the Queue choice', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('settings-interrupt-queue')
      .withTerminal({ width: 120, height: 40 })
      .launch();

    await testCase.waitForText('ask a question', 15000);

    // Open /settings.
    await testCase.sendKeys('/settings');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Top-level rows: Display / Verbosity / Theme / Terminal / Keybindings / History.
    await testCase.waitForText('Keybindings', 10000);

    // Terminal is row index 3 (Verbosity is spliced in after Display) — three ↓
    // from the default cursor lands on it.
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(120);
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(120);
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(120);
    await testCase.pressEnter();

    // Terminal sub-screen shows Newlines + Interrupt behaviour. The
    // breadcrumb title confirms we navigated rather than firing newlines
    // setup directly.
    await testCase.waitForText('/settings – terminal', 10000);
    await testCase.waitForText('Interrupt behaviour', 10000);

    // Interrupt behaviour is row index 1 — one ↓ then Enter.
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(120);
    await testCase.pressEnter();

    // Interrupt sub-screen: Steer / Queue.
    await testCase.waitForText('/settings – interrupt behaviour', 10000);
    await testCase.waitForText('Queue', 10000);

    // Steer is active by default (●). Queue is row index 1 — one ↓ then Enter.
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(120);
    await testCase.pressEnter();

    // Apply surfaces a transient confirmation and closes the overlay.
    await testCase.waitForText('Interrupt behaviour: queue', 10000);
    await testCase.sleepMs(500);

    const store = await testCase.getStore();
    expect(
      (store as unknown as { showSettingsPanel: boolean }).showSettingsPanel
    ).toBe(false);

    // Choice persists for the next session under $HOME/.kiro/settings.
    const prefsPath = path.join(
      testCase.sandboxDir,
      '.kiro',
      'settings',
      'cli.json'
    );
    expect(fs.existsSync(prefsPath)).toBe(true);
    const prefs = JSON.parse(fs.readFileSync(prefsPath, 'utf-8'));
    expect(prefs['chat.defaultInterruptBehavior']).toBe('queue');
  }, 30000);

  it('ESC out of the interrupt sub-screen cancels without changing the setting', async () => {
    // Seed an explicit default so we can prove ESC leaves it untouched.
    testCase = await E2ETestCase.builder()
      .withTestName('settings-interrupt-esc-cancel')
      .withTerminal({ width: 120, height: 40 })
      .withGlobalSettings({ 'chat.defaultInterruptBehavior': 'steer' })
      .launch();

    await testCase.waitForText('ask a question', 15000);

    // Open /settings and drill down: Terminal → Interrupt behaviour.
    await testCase.sendKeys('/settings');
    await testCase.sleepMs(100);
    await testCase.pressEnter();
    await testCase.waitForText('Keybindings', 10000);

    // Terminal is row index 3 (Verbosity is spliced in after Display).
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(120);
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(120);
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(120);
    await testCase.pressEnter();
    await testCase.waitForText('/settings – terminal', 10000);

    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(120);
    await testCase.pressEnter();
    await testCase.waitForText('/settings – interrupt behaviour', 10000);

    // Move the cursor onto Queue but DON'T press Enter — ESC instead.
    // Highlighting a row must not mutate the setting; only Enter applies.
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(120);

    // ESC walks back one level: interrupt → terminal (not straight to top
    // and not closing the overlay).
    await testCase.pressEscape();
    await testCase.sleepMs(300);
    await testCase.waitForText('/settings – terminal', 10000);

    // ESC again: terminal → top.
    await testCase.pressEscape();
    await testCase.sleepMs(300);
    await testCase.waitForText('Keybindings', 10000);

    // ESC at top closes the overlay entirely.
    await testCase.pressEscape();
    await testCase.sleepMs(300);
    const store = await testCase.getStore();
    expect(
      (store as unknown as { showSettingsPanel: boolean }).showSettingsPanel
    ).toBe(false);

    // The setting must be unchanged: either still 'steer' from the seed, or
    // the key was never written. Cancelling via ESC applies nothing.
    const prefsPath = path.join(
      testCase.sandboxDir,
      '.kiro',
      'settings',
      'cli.json'
    );
    const prefs = fs.existsSync(prefsPath)
      ? JSON.parse(fs.readFileSync(prefsPath, 'utf-8'))
      : {};
    expect(prefs['chat.defaultInterruptBehavior']).toBe('steer');
  }, 30000);
});

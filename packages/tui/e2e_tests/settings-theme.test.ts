/**
 * E2E test: `/settings` → Theme → Dark.
 *
 * Exercises the full overlay flow end-to-end:
 *   1. `/settings` opens the SettingsPanel (Explorer overlay).
 *   2. ↓ + Enter on the Theme row opens the ThemePanel.
 *   3. ↓ + Enter on Dark theme applies the bundled theme.
 *
 * Verifies:
 *   - Both panels close once apply completes.
 *   - The bundled-theme apply persists `~/.kiro/settings/kiro_cli_theme.json`
 *     with `baseTheme: "dark"` so the choice survives across launches.
 *
 * Catches regressions that the unit tests can't — e.g. SettingsPanel
 * routing, panel mount/unmount on close, ESC back-navigation wiring,
 * and the dispatcher path that bridges the slash command into the
 * panel state.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import * as fs from 'fs';
import * as path from 'path';
import { E2ETestCase } from './E2ETestCase';

const DOWN_ARROW = '\x1b[B';

describe('/settings → Theme → Dark', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('opens the panel chain, applies Dark theme, and persists prefs', async () => {
    testCase = await E2ETestCase.builder()
      .withTestName('settings-theme-dark')
      .withTerminal({ width: 120, height: 40 })
      .launch();

    await testCase.waitForText('ask a question', 15000);

    // Open /settings — sleep between sendKeys and pressEnter so the
    // input handler has a moment to register the typed text before
    // we submit, mirroring the pattern in `terminal-title.test.ts`.
    await testCase.sendKeys('/settings');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for a row label that's unique to the SettingsPanel
    // top-level — `Theme`, `Keybindings`, `History` etc. — rather
    // than the bare `/settings` string which also appears in the
    // command line itself.
    await testCase.waitForText('Keybindings', 10000);

    // Top-level row order: Display / Verbosity / Theme / Terminal /
    // Keybindings / History — Theme is row index 2 (Verbosity, a TUI peer,
    // sits after Display), so two ↓ from the default cursor land on it.
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(150);
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(150);
    await testCase.pressEnter();

    // ThemePanel title carries the full `/settings – theme` breadcrumb.
    await testCase.waitForText('/settings – theme', 10000);

    // Theme row order: Auto / Dark theme / Light theme / Custom — Dark
    // is row index 1.
    await testCase.sendKeys(DOWN_ARROW);
    await testCase.sleepMs(150);
    await testCase.pressEnter();

    // Apply path surfaces a transient confirmation. Bundled-theme
    // labels are user-visible strings — see `bundledThemes` in
    // `theme/user-theme.ts`.
    await testCase.waitForText('Theme set to Dark theme', 10000);
    await testCase.sleepMs(500);

    // Apply is the user's final confirmation — per the UX-team spec
    // (Larissa + Zoe agreement), reaching the last layer and
    // confirming closes the entire overlay. ESC mid-flow still walks
    // back one level. So both panels should be closed on success.
    const store = await testCase.getStore();
    expect(
      (store as unknown as { showThemePanel: boolean }).showThemePanel
    ).toBe(false);
    expect(
      (store as unknown as { showSettingsPanel: boolean }).showSettingsPanel
    ).toBe(false);

    // Prefs should persist for the next session. Path mirrors
    // `getThemePath()` in `theme/user-theme.ts` — `$HOME/.kiro/settings/`
    // resolves to the sandbox dir under E2E.
    const prefsPath = path.join(
      testCase.sandboxDir,
      '.kiro',
      'settings',
      'kiro_cli_theme.json'
    );
    expect(fs.existsSync(prefsPath)).toBe(true);
    const prefs = JSON.parse(fs.readFileSync(prefsPath, 'utf-8'));
    expect(prefs.baseTheme).toBe('dark');
    // Bundled dark theme also locks in a response and diff preset.
    expect(prefs.responsePreset).toBe('light');
    expect(prefs.diffPreset).toBe('dark');
  }, 20000); // overall test timeout — overlay flow has multiple sleeps
});

import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { dispatch } from '../dispatcher';
import type { SlashCommand } from '../../stores/app-store';
import { mkdirSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { createMockCommandContext } from './test-helpers.js';

const settingsCmd: SlashCommand = {
  name: '/settings',
  description: 'Configure theme, terminal, keybindings, and other preferences',
  source: 'local',
  meta: { local: true },
};

// showThemeMenu looks up the /theme SlashCommand from ctx.slashCommands when
// building its menu, so tests that delegate into it must register /theme too.
const themeCmd: SlashCommand = {
  name: '/theme',
  description: 'Select a theme that looks best for your terminal',
  source: 'local',
  meta: { local: true },
};

describe('/settings command', () => {
  let testDir: string;
  let originalHome: string | undefined;

  // Theme handler reads/writes user theme prefs; redirect HOME so tests
  // don't touch the developer's real config.
  beforeEach(() => {
    testDir = join(
      tmpdir(),
      `kiro-settings-cmd-test-${process.pid}-${Date.now()}-${Math.random().toString(36).slice(2)}`
    );
    mkdirSync(join(testDir, '.kiro', 'settings'), { recursive: true });
    originalHome = process.env.HOME;
    process.env.HOME = testDir;
  });

  afterEach(() => {
    process.env.HOME = originalHome;
    try {
      rmSync(testDir, { recursive: true, force: true });
    } catch {
      // ignore
    }
  });

  describe('bare /settings (no args)', () => {
    it('opens the SettingsPanel overlay', async () => {
      // The /settings UI now lives in <SettingsPanel> (an Explorer-based
      // overlay), not in the slash-command active-command machinery.
      // Bare /settings just flips the panel state on; the panel itself
      // owns the row list and routing.
      const ctx = createMockCommandContext({ slashCommands: [settingsCmd] });
      await dispatch(settingsCmd, '', ctx);

      expect(ctx._spies.setShowSettingsPanel!).toHaveBeenCalled();
      expect(ctx._spies.setShowSettingsPanel!.mock.calls[0]![0]).toBe(true);
      // We no longer route through setActiveCommand for the top-level
      // /settings menu — keep this assertion as a regression guard.
      expect(ctx._spies.setActiveCommand!).not.toHaveBeenCalled();
    });
  });

  describe('/settings <subcommand>', () => {
    it('routes /settings theme by opening ThemePanel without firing the /theme deprecation alert', async () => {
      const ctx = createMockCommandContext({
        slashCommands: [settingsCmd, themeCmd],
      });
      await dispatch(settingsCmd, 'theme', ctx);

      // Theme flow now lives in <ThemePanel> — the subcommand handler
      // surfaces it via setShowThemePanel(true) rather than a Menu/active
      // command sub-screen.
      expect(ctx._spies.setShowThemePanel!).toHaveBeenCalled();
      expect(ctx._spies.setShowThemePanel!.mock.calls[0]![0]).toBe(true);

      // Deprecation alert must NOT fire on this chained path — it would
      // be noisy and, more importantly, would break the "/theme has
      // moved" contract (that alert is only meant for direct /theme use).
      const alertCalls = ctx._spies.showAlert!.mock.calls;
      for (const [message] of alertCalls) {
        expect(String(message)).not.toContain('moved to /settings theme');
      }
    });

    it('shows an error for an unknown subcommand', async () => {
      const ctx = createMockCommandContext({ slashCommands: [settingsCmd] });
      await dispatch(settingsCmd, 'nonsense', ctx);

      expect(ctx._spies.showAlert!).toHaveBeenCalled();
      const [message, status] = ctx._spies.showAlert!.mock.calls[0]!;
      expect(String(message)).toContain('Unknown settings subcommand');
      expect(status).toBe('error');

      // We don't open any menu on an unknown subcommand
      expect(ctx._spies.setActiveCommand!).not.toHaveBeenCalled();
    });
  });

  describe('ESC-back-to-/settings flag', () => {
    // Every known subcommand must set settingsReturnOnEscape(true) so that
    // the subsequent ESC returns to the /settings menu instead of dismissing.
    // This is the hook the overlay close handlers read to decide whether to
    // re-open /settings. See CommandMenu.tsx onEscape / handleCloseKeybindingsPanel.
    it('sets settingsReturnOnEscape=true when routing to theme', async () => {
      const ctx = createMockCommandContext({
        slashCommands: [settingsCmd, themeCmd],
      });
      await dispatch(settingsCmd, 'theme', ctx);

      expect(ctx._spies.setSettingsReturnOnEscape!).toHaveBeenCalled();
      const call = ctx._spies.setSettingsReturnOnEscape!.mock.calls[0]!;
      expect(call[0]).toBe(true);
    });

    it('sets settingsReturnOnEscape=true when routing to keybindings', async () => {
      const ctx = createMockCommandContext({ slashCommands: [settingsCmd] });
      await dispatch(settingsCmd, 'keybindings', ctx);

      expect(ctx._spies.setSettingsReturnOnEscape!).toHaveBeenCalled();
      const call = ctx._spies.setSettingsReturnOnEscape!.mock.calls[0]!;
      expect(call[0]).toBe(true);
    });

    it('does not set the flag when the subcommand is unknown', async () => {
      const ctx = createMockCommandContext({ slashCommands: [settingsCmd] });
      await dispatch(settingsCmd, 'nonsense', ctx);

      // Unknown subcommand should not prime the overlay to return to /settings
      // — otherwise a stale flag would change ESC behavior on the next overlay.
      expect(ctx._spies.setSettingsReturnOnEscape!).not.toHaveBeenCalled();
    });
  });
});

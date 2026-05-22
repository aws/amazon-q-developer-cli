import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { dispatch } from '../dispatcher';
import type { SlashCommand } from '../../stores/app-store';
import { mkdirSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { createMockCommandContext } from './test-helpers.js';
import { settingsSubcommands } from '../settings-subcommands.js';

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
    it('opens a selection menu listing every registered subcommand', async () => {
      const ctx = createMockCommandContext({ slashCommands: [settingsCmd] });
      await dispatch(settingsCmd, '', ctx);

      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      const { options } = call[0];

      // One menu option per top-level subcommand (sub-options with ':' are nested)
      const topLevel = settingsSubcommands.filter(
        (s) => !s.value.includes(':')
      );
      expect(options).toHaveLength(topLevel.length);
      for (let i = 0; i < topLevel.length; i++) {
        expect(options[i].value).toBe(topLevel[i]!.value);
        expect(options[i].label).toBe(topLevel[i]!.label);
      }
    });

    it('renders as a non-searchable selection menu', async () => {
      const ctx = createMockCommandContext({ slashCommands: [settingsCmd] });
      await dispatch(settingsCmd, '', ctx);

      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      const { meta } = call[0].command;
      expect(meta.inputType).toBe('selection');
      expect(meta.searchable).toBe(false);
    });
  });

  describe('/settings <subcommand>', () => {
    it('routes /settings theme to the theme menu without firing the /theme deprecation alert', async () => {
      const ctx = createMockCommandContext({
        slashCommands: [settingsCmd, themeCmd],
      });
      await dispatch(settingsCmd, 'theme', ctx);

      // Theme menu opens (setActiveCommand called with theme options)
      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();

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

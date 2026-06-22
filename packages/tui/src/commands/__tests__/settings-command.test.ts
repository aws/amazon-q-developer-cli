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

// verbosityConfig (the effect /settings verbosity delegates into) resolves
// the canonical /verbosity command from ctx.slashCommands so the menu chip
// stays /verbosity regardless of entry path. Tests that exercise the
// /settings → verbosity routing path must register this command too.
const verbosityCmd: SlashCommand = {
  name: '/verbosity',
  description:
    'Configure lite-mode rendering: tool args, reasoning, output filters, density, subagent sections.',
  source: 'local',
  meta: { local: true, liteOnly: true },
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
    // Both modes render the SAME shared <SettingsPanel> overlay (lite via
    // <BackendPanels>), so breadcrumb titles / heights / ESC-back match. The
    // lite-only verbosity row is added inside the panel model (gated on
    // uiMode), not via the command-menu — so bare /settings just flips the
    // panel flag in either mode and never opens a command-menu.
    it.each(['tui', 'lite'] as const)(
      'opens the SettingsPanel overlay in %s mode (not the command-menu)',
      async (uiMode) => {
        const ctx = createMockCommandContext({ slashCommands: [settingsCmd] });
        (ctx as any).getUiMode = () => uiMode;
        await dispatch(settingsCmd, '', ctx);

        expect(ctx._spies.setShowSettingsPanel!).toHaveBeenCalled();
        expect(ctx._spies.setShowSettingsPanel!.mock.calls[0]![0]).toBe(true);
        expect(ctx._spies.setActiveCommand!).not.toHaveBeenCalled();
      }
    );
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

    it('routes /settings verbosity to the verbosity menu with the canonical chip name (lite mode)', async () => {
      // 1:1 wiring contract: reaching the verbosity menu via /settings →
      // verbosity must produce an activeCommand with `command.name ===
      // '/verbosity'` so CommandMenu's verbosity-specific UI wires up the
      // same way as direct entry. CommandMenu has three checks against
      // `command.name === '/verbosity'`:
      //   - Reset preview state when leaving /verbosity
      //   - Gate Ctrl+P / p preview-toggle hotkeys
      //   - Track the highlighted density preset for inline preview
      // If the chip says `/settings`, all three silently fail and the
      // user can't open the preview pane or see draft preset previews.
      // The verbosityConfig handler resolves the canonical /verbosity
      // SlashCommand internally so the chip name stays right regardless
      // of entry path; this test locks that behavior in.
      const ctx = createMockCommandContext({
        slashCommands: [settingsCmd, verbosityCmd],
      });
      (ctx as any).getUiMode = () => 'lite';
      await dispatch(settingsCmd, 'verbosity', ctx);

      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      const arg = call[0] as {
        command: SlashCommand;
        options: Array<{ value: string; label: string }>;
        previewKey?: string;
      };

      // Chip name is the canonical /verbosity, not /settings.
      expect(arg.command.name).toBe('/verbosity');

      // Opened in the same shape as a direct /verbosity entry: previewKey set
      // and the rows are a recognizable verbosity menu (density or config).
      expect(arg.previewKey).toBeTruthy();
      const labels = arg.options.map((o) => o.label);
      const isDensityMenu =
        labels.includes('default') || labels.includes('full');
      const isConfigMenu = labels.includes('Tool calls');
      expect(isDensityMenu || isConfigMenu).toBe(true);
    });

    it('forwards a trailing section into /settings verbosity <section> (lite)', async () => {
      // Nested typed access: `/settings verbosity truncation` should drill
      // straight into the verbosity truncation menu, mirroring the breadcrumb.
      // The verbosity subcommand forwards the tail to verbosityConfig, which
      // opens the truncation sub-menu (back-link encodes the section).
      const ctx = createMockCommandContext({
        slashCommands: [settingsCmd, verbosityCmd],
      });
      (ctx as any).getUiMode = () => 'lite';
      await dispatch(settingsCmd, 'verbosity truncation', ctx);

      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
      const call = ctx._spies.setActiveCommand!.mock.calls.at(-1)!;
      const arg = call[0] as { options: Array<{ value: string }> };
      const values = arg.options.map((o) => o.value);
      expect(values).toContain('menu:top:truncation');
      // Still primes the ESC-back-to-/settings flag.
      expect(ctx._spies.setSettingsReturnOnEscape!).toHaveBeenCalledWith(true);
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
    // verbosity needs /verbosity registered + lite mode so the inner handler
    // reaches its menu-build path; we still assert the wrapper, not the handler.
    it.each([
      { sub: 'theme', extra: [themeCmd], lite: false },
      { sub: 'keybindings', extra: [], lite: false },
      { sub: 'verbosity', extra: [verbosityCmd], lite: true },
    ])(
      'sets settingsReturnOnEscape=true when routing to $sub',
      async ({ sub, extra, lite }) => {
        const ctx = createMockCommandContext({
          slashCommands: [settingsCmd, ...extra],
        });
        if (lite) (ctx as any).getUiMode = () => 'lite';
        await dispatch(settingsCmd, sub, ctx);

        expect(ctx._spies.setSettingsReturnOnEscape!).toHaveBeenCalled();
        expect(ctx._spies.setSettingsReturnOnEscape!.mock.calls[0]![0]).toBe(
          true
        );
      }
    );

    it('does not set the flag when the subcommand is unknown', async () => {
      const ctx = createMockCommandContext({ slashCommands: [settingsCmd] });
      await dispatch(settingsCmd, 'nonsense', ctx);

      // Unknown subcommand should not prime the overlay to return to /settings
      // — otherwise a stale flag would change ESC behavior on the next overlay.
      expect(ctx._spies.setSettingsReturnOnEscape!).not.toHaveBeenCalled();
    });
  });

  // default-ui folded into the settings-display submenu (1e0a8a8c2); coverage lives there.
});

/**
 * Registry of /settings subcommands.
 *
 * Each entry declares its menu presentation and its `handle` function.
 * Handlers can delegate to an effect (e.g. `showThemeMenu`), open a panel,
 * or run local logic directly.
 *
 * To add a new subcommand, add an entry to `settingsSubcommands` below.
 */

import type { CommandContext } from './types.js';
import type { AvailableCommand } from '../types/commands.js';
import type { EffectHandler } from './effects.js';
import { setupTerminal } from '../utils/terminal-setup.js';
import { Settings } from '../constants/settings.js';
import { readStringSetting } from '../utils/cli-settings.js';

export interface SettingsSubcommand {
  /** Machine value passed as `/settings <value>` */
  value: string;
  /** Menu label */
  label: string;
  /** Menu description */
  description: string;
  /** Dispatch logic for this subcommand */
  handle: (ctx: SettingsHandleContext) => void | Promise<void>;
}

/**
 * Narrowed context passed to subcommand handlers. Avoids pulling in the
 * entire CommandContext surface so handlers stay focused.
 */
export interface SettingsHandleContext {
  ctx: CommandContext;
  /** The /settings SlashCommand object (not /theme or any other subcommand). */
  settingsCommand: AvailableCommand;
  /**
   * Look up an effect handler by name. Throws if the effect does not exist,
   * which makes misspellings a build/run error rather than a silent no-op.
   */
  resolveEffect: (name: string) => EffectHandler;
}

export const settingsSubcommands: readonly SettingsSubcommand[] = [
  {
    value: 'display',
    label: 'display',
    description: 'Control animations, ASCII art, and icons',
    handle: ({ ctx }) => {
      ctx.setSettingsReturnOnEscape(true);
      ctx.setShowDisplaySettingsPanel(true);
    },
  },
  {
    value: 'theme',
    label: 'theme',
    description: 'Colors, prompt style, diff styling',
    handle: ({ ctx, settingsCommand, resolveEffect }) => {
      ctx.setSettingsReturnOnEscape(true);

      // Pass the /settings cmd object (not /theme) so showThemeMenu's
      // deprecation alert (gated on cmd.name === '/theme') stays silent.
      resolveEffect('showThemeMenu')(null, ctx, settingsCommand, '');
    },
  },
  {
    value: 'terminal',
    label: 'terminal',
    description: 'Shift+Enter / Option+Enter for newlines',
    handle: async ({ ctx }) => {
      // Final-decision subcommand: execute, surface the result as a transient
      // alert, close overlay. Setup logic lives in utils/terminal-setup.ts.
      ctx.setLoadingMessage('Configuring terminal…');
      try {
        const result = await setupTerminal();
        ctx.setLoadingMessage(null);
        if (result.message) {
          ctx.showAlert(
            result.message,
            result.success ? 'success' : 'error',
            alertDurationFor(result.message)
          );
        }
      } catch (error) {
        ctx.setLoadingMessage(null);
        const message =
          error instanceof Error ? error.message : 'Terminal setup failed';
        ctx.showAlert(message, 'error', alertDurationFor(message));
      }
    },
  },
  {
    value: 'keybindings',
    label: 'keybindings',
    description: 'Customize keyboard shortcuts',
    handle: ({ ctx }) => {
      ctx.setSettingsReturnOnEscape(true);
      ctx.setShowKeybindingsPanel(true);
    },
  },
  {
    value: 'history',
    label: 'history',
    description: 'Prompt history scope (session or global)',
    handle: ({ ctx, settingsCommand }) => {
      const current = readStringSetting(Settings.CHAT_HISTORY_MODE, 'session');
      ctx.setSettingsReturnOnEscape(true);
      ctx.setActiveCommand({
        command: {
          ...settingsCommand,
          meta: { ...settingsCommand.meta, inputType: 'selection' as const, searchable: false },
        },
        options: [
          { value: 'history:session', label: `session${current === 'session' ? ' ●' : ''}`, description: 'Each session has its own prompt history' },
          { value: 'history:global', label: `global${current === 'global' ? ' ●' : ''}`, description: 'All sessions share one prompt history' },
        ],
      });
    },
  },
  {
    value: 'history:session',
    label: 'session',
    description: 'Each session has its own prompt history',
    handle: async ({ ctx }) => {
      await ctx.kiro.setSetting(Settings.CHAT_HISTORY_MODE, 'session').catch(() => {});
      ctx.showAlert('History: per-session (takes effect next session)', 'success', 5000);
    },
  },
  {
    value: 'history:global',
    label: 'global',
    description: 'All sessions share one prompt history',
    handle: async ({ ctx }) => {
      await ctx.kiro.setSetting(Settings.CHAT_HISTORY_MODE, 'global').catch(() => {});
      ctx.showAlert('History: global (takes effect next session)', 'success', 5000);
    },
  },
] as const;

/** Lookup helper: find a subcommand by its value. */
export function findSettingsSubcommand(
  value: string
): SettingsSubcommand | undefined {
  return settingsSubcommands.find((s) => s.value === value);
}

/**
 * Pick an alert duration based on message length. Short results (install
 * confirmations) get the default 5s; long results that include tmux
 * instructions, Apple Terminal multi-line output, or fallback hints get
 * 10s so users can actually read + copy the config.
 */
function alertDurationFor(message: string): number {
  return message.length > 180 ? 10000 : 5000;
}

/**
 * Build the `activeCommand` shape for the /settings top-level menu.
 * Shared by showSettingsMenu (first open) and reopenSettingsMenu (Esc-back).
 */
export function buildSettingsActiveCommand(settingsCommand: AvailableCommand): {
  command: AvailableCommand;
  options: Array<{ value: string; label: string; description: string }>;
} {
  return {
    command: {
      ...settingsCommand,
      meta: {
        ...settingsCommand.meta,
        inputType: 'selection' as const,
        searchable: false,
      },
    },
    options: settingsSubcommands
      .filter((s) => !s.value.includes(':'))
      .map((s) => ({
        value: s.value,
        label: s.label,
        description: s.description,
      })),
  };
}

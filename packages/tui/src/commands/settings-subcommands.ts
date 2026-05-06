/**
 * Registry of /settings subcommands.
 *
 * Each entry declares its menu presentation and its `handle` function.
 * Subcommands can be either TUI-local (delegate to an effect like
 * showThemeMenu) or backend (invoke via ctx.kiro.executeCommand). The
 * /settings command doesn't care — the handle closure encapsulates it.
 *
 * To add a new settings subcommand, add a single entry to `settingsSubcommands`
 * below. No changes are required in effects.ts.
 *
 * Example backend subcommand entry (for when its backend command lands):
 *
 *   {
 *     value: 'terminal',
 *     label: 'terminal',
 *     description: 'Enable Shift+Enter / Option+Enter for newlines',
 *     handle: async ({ ctx }) => {
 *       const result = await ctx.kiro.executeCommand({
 *         command: 'terminal-setup',
 *       } as TuiCommand);
 *       if (result?.message) {
 *         ctx.showAlert(result.message, result.success ? 'success' : 'error', 5000);
 *       }
 *     },
 *   },
 */

import type { CommandContext } from './types.js';
import type { SlashCommand } from '../stores/app-store.js';
import type { EffectHandler } from './effects.js';

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
  settingsCommand: SlashCommand;
  /**
   * Look up an effect handler by name. Throws if the effect does not exist,
   * which makes misspellings a build/run error rather than a silent no-op.
   */
  resolveEffect: (name: string) => EffectHandler;
}

export const settingsSubcommands: readonly SettingsSubcommand[] = [
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
    value: 'keybindings',
    label: 'keybindings',
    description: 'Customize keyboard shortcuts',
    handle: ({ ctx }) => {
      ctx.setSettingsReturnOnEscape(true);
      ctx.setShowKeybindingsPanel(true);
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
 * Build the `activeCommand` shape for the /settings top-level menu.
 * Shared by showSettingsMenu (first open) and reopenSettingsMenu (Esc-back).
 */
export function buildSettingsActiveCommand(settingsCommand: SlashCommand): {
  command: SlashCommand;
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
    options: settingsSubcommands.map((s) => ({
      value: s.value,
      label: s.label,
      description: s.description,
    })),
  };
}

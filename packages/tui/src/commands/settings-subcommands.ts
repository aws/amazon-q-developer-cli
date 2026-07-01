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
import {
  Settings,
  DISPLAY_SETTINGS_DESCRIPTION,
} from '../constants/settings.js';
import {
  InterruptMode,
  DEFAULT_INTERRUPT_MODE,
} from '../constants/interrupt-mode.js';
import {
  readStringSetting,
  readCliSettings,
  writeCliSettings,
} from '../utils/cli-settings.js';
import { getActiveGlyphs } from '../hooks/useGlyphs.js';

export interface SettingsSubcommand {
  /** Machine value passed as `/settings <value>` */
  value: string;
  /** Menu label */
  label: string;
  /** Menu description */
  description: string;
  /** Dispatch logic for this subcommand */
  handle: (ctx: SettingsHandleContext) => void | Promise<void>;
  /**
   * Hide this entry from the /settings menu when the UI is not in lite
   * mode. The entry stays in the registry — typing the full
   * `/settings <value>` form still routes through `findSettingsSubcommand`
   * — only the menu listing is affected. Used by lite-specific entries
   * (e.g. verbosity) so the menu doesn't surface options that produce a
   * lite-only error alert when selected from TUI mode.
   */
  liteOnly?: boolean;
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
  /**
   * Trailing argument after the subcommand name, e.g. the `truncation` in
   * `/settings verbosity truncation`. Empty string when the user typed only
   * the subcommand. Handlers that own a nested menu (currently verbosity)
   * forward this to their effect so a typed section name drills straight in;
   * handlers that don't take a sub-arg ignore it.
   */
  arg?: string;
}

export const settingsSubcommands: readonly SettingsSubcommand[] = [
  {
    value: 'display',
    label: 'display',
    description: DISPLAY_SETTINGS_DESCRIPTION,
    handle: ({ ctx }) => {
      ctx.setSettingsReturnOnEscape(true);
      ctx.setShowDisplaySettingsPanel(true);
    },
  },
  {
    value: 'verbosity',
    label: 'verbosity',
    description:
      'Tool args, reasoning, output filters, density (lite mode only)',
    liteOnly: true,
    handle: ({ ctx, settingsCommand, resolveEffect, arg }) => {
      ctx.setSettingsReturnOnEscape(true);

      // Pass the /settings cmd as the dispatcher hint — the verbosityConfig
      // handler resolves the canonical /verbosity SlashCommand from the
      // registry internally, so the menu chip says /verbosity (matching
      // direct entry) regardless of which value we hand it here. The shape
      // we pass is only used as a fallback if /verbosity isn't registered
      // (test-only). Mirrors the /settings theme delegation pattern below.
      // A trailing section name (e.g. `/settings verbosity truncation`) is
      // forwarded so the user drills straight into that sub-menu; empty args
      // opens the smart entry menu — same as bare `/verbosity`. ESC inside a
      // verbosity submenu is governed by `verboseReturnOnEscape`; once the
      // user ESCs out of the top-level verbosity menu, the
      // `settingsReturnOnEscape` flag set above re-opens /settings.
      resolveEffect('verbosityConfig')(null, ctx, settingsCommand, arg ?? '');
    },
  },
  {
    value: 'theme',
    label: 'theme',
    description: 'Colors, prompt style, diff styling',
    handle: ({ ctx }) => {
      ctx.setSettingsReturnOnEscape(true);
      ctx.setShowThemePanel(true);
    },
  },
  {
    value: 'terminal',
    label: 'terminal',
    description: 'Newlines, interrupt behaviour',
    handle: ({ ctx, settingsCommand }) => {
      ctx.setSettingsReturnOnEscape(true);
      ctx.setActiveCommand({
        command: {
          ...settingsCommand,
          meta: {
            ...settingsCommand.meta,
            inputType: 'selection' as const,
            searchable: false,
          },
        },
        options: [
          {
            value: 'terminal:newlines',
            label: 'newlines',
            description: 'Shift+Enter / Option+Enter for newlines',
          },
          {
            value: 'terminal:interrupt',
            label: 'interrupt behaviour',
            description: 'What happens when you type while Kiro is working',
          },
        ],
      });
    },
  },
  {
    value: 'terminal:newlines',
    label: 'newlines',
    description: 'Shift+Enter / Option+Enter for newlines',
    handle: async ({ ctx }) => {
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
    value: 'terminal:interrupt',
    label: 'interrupt behaviour',
    description: 'What happens when you type while Kiro is working',
    handle: ({ ctx, settingsCommand }) => {
      const current = readStringSetting(
        Settings.CHAT_DEFAULT_INTERRUPT_BEHAVIOR,
        DEFAULT_INTERRUPT_MODE
      );
      const glyphs = getActiveGlyphs();
      ctx.setSettingsReturnOnEscape(true);
      ctx.setActiveCommand({
        command: {
          ...settingsCommand,
          meta: {
            ...settingsCommand.meta,
            inputType: 'selection' as const,
            searchable: false,
          },
        },
        options: [
          {
            value: 'terminal:interrupt:steer',
            label: `steer${current === InterruptMode.STEER ? ` ${glyphs.dotFilled}` : ''}`,
            description: 'Inject your message mid-turn at tool boundaries',
          },
          {
            value: 'terminal:interrupt:queue',
            label: `queue${current === InterruptMode.QUEUE ? ` ${glyphs.dotFilled}` : ''}`,
            description: 'Buffer your message and send after turn ends',
          },
        ],
      });
    },
  },
  {
    value: 'terminal:interrupt:steer',
    label: 'steer',
    description: 'Inject your message mid-turn at tool boundaries',
    handle: ({ ctx }) => {
      const settings = readCliSettings();
      settings[Settings.CHAT_DEFAULT_INTERRUPT_BEHAVIOR] = InterruptMode.STEER;
      writeCliSettings(settings);
      ctx.showAlert(
        'Interrupt behaviour: steer (takes effect next session)',
        'success',
        5000
      );
    },
  },
  {
    value: 'terminal:interrupt:queue',
    label: 'queue',
    description: 'Buffer your message and send after turn ends',
    handle: ({ ctx }) => {
      const settings = readCliSettings();
      settings[Settings.CHAT_DEFAULT_INTERRUPT_BEHAVIOR] = InterruptMode.QUEUE;
      writeCliSettings(settings);
      ctx.showAlert(
        'Interrupt behaviour: queue (takes effect next session)',
        'success',
        5000
      );
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
      const glyphs = getActiveGlyphs();
      ctx.setSettingsReturnOnEscape(true);
      ctx.setActiveCommand({
        command: {
          ...settingsCommand,
          meta: {
            ...settingsCommand.meta,
            inputType: 'selection' as const,
            searchable: false,
          },
        },
        options: [
          {
            value: 'history:session',
            label: `session${current === 'session' ? ` ${glyphs.dotFilled}` : ''}`,
            description: 'Each session has its own prompt history',
          },
          {
            value: 'history:global',
            label: `global${current === 'global' ? ` ${glyphs.dotFilled}` : ''}`,
            description: 'All sessions share one prompt history',
          },
        ],
      });
    },
  },
  {
    value: 'history:session',
    label: 'session',
    description: 'Each session has its own prompt history',
    handle: async ({ ctx }) => {
      const settings = readCliSettings();
      settings[Settings.CHAT_HISTORY_MODE] = 'session';
      writeCliSettings(settings);
      await ctx.kiro
        .setSetting(Settings.CHAT_HISTORY_MODE, 'session')
        .catch(() => {});
      ctx.showAlert(
        'History: per-session (takes effect next session)',
        'success',
        5000
      );
    },
  },
  {
    value: 'history:global',
    label: 'global',
    description: 'All sessions share one prompt history',
    handle: async ({ ctx }) => {
      const settings = readCliSettings();
      settings[Settings.CHAT_HISTORY_MODE] = 'global';
      writeCliSettings(settings);
      await ctx.kiro
        .setSetting(Settings.CHAT_HISTORY_MODE, 'global')
        .catch(() => {});
      ctx.showAlert(
        'History: global (takes effect next session)',
        'success',
        5000
      );
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

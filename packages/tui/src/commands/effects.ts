/**
 * Effect registry for slash commands.
 *
 * Two tables:
 * 1. commandEffects: maps command name → effect name
 * 2. effectHandlers: maps effect name → handler function
 *
 * Command names derived from TuiCommand type (typeshare generated).
 */

import type { CommandContext } from './types.js';
import type { CommandResult, TuiCommand } from '../types/commands.js';

/** Effect handler function */
type EffectHandler = (result: CommandResult | null, ctx: CommandContext) => void;

/** Extract command name from TuiCommand union type */
type CommandName = TuiCommand['command'];

/** Effect names - semantic actions the TUI can perform */
type EffectName = 'updateModel' | 'updateAgent' | 'showContextPanel' | 'clearMessages' | 'exit';

/**
 * Command → Effect mapping.
 */
const commandEffects: Partial<Record<CommandName, EffectName>> = {
  model: 'updateModel',
  agent: 'updateAgent',
  context: 'showContextPanel',
  clear: 'clearMessages',
  exit: 'exit',
};

/**
 * Effect handlers.
 */
const effectHandlers: Record<EffectName, EffectHandler> = {
  updateModel: (result, ctx) => {
    const data = result?.data as { model?: { id: string; name: string } } | undefined;
    if (data?.model) {
      ctx.setCurrentModel(data.model);
    }
  },

  updateAgent: (result, ctx) => {
    const data = result?.data as { agent?: { name: string } } | undefined;
    if (data?.agent) {
      ctx.setCurrentAgent(data.agent);
    }
  },

  showContextPanel: (_result, ctx) => {
    ctx.setShowContextBreakdown(true);
  },

  clearMessages: (_result, ctx) => {
    ctx.clearMessages();
  },

  exit: () => {
    process.exit(0);
  },
};

/**
 * Run effect for a command.
 */
export function runEffect(commandName: string, result: CommandResult | null, ctx: CommandContext): void {
  const effectName = commandEffects[commandName as CommandName];
  if (effectName) {
    effectHandlers[effectName]?.(result, ctx);
  }
}

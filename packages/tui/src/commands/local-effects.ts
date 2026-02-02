/**
 * Registry for command local side-effects.
 * Backend handles execution, this handles TUI-side effects after success.
 */

export enum LocalCommand {
  Exit = 'exit',
  Clear = 'clear',
}

export type LocalEffectContext = {
  clearMessages: () => void;
  showAlert: (message: string) => void;
};

type LocalEffect = (ctx: LocalEffectContext) => void;

const effects: Record<LocalCommand, LocalEffect> = {
  [LocalCommand.Exit]: () => process.exit(0),
  [LocalCommand.Clear]: (ctx) => {
    // Only show alert - backend clears conversation history, UI keeps displaying messages
    ctx.showAlert('Conversation history cleared');
  },
};

/** Execute local side-effect for a command if registered */
export function executeLocalEffect(command: string, ctx: LocalEffectContext): void {
  const effect = effects[command as LocalCommand];
  effect?.(ctx);
}

import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';

/**
 * KAS-mode handler for `/help`.
 *
 * Builds the help listing client-side from the merged KAS + local command
 * registries. The V2 path calls a backend ext method (`_kiro/help`) which
 * KAS does not implement, so this handler avoids the round-trip entirely.
 */
export async function handleHelp(
  _cmd: KasCommand,
  _args: string,
  ctx: CommandContext,
  _options?: DispatchOptions
): Promise<void> {
  // liteOnly commands bind lite-only rendering hooks, so hide them outside
  // lite — matching the V2 showHelpPanel effect and the autocomplete menu.
  const inLite = ctx.getUiMode?.() === 'lite';
  const commands = [
    ...ctx.kasCommands.map((c) => ({
      name: c.name,
      description: c.description ?? '',
      usage: c.name,
      subcommands: c.meta?.subcommands,
    })),
    ...ctx.slashCommands
      .filter((c) => 'source' in c && c.source === 'local')
      .filter((c) => inLite || c.meta?.liteOnly !== true)
      .map((c) => ({
        name: c.name,
        description: c.description,
        usage: c.name,
      })),
  ].sort((a, b) => a.name.localeCompare(b.name));

  ctx.setShowHelpPanel(true, commands);
}

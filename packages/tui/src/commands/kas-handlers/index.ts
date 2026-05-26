import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';
import { KasCommandName } from '../../kas-commands';
import { handleChat } from './chat';
import { handleCompact } from './compact';
import { handleHooks } from './hooks';
import { handlePrompts } from './prompts';

export type KasHandler = (
  cmd: KasCommand,
  args: string,
  ctx: CommandContext,
  options?: DispatchOptions
) => Promise<void>;

/**
 * Map of KAS-side command handlers. Adding a new handler narrows the
 * `Partial<...>` further; removing the `Partial<>` would force every
 * `KasCommandName` member to be implemented.
 *
 * TODO: as more KAS commands gain client-side handlers (i.e. stop going
 * through `KasAcpClient.executeCommand`), drop the `Partial<>` so the
 * compiler enforces total coverage.
 */
export const kasHandlers: Partial<Record<KasCommandName, KasHandler>> = {
  [KasCommandName.Chat]: handleChat,
  [KasCommandName.Compact]: handleCompact,
  [KasCommandName.Hooks]: handleHooks,
  [KasCommandName.Prompts]: handlePrompts,
};

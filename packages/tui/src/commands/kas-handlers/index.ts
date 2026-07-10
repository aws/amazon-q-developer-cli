import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';
import { KasCommandName } from '../../kas-commands';
import { handleChat } from './chat';
import { handleCompact } from './compact';
import { handleContext } from './context';
import { handleHelp } from './help';
import { handleHooks } from './hooks';
import { handlePrompts } from './prompts';
import { handleRewind } from './rewind';
import { handleTools } from './tools';
import { handleUpgradeAgent } from './upgrade-agent';

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
  [KasCommandName.Context]: handleContext,
  [KasCommandName.Help]: handleHelp,
  [KasCommandName.Hooks]: handleHooks,
  [KasCommandName.Prompts]: handlePrompts,
  [KasCommandName.Rewind]: handleRewind,
  [KasCommandName.Tools]: handleTools,
  [KasCommandName.UpgradeAgent]: handleUpgradeAgent,
};

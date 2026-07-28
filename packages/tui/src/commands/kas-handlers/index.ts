import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';
import { KasCommandName } from '../../kas-commands';
import { handleChat } from './chat';
import { handleDisconnect } from './disconnect';
import { handleCompact } from './compact';
import { handleContext } from './context';
import { handleHelp } from './help';
import { handleHooks } from './hooks';
import { handlePrompts } from './prompts';
import { handleRewind } from './rewind';
import { handleTangent } from './tangent';
import { handleTools } from './tools';
import { handleUpgradeAgent } from './upgrade-agent';
import { handleModel } from './model';
import { handleAgent } from './agent';
import { handleAutonomous } from './autonomous';
import { handleEffort } from './effort';
import { handleRepo } from './repo';
import { handleMcp } from './mcp';

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
  [KasCommandName.Sessions]: handleChat,
  [KasCommandName.Disconnect]: handleDisconnect,
  [KasCommandName.Compact]: handleCompact,
  [KasCommandName.Context]: handleContext,
  [KasCommandName.Help]: handleHelp,
  [KasCommandName.Hooks]: handleHooks,
  [KasCommandName.Mcp]: handleMcp,
  [KasCommandName.Prompts]: handlePrompts,
  [KasCommandName.Rewind]: handleRewind,
  [KasCommandName.Tangent]: handleTangent,
  [KasCommandName.Tools]: handleTools,
  [KasCommandName.UpgradeAgent]: handleUpgradeAgent,
  [KasCommandName.Model]: handleModel,
  [KasCommandName.Agent]: handleAgent,
  [KasCommandName.Autonomous]: handleAutonomous,
  [KasCommandName.Effort]: handleEffort,
  [KasCommandName.Repo]: handleRepo,
};

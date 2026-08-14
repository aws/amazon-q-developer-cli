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
import { handleMemories } from './memories';
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
import { handleSessions } from './sessions';
import { handleMcp } from './mcp';
import { handleWorkflow } from './workflow';
import type { KasHandlerCommandName } from '../command-registry.js';

export type KasHandler = (
  cmd: KasCommand,
  args: string,
  ctx: CommandContext,
  options?: DispatchOptions
) => Promise<void>;

export const kasHandlers = {
  [KasCommandName.Chat]: handleChat,
  [KasCommandName.Sessions]: handleSessions,
  [KasCommandName.Disconnect]: handleDisconnect,
  [KasCommandName.Compact]: handleCompact,
  [KasCommandName.Context]: handleContext,
  [KasCommandName.Help]: handleHelp,
  [KasCommandName.Hooks]: handleHooks,
  [KasCommandName.Mcp]: handleMcp,
  [KasCommandName.Memories]: handleMemories,
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
  [KasCommandName.Workflow]: handleWorkflow,
} satisfies Record<KasHandlerCommandName, KasHandler>;

import type { AvailableCommand, CommandMeta } from '../types/commands.js';
import type { BackendPanelId } from '../components/layout/shared/BackendPanels.js';
import { KasCommandName } from '../kas-commands.js';
import type { AgentEngine } from '../agent-engine.js';
import { Feature, features } from '../features.js';
import { CONFIG_SUBCOMMANDS } from '../components/ui/config-panel-model.js';

export type CommandEffectName =
  | 'updateModel'
  | 'updateEffort'
  | 'updateAgent'
  | 'showContextPanel'
  | 'showHelpPanel'
  | 'showUsagePanel'
  | 'showMcpPanel'
  | 'showToolsPanel'
  | 'showHooksPanel'
  | 'showKnowledgePanel'
  | 'executePrompt'
  | 'clearMessages'
  | 'quit'
  | 'pasteImage'
  | 'promptEditor'
  | 'loadSession'
  | 'replyEditor'
  | 'showCodePanel'
  | 'showFeedbackUrl'
  | 'spawnSession'
  | 'runSpec'
  | 'switchSession'
  | 'copyToClipboard'
  | 'openRawView'
  | 'showThemeMenu'
  | 'showGoalPanel'
  | 'showSettingsMenu'
  | 'showConfigMenu'
  | 'switchToTui'
  | 'showChangelogPanel'
  | 'showSessionId'
  | 'showStatsPanel'
  | 'switchToGuideAgent'
  | 'switchToLite'
  | 'switchToPlanMode'
  | 'verbosityConfig'
  | 'rewindAction'
  | 'updateTitle'
  | 'openSessionDashboard';

interface LocalCommandDefinition {
  description: string;
  inputType?: CommandMeta['inputType'];
  rollout?: 'hide-unless-lite-enabled' | 'lite-only-unless-enabled';
  hiddenInKas?: boolean;
  /** Only registered on the KAS (V3) engine — for surfaces whose data is
   *  KAS-fed (config-origin descriptors, powers/steering pushes). */
  kasOnly?: boolean;
  liteOnly?: boolean;
  tuiOnly?: boolean;
  /**
   * Rollout feature that must be enabled for the command to register at
   * all (dark-ship: not registered → neither autocompletes nor dispatches).
   */
  feature?: Feature;
  /** Subcommand values surfaced in the Tab-completion dropdown. */
  subcommands?: readonly string[];
}

interface NonPanelCommandRegistration {
  effect?: CommandEffectName;
  panelState?: never;
  kasOnly?: never;
  local?: LocalCommandDefinition;
}

interface EffectPanelCommandRegistration {
  effect: CommandEffectName;
  panelState: BackendPanelId;
  kasOnly?: never;
  local?: LocalCommandDefinition;
}

interface KasPanelCommandRegistration {
  effect?: never;
  panelState: BackendPanelId;
  kasOnly: true;
  local?: LocalCommandDefinition;
}

type CommandRegistration =
  | NonPanelCommandRegistration
  | EffectPanelCommandRegistration
  | KasPanelCommandRegistration;

export const KAS_COMMAND_OWNERS = {
  [KasCommandName.Help]: 'handler',
  [KasCommandName.Agent]: 'handler',
  [KasCommandName.Chat]: 'handler',
  [KasCommandName.Sessions]: 'handler',
  [KasCommandName.Disconnect]: 'handler',
  [KasCommandName.Clear]: 'backend',
  [KasCommandName.Model]: 'handler',
  [KasCommandName.Effort]: 'handler',
  [KasCommandName.Reply]: 'backend',
  [KasCommandName.Paste]: 'backend',
  [KasCommandName.Prompts]: 'handler',
  [KasCommandName.Usage]: 'backend',
  [KasCommandName.Spec]: 'backend',
  [KasCommandName.Knowledge]: 'backend',
  [KasCommandName.Compact]: 'handler',
  [KasCommandName.Context]: 'handler',
  [KasCommandName.Code]: 'backend',
  [KasCommandName.Hooks]: 'handler',
  [KasCommandName.Mcp]: 'handler',
  [KasCommandName.Tools]: 'handler',
  [KasCommandName.Plan]: 'backend',
  [KasCommandName.Autonomous]: 'handler',
  [KasCommandName.Feedback]: 'backend',
  [KasCommandName.Rewind]: 'handler',
  [KasCommandName.Voice]: 'backend',
  [KasCommandName.UpgradeAgent]: 'handler',
  [KasCommandName.Repo]: 'handler',
  [KasCommandName.Tangent]: 'handler',
  [KasCommandName.Goal]: 'backend',
  [KasCommandName.Workflow]: 'handler',
  [KasCommandName.Workflows]: 'backend',
  [KasCommandName.WorkflowRun]: 'backend',
  [KasCommandName.WorkflowResume]: 'backend',
  [KasCommandName.WorkflowStatus]: 'backend',
  [KasCommandName.WorkflowCancel]: 'backend',
  [KasCommandName.Memories]: 'handler',
} as const satisfies Record<KasCommandName, 'handler' | 'backend'>;

export type KasHandlerCommandName = {
  [Name in KasCommandName]: (typeof KAS_COMMAND_OWNERS)[Name] extends 'handler'
    ? Name
    : never;
}[KasCommandName];

export function isKasHandlerCommandName(
  name: KasCommandName
): name is KasHandlerCommandName {
  return KAS_COMMAND_OWNERS[name] === 'handler';
}

export const COMMAND_REGISTRY = {
  feedback: { effect: 'showFeedbackUrl' },
  help: { effect: 'showHelpPanel', panelState: 'showHelpPanel' },
  model: { effect: 'updateModel' },
  effort: { effect: 'updateEffort' },
  agent: { effect: 'updateAgent' },
  plan: { effect: 'switchToPlanMode' },
  context: {
    effect: 'showContextPanel',
    panelState: 'showContextBreakdown',
  },
  usage: { effect: 'showUsagePanel', panelState: 'showUsagePanel' },
  prompts: { effect: 'executePrompt' },
  clear: { effect: 'clearMessages' },
  mcp: { effect: 'showMcpPanel', panelState: 'showMcpPanel' },
  tools: { effect: 'showToolsPanel', panelState: 'showToolsPanel' },
  stats: { effect: 'showStatsPanel', panelState: 'showStatsPanel' },
  hooks: { effect: 'showHooksPanel', panelState: 'showHooksPanel' },
  knowledge: {
    effect: 'showKnowledgePanel',
    panelState: 'showKnowledgePanel',
  },
  paste: { effect: 'pasteImage' },
  reply: { effect: 'replyEditor' },
  code: { effect: 'showCodePanel', panelState: 'showCodePanel' },
  spec: { effect: 'runSpec' },
  guide: { effect: 'switchToGuideAgent' },
  goal: { effect: 'showGoalPanel', panelState: 'showGoalPanel' },
  rewind: { effect: 'rewindAction', panelState: 'showRewindExplorer' },
  sessions: { effect: 'openSessionDashboard' },
  repo: { panelState: 'showRepoPicker', kasOnly: true },
  tangent: { panelState: 'showTangentExplorer', kasOnly: true },
  memories: { panelState: 'showMemoriesPanel', kasOnly: true },
  workflow: { panelState: 'workflowHistory', kasOnly: true },
  editor: {
    effect: 'promptEditor',
    local: { description: 'Open $EDITOR to compose a prompt' },
  },
  spawn: {
    effect: 'spawnSession',
    local: { description: 'Spawn a new agent session with a task' },
  },
  switch: {
    effect: 'switchSession',
    local: { description: 'Switch to a spawned agent session' },
  },
  copy: {
    effect: 'copyToClipboard',
    local: {
      description:
        'Copy last response to clipboard (use /transcript for full conversation)',
    },
  },
  transcript: {
    effect: 'openRawView',
    local: {
      description: 'Open conversation transcript in $PAGER (quit with q)',
    },
  },
  quit: {
    effect: 'quit',
    local: { description: 'Quit the application' },
  },
  exit: {
    effect: 'quit',
    local: { description: 'Quit the application' },
  },
  settings: {
    effect: 'showSettingsMenu',
    local: {
      description:
        'Configure theme, terminal, keybindings, and other preferences',
    },
  },
  config: {
    // Consolidated config category view (agents/MCP/steering/skills/…).
    // Dark-shipped behind the cloud_config rollout: the feature gate drops
    // the registration entirely off-cohort, so it neither autocompletes nor
    // dispatches for regular users. Subcommands power the Tab dropdown,
    // mirroring /settings-style typed-subcommand routing
    // (config-subcommands.ts via showConfigMenu). KAS-only: the panel's
    // data (config-origin descriptors, powers/steering/hooks pushes) is all
    // KAS-fed — V2 has no KAS, so /config doesn't register there.
    effect: 'showConfigMenu',
    local: {
      description:
        'View configured agents, MCP servers, powers, steering, skills, and hooks',
      feature: Feature.CloudConfig,
      kasOnly: true,
      subcommands: CONFIG_SUBCOMMANDS,
    },
  },
  theme: {
    effect: 'showThemeMenu',
    local: {
      description:
        '(moved to /settings theme) Select a theme that looks best for your terminal',
      hiddenInKas: true,
    },
  },
  lite: {
    effect: 'switchToLite',
    local: {
      description: '[EXPERIMENTAL] Switch to Lite UI',
      rollout: 'hide-unless-lite-enabled',
      tuiOnly: true,
    },
  },
  tui: {
    effect: 'switchToTui',
    local: { description: 'Switch to TUI mode', liteOnly: true },
  },
  verbosity: {
    effect: 'verbosityConfig',
    local: {
      description:
        'Configure rendering: tool args, reasoning, output filters, density, subagent sections.',
      rollout: 'lite-only-unless-enabled',
      hiddenInKas: true,
    },
  },
  changelog: {
    effect: 'showChangelogPanel',
    panelState: 'showChangelogPanel',
    local: {
      description: 'Show recent release notes',
      inputType: 'panel',
    },
  },
  'session-id': {
    effect: 'showSessionId',
    local: { description: 'Print the current session ID' },
  },
  title: {
    effect: 'updateTitle',
    local: { description: 'Set, clear, or show the terminal window title' },
  },
} as const satisfies Record<string, CommandRegistration>;

type RegisteredCommandName = keyof typeof COMMAND_REGISTRY;

interface LocalSlashCommand extends AvailableCommand {
  source: 'local';
  meta: CommandMeta & { local: true };
}

function registryName(commandName: string): RegisteredCommandName | undefined {
  const normalized = commandName.replace(/^\//, '');
  return normalized in COMMAND_REGISTRY
    ? (normalized as RegisteredCommandName)
    : undefined;
}

export function getCommandEffect(
  commandName: string
): CommandEffectName | undefined {
  const name = registryName(commandName);
  if (!name) return undefined;
  const registration = COMMAND_REGISTRY[name];
  return 'effect' in registration ? registration.effect : undefined;
}

export function hasCommandEffect(commandName: string): boolean {
  return getCommandEffect(commandName) !== undefined;
}

export function getCommandPanelState(
  commandName: string
): BackendPanelId | undefined {
  const name = registryName(commandName);
  if (!name) return undefined;
  const registration = COMMAND_REGISTRY[name];
  return 'panelState' in registration ? registration.panelState : undefined;
}

export function getLocalSlashCommands(
  liteRolloutEnabled = process.env.KIRO_LITE_ROLLOUT_ENABLED === '1',
  agentEngine?: AgentEngine
): LocalSlashCommand[] {
  return Object.entries(COMMAND_REGISTRY).flatMap(
    ([name, registration]): LocalSlashCommand[] => {
      if (!('local' in registration) || !registration.local) return [];
      const local: LocalCommandDefinition = registration.local;
      if (local.rollout === 'hide-unless-lite-enabled' && !liteRolloutEnabled) {
        return [];
      }
      // Feature-gated commands are dark outside their rollout cohort.
      if (local.feature && !features.isEnabled(local.feature)) {
        return [];
      }
      // KAS-only commands don't register on V2 at all.
      if (local.kasOnly && agentEngine !== 'kas') {
        return [];
      }

      const meta: LocalSlashCommand['meta'] = { local: true };
      if ('inputType' in local && local.inputType) {
        meta.inputType = local.inputType;
      }
      if (local.subcommands) {
        meta.subcommands = [...local.subcommands];
      }
      if (local.rollout === 'lite-only-unless-enabled' && !liteRolloutEnabled) {
        meta.liteOnly = true;
      }
      if (local.hiddenInKas && agentEngine === 'kas') {
        meta.hidden = true;
      }
      if (local.liteOnly) {
        meta.liteOnly = true;
      }
      if (local.tuiOnly) {
        meta.tuiOnly = true;
      }

      return [
        {
          name: `/${name}`,
          description: local.description,
          source: 'local',
          meta,
        },
      ];
    }
  );
}

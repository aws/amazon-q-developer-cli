// Note that this is only for KAS.
// This is needed because KAS is a harness to be used by multiple clients.
// Instead of exposing higher abstraction level extension methods,
// it exposes more basic primitives that are needed by every client.
// It is then up to the client to compose these primitives to fulfill
// their own needs.
import type { AvailableCommand, CommandMeta } from './types/commands';
import { Feature, features } from './features';

export enum KasCommandName {
  Help = '/help',
  Agent = '/agent',
  Chat = '/chat',
  Sessions = '/sessions',
  Disconnect = '/disconnect',
  Clear = '/clear',
  Model = '/model',
  Effort = '/effort',
  Reply = '/reply',
  Paste = '/paste',
  Prompts = '/prompts',
  Usage = '/usage',
  Spec = '/spec',
  Knowledge = '/knowledge',
  Compact = '/compact',
  Context = '/context',
  Code = '/code',
  Hooks = '/hooks',
  Mcp = '/mcp',
  Tools = '/tools',
  Plan = '/plan',
  Autonomous = '/autonomous',
  Feedback = '/feedback',
  Rewind = '/rewind',
  Voice = '/voice',
  UpgradeAgent = '/upgrade-agent',
  Repo = '/repo',
  Tangent = '/tangent',
  Goal = '/goal',
  Workflow = '/workflow',
  Workflows = '/workflows',
  WorkflowRun = '/workflow-run',
  WorkflowResume = '/workflow-resume',
  WorkflowStatus = '/workflow-status',
  WorkflowCancel = '/workflow-cancel',
  Memories = '/memories',
}

export type KasWorkflowAliasSubcommand =
  | ''
  | 'run'
  | 'resume'
  | 'status'
  | 'cancel';

export function getKasWorkflowAliasSubcommand(
  name: string
): KasWorkflowAliasSubcommand | undefined {
  const normalized = (name.startsWith('/') ? name : `/${name}`).toLowerCase();
  switch (normalized) {
    case KasCommandName.Workflows:
      return '';
    case KasCommandName.WorkflowRun:
      return 'run';
    case KasCommandName.WorkflowResume:
      return 'resume';
    case KasCommandName.WorkflowStatus:
      return 'status';
    case KasCommandName.WorkflowCancel:
      return 'cancel';
    default:
      return undefined;
  }
}

const KAS_COMMAND_NAME_VALUES: ReadonlySet<string> = new Set(
  Object.values(KasCommandName)
);

export function isKasCommandName(name: string): name is KasCommandName {
  return KAS_COMMAND_NAME_VALUES.has(name);
}

export function isKasCommand(cmd: AvailableCommand): cmd is KasCommand {
  return isKasCommandName(cmd.name);
}

/**
 * TODO: KasCommand currently mirrors `AvailableCommand` shape with a
 * narrowed `name`. As KAS-side handlers grow we'll likely diverge with
 * KAS-specific fields (typed subcommand schemas, input validators, etc.)
 * and stop sharing the V2 backend command shape entirely.
 */
export interface KasCommand extends AvailableCommand {
  name: KasCommandName;
  meta?: CommandMeta;
  feature?: Feature;
}

/** TUI-owned slash commands for KAS mode. */
export const KAS_COMMANDS: readonly KasCommand[] = [
  {
    name: KasCommandName.Help,
    description: 'Show available commands',
    meta: { inputType: 'panel' },
  },
  {
    name: KasCommandName.Agent,
    description: 'List or switch agents',
    meta: {
      inputType: 'selection',
      hint: '',
      subcommands: ['create', 'edit', 'swap'],
      subcommandDescriptions: {
        create: 'Create a new agent',
        edit: 'Edit an agent config in $EDITOR',
        swap: 'Switch to a different agent',
      },
      subcommandHints: {
        create: '<name> [--from <agent>] [--directory <path>]',
        edit: '[name]',
        swap: '<name>',
      },
    },
  },
  {
    name: KasCommandName.Chat,
    description: 'Load a previous session, save, or start a new one',
    meta: {
      inputType: 'selection',
      local: true,
      subcommands: ['new', 'save', 'load'],
      subcommandDescriptions: {
        new: 'Start a fresh session',
        save: 'Save the conversation to a file',
        load: 'Load a conversation from a file',
      },
      subcommandHints: {
        new: '[prompt]',
        save: '[--force] <path>',
        load: '<path>',
      },
    },
  },
  {
    // Cloud-gated alias of /chat: same subcommands and selection view.
    name: KasCommandName.Sessions,
    description: 'Load a previous session, save, or start a new one',
    meta: {
      inputType: 'selection',
      local: true,
      cloudOnly: true,
      subcommands: ['new', 'save', 'load'],
      subcommandDescriptions: {
        new: 'Start a fresh session',
        save: 'Save the conversation to a file',
        load: 'Load a conversation from a file',
      },
      subcommandHints: {
        new: '[prompt]',
        save: '[--force] <path>',
        load: '<path>',
      },
    },
  },
  {
    name: KasCommandName.Clear,
    description: 'Clear the conversation and start a fresh session',
  },
  {
    name: KasCommandName.Model,
    description: 'List or switch models',
    meta: {
      inputType: 'selection',
      hint: '',
      subcommands: ['set-current-as-default'],
      subcommandDescriptions: {
        'set-current-as-default': 'Save the active model as the default',
      },
    },
  },
  {
    name: KasCommandName.Effort,
    description: 'List or set the reasoning effort level',
    meta: {
      inputType: 'selection',
      hint: '',
      subcommands: ['set-current-as-default'],
      subcommandDescriptions: {
        'set-current-as-default': 'Save the active effort level as the default',
      },
    },
  },
  {
    name: KasCommandName.Reply,
    description: 'Reply to the last assistant message in $EDITOR',
  },
  {
    name: KasCommandName.Paste,
    description: 'Paste image from clipboard',
  },
  {
    name: KasCommandName.Prompts,
    description: 'Select or list available prompts',
    meta: {
      inputType: 'selection',
      hint: '',
    },
  },
  {
    name: KasCommandName.Usage,
    description: 'Show plan usage and billing information',
    meta: { inputType: 'panel' },
  },
  {
    name: KasCommandName.Spec,
    description: 'List specs, switch to spec mode, or run spec tasks',
    meta: {
      local: true,
      subcommands: ['new', 'run', 'view', 'analyze_requirements'],
      subcommandDescriptions: {
        new: 'Create a new spec',
        run: 'Execute tasks from a spec',
        view: 'View a spec document',
        analyze_requirements: 'Analyze requirements coverage for a spec',
      },
      subcommandHints: {
        new: '<feature-name>',
        run: '<feature-name>',
        view: '<feature-name> [requirements|design|tasks]',
        analyze_requirements: '↵ to select a spec',
      },
    },
  },
  {
    name: KasCommandName.Knowledge,
    description: 'Manage knowledge bases',
    meta: { inputType: 'panel' },
  },
  {
    name: KasCommandName.Compact,
    description: 'Compact conversation history to reduce context usage',
  },
  {
    name: KasCommandName.Context,
    description: 'Show or manage context files',
    meta: {
      inputType: 'panel',
      hint: 'add <path>, remove <path>, clear',
      subcommands: ['show', 'add', 'remove', 'clear'],
      subcommandDescriptions: {
        show: 'Show context files and usage',
        add: 'Add files to context',
        remove: 'Remove files from context',
        clear: 'Remove all files from context',
      },
      subcommandHints: { add: '[--force] <path>...', remove: '<path>...' },
    },
  },
  {
    name: KasCommandName.Code,
    description:
      'Code intelligence status, initialization, and codebase overview',
    meta: {
      inputType: 'panel',
      subcommands: ['status', 'init', 'overview'],
      subcommandDescriptions: {
        status: 'Show code intelligence status',
        init: 'Initialize code intelligence for the workspace',
        overview: 'Generate a codebase overview',
      },
    },
  },
  {
    name: KasCommandName.Hooks,
    description: 'View configured hooks',
    meta: { inputType: 'panel' },
  },
  {
    name: KasCommandName.Mcp,
    description: 'Show MCP server status',
    meta: { inputType: 'panel' },
  },
  {
    name: KasCommandName.Tools,
    description: 'List available tools',
    meta: { inputType: 'panel' },
  },
  {
    name: KasCommandName.Plan,
    description:
      'Switch to plan mode to break ideas into an implementation plan',
  },
  {
    // Dark-shipped: feature-gated behind the cloud-sandbox rollout AND
    // cloud-only, so no released user sees it and, even in the cohort, it is
    // offered only inside cloud sessions.
    name: KasCommandName.Autonomous,
    description: 'Turn autonomous mode on or off',
    feature: Feature.RemoteSandbox,
    meta: {
      cloudOnly: true,
      subcommands: ['on', 'off'],
      subcommandDescriptions: {
        on: 'Enable autonomous mode',
        off: 'Disable autonomous mode',
      },
    },
  },
  {
    name: KasCommandName.Feedback,
    description: 'Submit feedback, request features, or report issues',
    meta: {
      inputType: 'selection',
      searchable: false,
      hint: '',
    },
  },
  {
    name: KasCommandName.Rewind,
    description: 'Fork the session at an earlier turn',
    meta: { inputType: 'panel' },
  },
  {
    // Rollout-gated via the FeatureManager singleton (Feature.Voice, resolved
    // from rollout.json → KIRO_ENABLED_FEATURES). The dispatcher owns the
    // capture flow once invoked.
    name: KasCommandName.Voice,
    description: 'Record voice input',
    feature: Feature.Voice,
  },
  {
    name: KasCommandName.Goal,
    description: 'Work toward a goal in a loop until done',
    feature: Feature.Workflows,
    meta: {
      inputType: 'panel',
      local: true,
      hint: '<description> [--max N]',
    },
  },
  {
    name: KasCommandName.Workflow,
    description: 'Browse and manage workflows or run a recipe',
    feature: Feature.Workflows,
    meta: {
      inputType: 'panel',
      local: true,
      hint: '[run <recipe> | list]',
      subcommands: ['run', 'list'],
      subcommandDescriptions: {
        run: 'Run a workflow recipe',
        list: 'List available recipes',
      },
      subcommandHints: {
        run: '<recipe> [inputs]',
        list: '',
      },
    },
  },
  {
    name: KasCommandName.Workflows,
    description: 'Browse workflow history',
    feature: Feature.Workflows,
    meta: { local: true, hidden: true },
  },
  {
    name: KasCommandName.WorkflowRun,
    description: 'Run a workflow',
    feature: Feature.Workflows,
    meta: { local: true, hidden: true },
  },
  {
    name: KasCommandName.WorkflowResume,
    description: 'Resume a paused workflow',
    feature: Feature.Workflows,
    meta: { local: true, hidden: true },
  },
  {
    name: KasCommandName.WorkflowStatus,
    description: 'Check workflow status',
    feature: Feature.Workflows,
    meta: { local: true, hidden: true },
  },
  {
    name: KasCommandName.WorkflowCancel,
    description: 'Cancel a workflow',
    feature: Feature.Workflows,
    meta: { local: true, hidden: true },
  },
  {
    name: KasCommandName.UpgradeAgent,
    description: 'Upgrade V2 agent configs to universal (V2 + V3) format',
    meta: {
      inputType: 'selection',
      hint: '',
      subcommands: ['run', 'diagnostics'],
      subcommandDescriptions: {
        run: 'Upgrade agents by group',
        diagnostics: 'Review upgraded agents',
      },
      subcommandHints: {
        run: '(upgrade agents by group)',
        diagnostics: '(review upgraded agents)',
      },
    },
  },
  {
    name: KasCommandName.Repo,
    description:
      'Attach a repository to the cloud session (or /repo <owner/name> to attach directly)',
    meta: { inputType: 'panel', hint: '[owner/name]', cloudOnly: true },
  },
  {
    name: KasCommandName.Disconnect,
    description: 'Disconnect from the cloud session (it keeps running)',
    meta: { local: true, cloudOnly: true },
  },
  {
    name: KasCommandName.Tangent,
    description: 'Go back, switch to, or create a conversation tangent',
    feature: Feature.Tangent,
    meta: {
      inputType: 'panel',
      hint: '<name> | ls',
      subcommands: ['ls'],
      subcommandDescriptions: { ls: 'List tangents' },
      subcommandsOptional: true,
    },
  },
  {
    name: KasCommandName.Memories,
    description: 'Manage repo-scoped memories from previous sessions',
    feature: Feature.Memory,
    meta: { inputType: 'panel' },
  },
];

const KAS_WORKFLOW_COMMAND_NAMES: ReadonlySet<string> = new Set(
  KAS_COMMANDS.filter((command) => command.feature === Feature.Workflows).map(
    (command) => command.name
  )
);

export function isKasWorkflowCommandName(name: string): boolean {
  const normalized = name.startsWith('/') ? name : `/${name}`;
  return KAS_WORKFLOW_COMMAND_NAMES.has(normalized);
}

export function filterByEnabledFeatures(
  commands: readonly KasCommand[]
): readonly KasCommand[] {
  return commands.filter((c) => !c.feature || features.isEnabled(c.feature));
}

/** The KAS command set for this launch, with rollout-gated commands resolved. */
export function getKasCommands(): readonly KasCommand[] {
  return filterByEnabledFeatures(KAS_COMMANDS);
}

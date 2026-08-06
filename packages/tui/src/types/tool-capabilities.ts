import {
  TOOL_LABELS,
  getToolLabel,
  type BuiltinToolId,
} from './tool-status.js';

export type ToolKind = string;
export type ToolCallOrigin = 'builtin' | 'mcp';

export const NON_SCROLLBACK_TOOL_IDS = {
  report: true,
  thinking: true,
  todo: true,
  aws: true,
  subagent: true,
} as const satisfies Partial<Record<BuiltinToolId, true>>;

type NonScrollbackToolId = keyof typeof NON_SCROLLBACK_TOOL_IDS;

export type ScrollbackToolRenderer =
  | Exclude<BuiltinToolId, NonScrollbackToolId>
  | 'goal'
  | 'session'
  | 'workflow'
  | 'generic';

export type ToolApprovalPresentation = 'diff' | 'subagent' | 'arguments';
export type ToolApprovalDetail = 'shell-command' | 'delete-path' | 'generic';
export type ToolDiffPolicy = 'unified' | 'none';

export const TOOL_RENDERER_APPROVAL = {
  write: 'diff',
  read: 'arguments',
  shell: 'arguments',
  web_search: 'arguments',
  web_fetch: 'arguments',
  grep: 'arguments',
  glob: 'arguments',
  ls: 'arguments',
  code: 'arguments',
  session: 'subagent',
  introspect: 'arguments',
  image_read: 'arguments',
  goal: 'arguments',
  task: 'arguments',
  knowledge: 'arguments',
  workflow: 'arguments',
  generic: 'arguments',
} as const satisfies Record<ScrollbackToolRenderer, ToolApprovalPresentation>;

interface ToolCapability {
  names: readonly string[];
  kinds?: readonly ToolKind[];
  /** Named aliases that retain the legacy kind-first renderer precedence. */
  kindFirstNames?: readonly string[];
  builtinId?: BuiltinToolId;
  renderer: ScrollbackToolRenderer;
  diff: ToolDiffPolicy;
  diffByName?: Readonly<Record<string, ToolDiffPolicy>>;
  trivial?: boolean;
  artifactWriteNames?: readonly string[];
  parentNames?: readonly string[];
}

const LEGACY_ARTIFACT_WRITE_NAMES = [
  'Write',
  'create',
  'Edit',
  'fs_edit',
] as const;

const WORKFLOW_LAUNCH_NAMES = ['run_workflow', 'Run Workflow'] as const;

export const TOOL_CAPABILITIES = {
  write: {
    names: [
      'fs_write',
      'write',
      'str_replace',
      'edit',
      'create_file',
      'write_file',
      'fs_append',
      'delete_file',
      'Write File',
      'Replace in File',
      'Append to File',
      'Delete File',
    ],
    kinds: ['edit'],
    builtinId: 'write',
    renderer: 'write',
    diff: 'unified',
    diffByName: {
      fs_write: 'unified',
      write: 'unified',
      str_replace: 'unified',
      edit: 'unified',
      create_file: 'unified',
      write_file: 'unified',
      fs_append: 'none',
      delete_file: 'none',
      'Write File': 'unified',
      'Replace in File': 'unified',
      'Append to File': 'none',
      'Delete File': 'none',
    },
    artifactWriteNames: ['fs_write'],
  },
  read: {
    names: [
      'fs_read',
      'read',
      'read_file',
      'read_files',
      'list_directory',
      'Read File',
      'Read Files',
      'List Directory',
    ],
    kinds: ['read'],
    builtinId: 'read',
    renderer: 'read',
    diff: 'none',
    trivial: true,
  },
  shell: {
    names: [
      'execute_bash',
      'execute_cmd',
      'shell',
      'control_bash_process',
      'control_pwsh_process',
      'run_command',
      'Run Command',
      'Control Process',
    ],
    kinds: ['execute'],
    builtinId: 'shell',
    renderer: 'shell',
    diff: 'none',
  },
  shellProcess: {
    names: [
      'list_processes',
      'List Processes',
      'get_process_output',
      'Get Process Output',
    ],
    renderer: 'generic',
    diff: 'none',
  },
  webSearch: {
    names: ['web_search', 'Searching the web'],
    builtinId: 'web_search',
    renderer: 'web_search',
    diff: 'none',
  },
  webFetch: {
    names: ['web_fetch', 'Fetching web content', 'Fetch URL'],
    builtinId: 'web_fetch',
    renderer: 'web_fetch',
    diff: 'none',
  },
  grep: {
    names: ['grep', 'grep_search', 'Grep Search'],
    kinds: ['search'],
    builtinId: 'grep',
    renderer: 'grep',
    diff: 'none',
    trivial: true,
  },
  glob: {
    names: ['glob', 'file_search', 'File Search'],
    builtinId: 'glob',
    renderer: 'glob',
    diff: 'none',
    trivial: true,
  },
  ls: {
    names: ['ls'],
    builtinId: 'ls',
    renderer: 'ls',
    diff: 'none',
  },
  code: {
    names: ['code', 'Code Intelligence'],
    // The engine tags generated codebase overviews as reads. The legacy
    // dispatcher handled that kind before the code-name branch.
    kindFirstNames: ['Code Intelligence'],
    builtinId: 'code',
    renderer: 'code',
    diff: 'none',
    trivial: true,
  },
  imageRead: {
    names: ['image_read', 'imageRead'],
    builtinId: 'image_read',
    renderer: 'image_read',
    diff: 'none',
  },
  session: {
    names: [
      'session_management',
      'subagent',
      'agent_crew',
      'orchestrate_subagent',
      'invoke_sub_agent',
      'subagent_response',
      'Invoke Agent',
      'Subagent Response',
    ],
    renderer: 'session',
    diff: 'none',
    parentNames: [
      'subagent',
      'orchestrate_subagent',
      'invoke_sub_agent',
      'agent_crew',
    ],
  },
  introspect: {
    names: ['introspect', 'Introspect'],
    builtinId: 'introspect',
    renderer: 'introspect',
    diff: 'none',
    trivial: true,
  },
  knowledge: {
    names: ['knowledge', 'Knowledge Search'],
    builtinId: 'knowledge',
    renderer: 'knowledge',
    diff: 'none',
  },
  task: {
    names: ['task', 'todo_list', 'todo', 'Task List'],
    builtinId: 'task',
    renderer: 'task',
    diff: 'none',
  },
  goal: {
    names: ['goal'],
    renderer: 'goal',
    diff: 'none',
  },
  workflow: {
    names: [...WORKFLOW_LAUNCH_NAMES, 'inspect_workflow', 'Inspect Workflow'],
    renderer: 'workflow',
    diff: 'none',
  },
} as const satisfies Record<string, ToolCapability>;

export type ToolCapabilityId = keyof typeof TOOL_CAPABILITIES;

type RegisteredBuiltinToolId = {
  [Id in ToolCapabilityId]: (typeof TOOL_CAPABILITIES)[Id] extends {
    readonly builtinId: infer BuiltinId extends BuiltinToolId;
  }
    ? BuiltinId
    : never;
}[ToolCapabilityId];

export type MissingBuiltinToolCapability = Exclude<
  BuiltinToolId,
  NonScrollbackToolId | RegisteredBuiltinToolId
>;

/**
 * Adding a BuiltinToolId must classify it as either non-scrollback or as a
 * canonical tool capability. An empty object stops compiling while any ID is
 * missing, before either UI can silently route the new built-in to generic.
 */
export const BUILTIN_TOOL_CAPABILITY_COVERAGE = {} as const satisfies Record<
  MissingBuiltinToolCapability,
  true
>;

const capabilityEntries = Object.entries(TOOL_CAPABILITIES) as Array<
  [ToolCapabilityId, ToolCapability]
>;

const capabilitiesByName = new Map<string, ToolCapability>();
const capabilitiesByKind = new Map<ToolKind, ToolCapability>();
const artifactWriteNames = new Set<string>();
const knownToolNames = new Set<string>();
const deleteApprovalNames = new Set([
  'delete_file',
  'delete',
  'fs_delete',
  'remove_file',
]);

for (const [id, capability] of capabilityEntries) {
  for (const name of capability.names) {
    if (capabilitiesByName.has(name)) {
      throw new Error(`Duplicate tool name "${name}" in capability "${id}"`);
    }
    capabilitiesByName.set(name, capability);
    knownToolNames.add(name);
  }
  for (const kind of capability.kinds ?? []) {
    if (capabilitiesByKind.has(kind)) {
      throw new Error(`Duplicate tool kind "${kind}" in capability "${id}"`);
    }
    capabilitiesByKind.set(kind, capability);
  }
  for (const name of capability.artifactWriteNames ?? []) {
    artifactWriteNames.add(name);
  }
}
for (const name of deleteApprovalNames) knownToolNames.add(name);
knownToolNames.add('bash');
for (const name of LEGACY_ARTIFACT_WRITE_NAMES) {
  artifactWriteNames.add(name);
  knownToolNames.add(name);
}

export function resolveToolCapability(
  name: string,
  kind?: ToolKind,
  origin?: ToolCallOrigin
): ToolCapability | undefined {
  if (origin === 'mcp' || name.startsWith('mcp__')) return undefined;
  const named = capabilitiesByName.get(name);
  const byKind = kind !== undefined ? capabilitiesByKind.get(kind) : undefined;
  if (named?.kindFirstNames?.includes(name) && byKind) return byKind;
  return named ?? byKind;
}

export function resolveScrollbackToolRenderer(
  name: string,
  kind?: ToolKind,
  origin?: ToolCallOrigin
): ScrollbackToolRenderer {
  return resolveToolCapability(name, kind, origin)?.renderer ?? 'generic';
}

export function resolveToolId(
  name: string,
  kind?: ToolKind,
  origin?: ToolCallOrigin
): BuiltinToolId | undefined {
  return resolveToolCapability(name, kind, origin)?.builtinId;
}

export function resolveToolDisplayName(
  name: string,
  kind?: ToolKind,
  origin?: ToolCallOrigin
): string {
  if (origin === 'mcp' || name.startsWith('mcp__')) return name;
  const capability = resolveToolCapability(name, kind, origin);
  const direct = Object.prototype.hasOwnProperty.call(TOOL_LABELS, name)
    ? (name as BuiltinToolId)
    : undefined;
  const builtinId = capability?.builtinId ?? direct;
  if (builtinId) return getToolLabel(builtinId);
  return name;
}

export function toolApprovalPresentation(
  name: string,
  kind?: ToolKind,
  origin?: ToolCallOrigin
): ToolApprovalPresentation {
  const capability = resolveToolCapability(name, kind, origin);
  const presentation =
    TOOL_RENDERER_APPROVAL[capability?.renderer ?? 'generic'];
  if (presentation === 'subagent' && !capability?.parentNames?.includes(name)) {
    return 'arguments';
  }
  if (
    presentation === 'diff' &&
    toolDiffPolicy(name, kind, origin) === 'none'
  ) {
    return 'arguments';
  }
  return presentation;
}

export function toolApprovalDetail(
  name: string,
  kind?: ToolKind,
  origin?: ToolCallOrigin
): ToolApprovalDetail {
  if (origin === 'mcp' || name.startsWith('mcp__')) return 'generic';
  if (deleteApprovalNames.has(name)) return 'delete-path';
  if (
    name === 'bash' ||
    resolveScrollbackToolRenderer(name, kind, origin) === 'shell'
  ) {
    return 'shell-command';
  }
  return 'generic';
}

export function toolDiffPolicy(
  name: string,
  kind?: ToolKind,
  origin?: ToolCallOrigin
): ToolDiffPolicy {
  const capability = resolveToolCapability(name, kind, origin);
  if (!capability || capability.diff === 'none') return 'none';
  const namePolicy = capability.diffByName?.[name];
  if (namePolicy) return namePolicy;
  if (kind && capability.kinds?.includes(kind)) return capability.diff;
  return 'none';
}

export function isTrivialTool(
  name: string,
  kind?: ToolKind,
  origin?: ToolCallOrigin
): boolean {
  return resolveToolCapability(name, kind, origin)?.trivial === true;
}

export function isArtifactWriteTool(name: string): boolean {
  return artifactWriteNames.has(name);
}

export const KNOWN_TOOL_NAMES: ReadonlySet<string> = knownToolNames;

const namesFor = (id: ToolCapabilityId): ReadonlySet<string> =>
  new Set<string>(TOOL_CAPABILITIES[id].names);

export const WRITE_TOOL_NAMES = namesFor('write');
export const READ_TOOL_NAMES = namesFor('read');
export const SHELL_TOOL_NAMES = namesFor('shell');
export const SHELL_PROCESS_TOOL_NAMES = namesFor('shellProcess');
export const WEB_SEARCH_TOOL_NAMES = namesFor('webSearch');
export const WEB_FETCH_TOOL_NAMES = namesFor('webFetch');
export const GREP_TOOL_NAMES = namesFor('grep');
export const GLOB_TOOL_NAMES = namesFor('glob');
export const LS_TOOL_NAMES = namesFor('ls');
export const CODE_TOOL_NAMES = namesFor('code');
export const IMAGE_READ_TOOL_NAMES = namesFor('imageRead');
export const SESSION_TOOL_NAMES = namesFor('session');
export const INTROSPECT_TOOL_NAMES = namesFor('introspect');
export const KNOWLEDGE_TOOL_NAMES = namesFor('knowledge');
export const TASK_TOOL_NAMES = namesFor('task');
export const WORKFLOW_TOOL_NAMES = namesFor('workflow');
export const WORKFLOW_LAUNCH_TOOL_NAMES: ReadonlySet<string> = new Set(
  WORKFLOW_LAUNCH_NAMES
);

export const isWorkflowLaunchTool = (name?: string): boolean =>
  name !== undefined && WORKFLOW_LAUNCH_TOOL_NAMES.has(name);

export const PARENT_SUBAGENT_TOOL_NAMES: ReadonlySet<string> = new Set(
  TOOL_CAPABILITIES.session.parentNames
);

export const isParentSubagentTool = (
  name?: string | null,
  origin?: ToolCallOrigin
): boolean =>
  !!name && origin !== 'mcp' && PARENT_SUBAGENT_TOOL_NAMES.has(name);

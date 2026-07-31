import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import {
  parseAgentSubcommand,
  type ParsedAgentCommand,
} from '../../acp-client';
import { getAgentDisplayName } from '../../utils/agentColors';
import { extractRpcErrorMessage } from '../../utils/error-handling';
import { openFileInEditor } from '../../utils/editor.js';
import { defaultAgentDirs } from '../../utils/agent-migration/io.js';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from 'fs';
import { homedir } from 'os';
import { isAbsolute, join, resolve } from 'path';

/**
 * `/agent` list / switch / create for KAS. The available agents come from the
 * `kasAvailableAgents` store slice (parsed from the `mode` configOption,
 * normalized + filtered to the user-selectable set). Switching goes through
 * `ctx.kiro.setConfigOption('mode', …)`, which re-emits the normalized agent
 * events so the store + chip self-heal. Creation writes a profile file into an
 * agents directory; KAS watches those directories and pushes the updated agent
 * list, so no explicit reload is needed.
 */
export async function handleAgent(
  cmd: KasCommand,
  args: string,
  ctx: CommandContext
): Promise<void> {
  const parsed = parseAgentSubcommand({ value: args });
  switch (parsed.kind) {
    case 'list':
      return showAgentPicker(ctx, cmd);
    case 'swap':
      return switchAgent(ctx, parsed.name);
    case 'create':
      return createAgent(ctx, parsed);
    case 'edit':
      return editAgent(ctx, parsed.name);
  }
}

function showAgentPicker(ctx: CommandContext, cmd: KasCommand): void {
  if (ctx.kasAvailableAgents.length === 0) {
    // Cloud: the sandbox owns the agent surface and pushes it over the
    // downlink shortly after attach — an empty list is a not-yet,
    // not a failure.
    if (ctx.cloudSessionActive) {
      ctx.showAlert(
        'Waiting for the sandbox to report its agents — try again in a moment',
        'warning',
        4000
      );
      return;
    }
    ctx.showAlert('No agents available', 'error', 3000);
    return;
  }
  const currentName = ctx.getCurrentAgent?.()?.name;
  const options = ctx.kasAvailableAgents.map((a) => {
    const isActive = a.id === currentName;
    const descBase = a.description ?? '';
    return {
      value: a.id,
      label: getAgentDisplayName(a.id, a.name),
      description: isActive
        ? `[active]${descBase ? ` ${descBase}` : ''}`
        : descBase,
      // Group by source (e.g. "Bundled", "Workspace") so the menu reflects
      // where each agent came from. Agents without source metadata fall into
      // the default (ungrouped) bucket.
      ...(a.source ? { group: capitalize(a.source) } : {}),
    };
  });
  ctx.setActiveCommand({ command: cmd, options });
}

async function switchAgent(
  ctx: CommandContext,
  agentName: string
): Promise<void> {
  ctx.setLoadingMessage(`Agent changing to ${agentName}`);
  try {
    await ctx.kiro.setConfigOption('mode', agentName);
  } catch (err) {
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to switch agent'),
      'error',
      5000
    );
    return;
  }
  ctx.setLoadingMessage(null);
  if (ctx.getCurrentAgent?.()?.name !== agentName) {
    ctx.showAlert(`Agent '${agentName}' not available`, 'error', 5000);
    return;
  }
  ctx.showAlert(`Switched to ${agentName}`, 'success', 3000);
}

const CREATE_USAGE =
  'Usage: /agent create <name> [--from <agent>] [--directory <path>]';

const PROFILE_EXTENSIONS = ['json', 'md'] as const;

/** Editor launcher seam: injectable so tests can bypass the real spawn. */
export type OpenEditor = (filePath: string) => {
  exitCode: number;
  error?: string;
};

/** Scaffold + editor + validate flow for `/agent create`. Writes the profile
 *  file first so the editor always opens on a saved, discoverable file even
 *  if the user quits without writing.
 *  Exported for unit testing; production callers go through handleAgent. */
export async function createAgent(
  ctx: CommandContext,
  args: Extract<ParsedAgentCommand, { kind: 'create' }>,
  openEditor: OpenEditor = openFileInEditor
): Promise<void> {
  if (ctx.cloudSessionActive) {
    // Cloud: the sandbox owns the agent surface; a locally written profile
    // would never be discovered by the remote agent.
    ctx.showAlert(
      '/agent create is not available in cloud sessions',
      'error',
      5000
    );
    return;
  }

  const name = args.name;
  if (!name) {
    ctx.showAlert(`Agent name is required. ${CREATE_USAGE}`, 'error', 5000);
    return;
  }

  let dir: string;
  try {
    dir = resolveAgentsDirectory(args.directory);
  } catch (err) {
    ctx.showAlert(
      `Failed to create agent: ${err instanceof Error ? err.message : String(err)}`,
      'error',
      5000
    );
    return;
  }

  for (const ext of PROFILE_EXTENSIONS) {
    const candidate = join(dir, `${name}.${ext}`);
    if (existsSync(candidate)) {
      ctx.showAlert(
        `File already exists at ${candidate}. Aborting`,
        'error',
        5000
      );
      return;
    }
  }

  let content: string;
  let extension: (typeof PROFILE_EXTENSIONS)[number] = 'json';
  if (args.from) {
    const sourcePath = findAgentProfile(args.from);
    if (!sourcePath) {
      ctx.showAlert(`No agent with name '${args.from}' found`, 'error', 5000);
      return;
    }
    try {
      const raw = readFileSync(sourcePath, 'utf-8');
      if (sourcePath.endsWith('.md')) {
        extension = 'md';
        content = renameMdProfile(raw, name);
      } else {
        const parsed: unknown = JSON.parse(raw);
        if (
          typeof parsed !== 'object' ||
          parsed === null ||
          Array.isArray(parsed)
        ) {
          ctx.showAlert(
            `Agent config at ${sourcePath} is not a JSON object`,
            'error',
            5000
          );
          return;
        }
        content = JSON.stringify({ ...parsed, name }, null, 2) + '\n';
      }
    } catch (err) {
      ctx.showAlert(
        `Failed to read agent '${args.from}': ${err instanceof Error ? err.message : String(err)}`,
        'error',
        5000
      );
      return;
    }
  } else {
    content =
      JSON.stringify(
        { name, description: '', prompt: '', tools: [] },
        null,
        2
      ) + '\n';
  }

  const filePath = join(dir, `${name}.${extension}`);
  try {
    mkdirSync(dir, { recursive: true });
    writeFileSync(filePath, content);
  } catch (err) {
    ctx.showAlert(
      `Failed to write agent config: ${err instanceof Error ? err.message : String(err)}`,
      'error',
      5000
    );
    return;
  }

  const { exitCode, error } = openEditor(filePath);
  if (exitCode !== 0) {
    ctx.showAlert(
      error ?? `Editor exited with code ${exitCode}`,
      'error',
      3000
    );
    return;
  }

  const validationError =
    extension === 'json' ? validateJsonProfile(filePath) : undefined;
  if (validationError) {
    ctx.showAlert(validationError, 'error', 5000);
    return;
  }

  // KAS watches the agents directories and pushes the refreshed agent list;
  // profiles written outside those directories won't be discovered.
  ctx.showAlert(`Agent '${name}' created at ${filePath}`, 'success', 5000);
}

/** Locate + editor + validate flow for `/agent edit`. The name defaults to
 *  the active agent; built-in agents have no profile file and are rejected.
 *  Exported for unit testing; production callers go through handleAgent. */
export async function editAgent(
  ctx: CommandContext,
  requestedName: string | undefined,
  openEditor: OpenEditor = openFileInEditor
): Promise<void> {
  if (ctx.cloudSessionActive) {
    // Cloud: the sandbox owns the agent surface; local profile files are not
    // what the remote agent is running.
    ctx.showAlert(
      '/agent edit is not available in cloud sessions',
      'error',
      5000
    );
    return;
  }

  const name = requestedName ?? ctx.getCurrentAgent?.()?.name;
  if (!name) {
    ctx.showAlert(
      'No agent selected. Usage: /agent edit [name]',
      'error',
      5000
    );
    return;
  }

  const entry = ctx.kasAvailableAgents.find((a) => a.id === name);
  if (entry?.source === 'bundled') {
    ctx.showAlert(
      `Cannot edit built-in agent '${name}'. Create a new agent with '/agent create'`,
      'error',
      5000
    );
    return;
  }

  const filePath = findAgentProfile(name);
  if (!filePath) {
    ctx.showAlert(
      entry
        ? `Agent '${name}' has no config file on disk`
        : `Agent '${name}' not found`,
      'error',
      5000
    );
    return;
  }

  const { exitCode, error } = openEditor(filePath);
  if (exitCode !== 0) {
    ctx.showAlert(
      error ?? `Editor exited with code ${exitCode}`,
      'error',
      3000
    );
    return;
  }

  const validationError = filePath.endsWith('.json')
    ? validateJsonProfile(filePath)
    : undefined;
  if (validationError) {
    ctx.showAlert(validationError, 'error', 5000);
    return;
  }

  ctx.showAlert(`Edited agent '${name}' at ${filePath}`, 'success', 5000);
}

/** Special directory keywords accepted by `--directory`. */
const DIR_GLOBAL = 'global';
const DIR_WORKSPACE = 'workspace';

/** Resolve `--directory` to an agents directory. Defaults to the user-global
 *  agents directory; `workspace` targets `<cwd>/.kiro/agents`; anything else
 *  is a path (with `~` expansion, relative to cwd). */
function resolveAgentsDirectory(directory: string | undefined): string {
  const [workspaceDir, globalDir] = defaultAgentDirs();
  if (directory === undefined || directory === DIR_GLOBAL) return globalDir!;
  if (directory === DIR_WORKSPACE) return workspaceDir!;

  let path = directory;
  if (path === '~' || path.startsWith('~/')) {
    const home = process.env.HOME || process.env.USERPROFILE || homedir();
    path = path === '~' ? home : join(home, path.slice(2));
  }
  if (!isAbsolute(path)) {
    path = resolve(process.cwd(), path);
  }
  if (existsSync(path) && !statSync(path).isDirectory()) {
    throw new Error('Path must be a directory');
  }
  return path;
}

/** Locate an existing agent profile by name: workspace dir first (it shadows
 *  global at load time), `.json` before `.md`. */
function findAgentProfile(name: string): string | undefined {
  for (const dir of defaultAgentDirs()) {
    for (const ext of PROFILE_EXTENSIONS) {
      const candidate = join(dir, `${name}.${ext}`);
      if (existsSync(candidate)) return candidate;
    }
  }
  return undefined;
}

/** A copied markdown profile keeps its filename-derived id unless the front
 *  matter pins an explicit `name:` — rewrite that pin to the new name. */
function renameMdProfile(content: string, newName: string): string {
  const frontMatter = content.match(/^---\r?\n([\s\S]*?)\r?\n---/);
  if (!frontMatter) return content;
  const updated = frontMatter[1]!.replace(/^name:.*$/m, `name: ${newName}`);
  return content.replace(frontMatter[1]!, updated);
}

/** Post-editor validation for `.json` profiles: parseable JSON object, and an
 *  explicit `name` (if present) must be a non-empty string. Returns an error
 *  message, or undefined when valid. */
function validateJsonProfile(filePath: string): string | undefined {
  try {
    const parsed: unknown = JSON.parse(readFileSync(filePath, 'utf-8'));
    if (
      typeof parsed !== 'object' ||
      parsed === null ||
      Array.isArray(parsed)
    ) {
      return `Malformed agent config at ${filePath}: not a JSON object`;
    }
    const name = (parsed as Record<string, unknown>).name;
    if (name !== undefined && (typeof name !== 'string' || !name.trim())) {
      return `Malformed agent config at ${filePath}: "name" must be a non-empty string`;
    }
    return undefined;
  } catch (e) {
    return e instanceof SyntaxError
      ? `Malformed agent config at ${filePath}: ${e.message}`
      : `Failed to read agent config at ${filePath}: ${e}`;
  }
}

function capitalize(s: string): string {
  return s.length > 0 ? s[0]!.toUpperCase() + s.slice(1) : s;
}

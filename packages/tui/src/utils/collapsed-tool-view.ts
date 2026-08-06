import {
  isParentSubagentTool,
  resolveToolDisplayName,
  type ToolCallOrigin,
  type ToolKind,
} from '../types/tool-capabilities.js';
import { parseToolArg } from './tool-result.js';

export { resolveToolDisplayName } from '../types/tool-capabilities.js';

const PREVIEW_MAX = 120;

/** Title prefix KAS uses for per-stage subagent wrapper cards. */
const SUBAGENT_TITLE_PREFIX = 'Sub-agent:';

/** Title KAS uses for the orchestration card. */
const ORCHESTRATE_SUBAGENT_TITLE = 'Orchestrate Sub-agent';

/**
 * Whether a tool call is a subagent SPAWN card that KAS surfaces only as a
 * TITLE form — the "Sub-agent: <role>" per-stage wrappers and the "Orchestrate
 * Sub-agent" card. Those arrive with kind 'other', bypass SessionTool, and
 * otherwise fall through to a verbose name/prompt/explanation dump, so they
 * must always collapse to a one-line prompt preview (in every mode, not just
 * spec).
 *
 * Deliberately does NOT match the snake_case spawn wire names (`subagent`,
 * `agent_crew`, `orchestrate_subagent`, `invoke_sub_agent`, `Invoke Agent`):
 * those route to SessionTool, which renders clean live labels (agent counts,
 * "Spawned agent", shimmer), and intercepting them here would regress that to
 * a raw collapsed title. The `kind === 'other'` guard both reflects the wire
 * kind of these KAS cards and avoids a false positive on a coincidental
 * MCP/user tool that happens to share the title text.
 */
export function isSubagentCard(
  name: string,
  kind?: ToolKind,
  origin?: ToolCallOrigin
): boolean {
  if (origin === 'mcp') return false;
  if (kind !== undefined && kind !== 'other') return false;
  if (name.startsWith(SUBAGENT_TITLE_PREFIX)) return true;
  if (name === ORCHESTRATE_SUBAGENT_TITLE) return true;
  return false;
}

/**
 * The role substring of a KAS "Sub-agent: <role>" title-form name (trimmed,
 * possibly empty), or null when `name` is not that form. The agent name lives
 * in the title itself for these cards, so we surface it as the collapsed
 * entry's target.
 */
function subagentRoleFromTitleName(name: string): string | null {
  if (!name.startsWith(SUBAGENT_TITLE_PREFIX)) return null;
  return name.slice(SUBAGENT_TITLE_PREFIX.length).trim();
}

/** Arg keys, in priority order, whose first line best summarizes a tool call
 *  when its full args are collapsed (spec mode). */
const PRIMARY_ARG_KEYS = [
  'prompt',
  'command',
  'query',
  'pattern',
  'path',
  'task',
];

/**
 * Whether a tool call should be collapsed in spec mode. Collapse every tool
 * that actually carries args to hide, to a title + one-line preview.
 *
 * Two things are deliberately NOT collapsed:
 *   - Interactive `user_input` questions (a `question` arg with no `prompt`).
 *   - Arg-less tool calls. KAS sends `user_input` with the question in the
 *     TITLE and no `rawInput` at all, so there is nothing to collapse; left
 *     to render normally, the title is markdown-honored (Tool.renderTitle)
 *     instead of being flattened to a plain collapsed line.
 */
export function isCollapsibleTool(content?: string): boolean {
  if (isUserInputQuestion(content)) return false;
  return hasArgs(content);
}

/**
 * Whether `ToolUseContent` should render a tool call as a collapsed one-line
 * preview instead of the full render. Collapses when either spec mode hides
 * all tool args (`hideArgs`) or the call is a subagent spawn card, AND the
 * call actually has args worth collapsing. Extracted as a pure function so the
 * render-gate logic is unit-testable without an Ink render harness.
 */
export function shouldCollapseToolCard(
  name: string,
  kind: ToolKind | undefined,
  content: string | undefined,
  hideArgs: boolean,
  origin?: ToolCallOrigin
): boolean {
  return (
    (hideArgs || isSubagentCard(name, kind, origin)) &&
    isCollapsibleTool(content)
  );
}

/** True when the tool args parse to a non-empty object — i.e. there is
 *  something to collapse. */
function hasArgs(content?: string): boolean {
  if (!content) return false;
  try {
    const obj: unknown = JSON.parse(content);
    return !!obj && typeof obj === 'object' && Object.keys(obj).length > 0;
  } catch {
    return false;
  }
}

/**
 * True when the tool args are a `user_input`-style question (a `question`
 * field and no `prompt`). Delegations/subagents carry a `prompt`, not a
 * question, so they still collapse. Note: KAS currently sends user_input with
 * no args at all (question in the title) — that case is handled by hasArgs.
 */
function isUserInputQuestion(content?: string): boolean {
  const question = parseToolArg(content, 'question');
  if (!question || question.trim().length === 0) return false;
  return !parseToolArg(content, 'prompt');
}

/** First non-empty line of a value, trimmed and truncated for a one-line preview. */
function firstLine(value: string): string {
  const line =
    value
      .split('\n')
      .map((l) => l.trim())
      .find((l) => l.length > 0) ?? '';
  return line.length > PREVIEW_MAX
    ? `${line.slice(0, PREVIEW_MAX - 1)}…`
    : line;
}

/** A collapsed tool-use render: the friendly tool name, an optional target
 *  (e.g. the spawned agent's name), and a one-line preview of the primary arg.
 *  Status comes from the enclosing StatusBar. */
export interface CollapsedToolPreview {
  title: string;
  target?: string;
  preview?: string;
}

/**
 * Build the collapsed (spec-mode) preview for a tool call: title + optional
 * target + the first line of the most relevant argument. For subagent spawns
 * the preview is the spawned agent's prompt and the target is its name, so a
 * collapsed entry reads e.g. "Subagent requirement-detailer / Requirement 2:
 * ...". Pure and unit-testable, independent of ink rendering.
 */
export function collapsedToolPreview(
  name: string,
  kind: ToolKind | undefined,
  content: string,
  origin?: ToolCallOrigin
): CollapsedToolPreview {
  const title = resolveToolDisplayName(name, kind, origin);

  // KAS "Sub-agent: <role>" wrapper cards carry the agent name in the title
  // itself; normalize to a clean "Subagent" title + role target so they read
  // like the snake_case `subagent` form instead of dumping their raw args.
  const role = subagentRoleFromTitleName(name);
  if (role !== null) {
    const raw =
      parseToolArg(content, 'prompt') ?? parseToolArg(content, 'task');
    return {
      title: resolveToolDisplayName('subagent'),
      target: role || undefined,
      preview: raw ? firstLine(raw) : undefined,
    };
  }

  // KAS "Orchestrate Sub-agent" card: keep its title, derive the target from
  // the spawned agent arg when present.
  if (name === ORCHESTRATE_SUBAGENT_TITLE) {
    const target =
      parseToolArg(content, 'agent') ??
      parseToolArg(content, 'subagent_type') ??
      parseToolArg(content, 'name') ??
      undefined;
    const raw =
      parseToolArg(content, 'prompt') ?? parseToolArg(content, 'task');
    return { title, target, preview: raw ? firstLine(raw) : undefined };
  }

  if (isParentSubagentTool(name, origin)) {
    const target =
      parseToolArg(content, 'agent') ??
      parseToolArg(content, 'subagent_type') ??
      parseToolArg(content, 'name') ??
      undefined;
    const raw =
      parseToolArg(content, 'prompt') ?? parseToolArg(content, 'task');
    return { title, target, preview: raw ? firstLine(raw) : undefined };
  }

  let raw: string | null = null;
  for (const key of PRIMARY_ARG_KEYS) {
    raw = parseToolArg(content, key);
    if (raw) break;
  }
  // fs_read carries no top-level path — its target lives in
  // `operations: [{ mode, path|image_paths }]` (or legacy `ops`). Fall back to
  // the first operation's path so a collapsed read shows the file, not "Read".
  raw ??= firstOperationPath(content);
  return { title, preview: raw ? firstLine(raw) : undefined };
}

/**
 * First file path from an fs_read-style `operations`/`ops` array — the op's
 * `path`, or its first `image_paths` entry — or null when the content has no
 * such array. Mirrors the operations lookup in ToolUseMessage's full render.
 */
function firstOperationPath(content: string): string | null {
  try {
    const parsed: unknown = JSON.parse(content);
    if (!parsed || typeof parsed !== 'object') return null;
    const record = parsed as Record<string, unknown>;
    const ops = record.operations ?? record.ops;
    if (!Array.isArray(ops) || ops.length === 0) return null;
    const first = ops[0] as Record<string, unknown> | null;
    if (!first || typeof first !== 'object') return null;
    if (typeof first.path === 'string') return first.path;
    if (
      Array.isArray(first.image_paths) &&
      typeof first.image_paths[0] === 'string'
    ) {
      return first.image_paths[0];
    }
    return null;
  } catch {
    return null;
  }
}

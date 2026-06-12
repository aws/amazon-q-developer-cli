import {
  resolveToolId,
  kindToToolId,
  type ToolKind,
} from '../types/agent-events.js';
import {
  TOOL_LABELS,
  getToolLabel,
  type BuiltinToolId,
} from '../types/tool-status.js';
import { parseToolArg } from './tool-result.js';

const PREVIEW_MAX = 120;

/** Tool names that spawn/orchestrate subagents — their primary preview arg is
 *  the prompt/task given to the spawned agent. */
const SUBAGENT_TOOL_NAMES = new Set(['subagent', 'agent_crew']);

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
 * Resolve the friendly display name for a tool from its wire name, falling
 * back to a name that is itself a builtin id (e.g. "subagent"), then its ACP
 * kind, and finally the raw name — mirroring the routing in ToolUseContent so
 * a collapsed entry shows the same label users see normally.
 */
export function resolveToolDisplayName(name: string, kind?: ToolKind): string {
  const direct = Object.prototype.hasOwnProperty.call(TOOL_LABELS, name)
    ? (name as BuiltinToolId)
    : undefined;
  const toolId = resolveToolId(name) ?? direct ?? kindToToolId(kind);
  return toolId ? getToolLabel(toolId) : name;
}

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
  content: string
): CollapsedToolPreview {
  const title = resolveToolDisplayName(name, kind);

  if (SUBAGENT_TOOL_NAMES.has(name)) {
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

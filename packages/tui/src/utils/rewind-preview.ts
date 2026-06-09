/**
 * Shared preview builder for /rewind — used by both KAS handler and V2 effect handler.
 * Builds enriched timeline previews from the TUI message store.
 */
import { MessageRole } from '../stores/app-store';

const MAX_PREVIEW_LINES = 40;
const MAX_LINE_WIDTH = 160;
const TOOL_PREFIX = '↳ ';

export interface TurnMessage {
  id: string;
  role: string;
  content: string;
  name?: string;
  contextPercent?: number;
}

/**
 * Enrich a turn list with rich previews built from the TUI message store.
 * Falls back to the existing responseSnippet when store data is unavailable.
 */
export function enrichTurnsWithPreview(
  turns: Array<{
    logIndex: number;
    label: string;
    group: string;
    responseSnippet: string;
  }>,
  messages: TurnMessage[]
): typeof turns {
  const turnGroups = groupByTurn(messages);

  return turns.map((turn, i) => {
    const group = turnGroups[i];
    if (!group || group.length <= 1) return turn; // no AI messages
    const userMsg = group[0]!;
    const preview = buildPreview(group.slice(1)); // skip user message
    const contextGroup =
      userMsg.contextPercent != null
        ? `${Math.round(userMsg.contextPercent)}%`
        : turn.group || '--';
    return {
      ...turn,
      group: contextGroup,
      responseSnippet: preview || turn.responseSnippet,
    };
  });
}

/** Group messages into turns (each starting with a User message). Skips leading non-User messages. */
function groupByTurn(messages: TurnMessage[]): TurnMessage[][] {
  const groups: TurnMessage[][] = [];
  let current: TurnMessage[] | null = null;
  for (const msg of messages) {
    if (msg.role === MessageRole.User) {
      if (current) groups.push(current);
      current = [msg];
    } else if (current) {
      current.push(msg);
    }
    // Skip messages before the first User message
  }
  if (current) groups.push(current);
  return groups;
}

/** Build preview for a turn's AI messages. */
export function buildPreview(msgs: TurnMessage[]): string {
  const hasTools = msgs.some((m) => m.role === MessageRole.ToolUse);
  if (!hasTools) {
    const model = msgs.find((m) => m.role === MessageRole.Model);
    if (!model) return '';
    const line = model.content.split('\n').find((l) => l.trim());
    return line ? truncate(line) : '';
  }

  const lines: string[] = [];
  for (const msg of msgs) {
    if (lines.length >= MAX_PREVIEW_LINES) break;
    if (msg.role === MessageRole.Model && msg.content) {
      const line = msg.content.split('\n').find((l) => l.trim());
      if (line) lines.push(truncate(line));
    } else if (msg.role === MessageRole.ToolUse) {
      lines.push(TOOL_PREFIX + toolLabel(msg.name || 'unknown', msg.content));
    }
  }
  return lines.join('\n');
}

function toolLabel(name: string, contentJson: string): string {
  try {
    const args = JSON.parse(contentJson);
    const purpose = args?.__tool_use_purpose?.trim();
    if (purpose)
      return `${name}: ${purpose[0]?.toLowerCase()}${purpose.slice(1)}`;
    const keyArg = extractKeyArg(args);
    if (keyArg) return `${name} ${keyArg}`;
  } catch {
    /* malformed args */
  }
  return name;
}

function extractKeyArg(args: Record<string, unknown>): string {
  const paths = args.paths as string[] | undefined;
  if (Array.isArray(paths) && paths.length > 0) {
    const first = paths[0]!.split('/').pop() || paths[0]!;
    return paths.length > 1 ? `${first} +${paths.length - 1} more` : first;
  }

  const val = (args.command ??
    args.path ??
    args.filePath ??
    args.file_path ??
    args.pattern ??
    args.query ??
    args.url ??
    null) as string | null;
  if (typeof val !== 'string') return '';
  const short =
    val.includes('/') && !val.includes(' ') ? val.split('/').pop() || val : val;
  return truncate(short);
}

function truncate(s: string): string {
  return s.length > MAX_LINE_WIDTH ? s.slice(0, MAX_LINE_WIDTH - 1) + '…' : s;
}

import type { ToolCallEvent } from '../types/agent-events';
import { toolDiffPolicy } from '../types/tool-capabilities.js';

/**
 * Result of synthesizing a ToolUse message's content from a ToolCall event.
 */
export interface SynthesizedToolUse {
  /**
   * The model's `__tool_use_purpose` (the per-tool "why"), captured from the
   * untouched rawInput before synthesis below rebuilds a narrower content.
   * Lite's reasoning slot recovers it; modern TUI's <Tool> excludes the field
   * from its params display anyway, so dropping it from `content` is
   * byte-equivalent for that path.
   */
  purpose: string | undefined;
  /** The JSON content blob stored on the message. */
  content: string;
}

/** Infer the edit command from an edit tool's args. */
function inferEditCommand(args: Record<string, unknown>): string {
  if (args.oldStr !== undefined) return 'strReplace';
  if (args.insertLine !== undefined || args.append) return 'insert';
  return 'create';
}

/**
 * Capture the tool-use purpose and synthesize the message content from a
 * ToolCall event. Edit-shaped tools (V2 `toolContent` diff or `kind === 'edit'`)
 * get a normalized {command, path, content, oldStr, newStr, insertLine} blob
 * that lite's renderer parses; everything else echoes the raw args.
 */
export function synthesizeToolUseContent(
  event: ToolCallEvent
): SynthesizedToolUse {
  const rawArgs = (event.args ?? {}) as Record<string, unknown>;
  const purpose =
    typeof rawArgs.__tool_use_purpose === 'string' &&
    rawArgs.__tool_use_purpose.trim().length > 0
      ? rawArgs.__tool_use_purpose
      : undefined;

  let content: string;
  const toolContentDiff = event.toolContent?.[0];
  const hasUnifiedDiff =
    toolDiffPolicy(event.name, event.kind, event.origin) === 'unified';
  if (toolContentDiff && hasUnifiedDiff) {
    const args = event.args as Record<string, unknown>;
    content = JSON.stringify({
      command: inferEditCommand(args),
      path: toolContentDiff.path,
      content: toolContentDiff.newText,
      oldStr: toolContentDiff.oldText,
      newStr: toolContentDiff.newText,
      insertLine: args.insertLine,
    });
  } else if (event.kind === 'edit' && hasUnifiedDiff) {
    const args = event.args as Record<string, unknown>;
    content = JSON.stringify({
      command: inferEditCommand(args),
      path: args.path,
      content: args.text || args.content || '',
      oldStr: args.oldStr,
      newStr: args.newStr,
      insertLine: args.insertLine,
    });
  } else {
    content = JSON.stringify(event.args);
  }

  return { purpose, content };
}

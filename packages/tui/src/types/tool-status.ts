/**
 * Centralized tool display metadata for all built-in tools.
 *
 * Each entry defines the display label shown in the TUI for a tool.
 *
 * Tool components should import `getToolLabel` to resolve display names
 * instead of hardcoding strings.
 */

/**
 * All built-in tool identifiers.
 * Tools not yet implemented in the TUI are included for forward-compatibility.
 */
export type BuiltinToolId =
  | 'code'
  | 'shell'
  | 'read'
  | 'write'
  | 'glob'
  | 'grep'
  // TODO: Remove 'ls' and 'image_read' once enough time has passed that users are unlikely
  // to load saved conversations containing old ls/imageRead tool calls.
  | 'ls'
  | 'introspect'
  | 'knowledge'
  | 'report'
  | 'thinking'
  | 'todo'
  | 'aws'
  | 'subagent'
  // TODO: Remove 'image_read' (see 'ls' TODO above).
  | 'image_read'
  | 'web_fetch'
  | 'web_search'
  | 'task';

export const TOOL_LABELS: Record<BuiltinToolId, string> = {
  code: 'Code',
  shell: 'Shell',
  read: 'Read',
  write: 'Write',
  glob: 'Glob',
  grep: 'Grep',
  // TODO: Remove ls and image_read labels (see BuiltinToolId TODO above).
  ls: 'Ls',
  image_read: 'ImageRead',
  introspect: 'Introspect',
  knowledge: 'Knowledge',
  report: 'Report',
  thinking: 'Thinking',
  todo: 'TaskList',
  aws: 'AWS',
  subagent: 'Subagent',
  web_fetch: 'WebFetch',
  web_search: 'WebSearch',
  task: 'Task',
};

/**
 * Resolve the display label for a tool given its id.
 * The label is now state-independent (no active/completed distinction).
 */
export function getToolLabel(toolId: BuiltinToolId): string {
  return TOOL_LABELS[toolId];
}

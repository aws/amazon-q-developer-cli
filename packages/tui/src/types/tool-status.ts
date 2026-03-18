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
  | 'ls'
  | 'introspect'
  | 'knowledge'
  | 'report'
  | 'thinking'
  | 'todo'
  | 'aws'
  | 'subagent'
  | 'web_fetch'
  | 'web_search';

export const TOOL_LABELS: Record<BuiltinToolId, string> = {
  code: 'Code',
  shell: 'Shell',
  read: 'Read',
  write: 'Write',
  glob: 'Glob',
  grep: 'Grep',
  ls: 'Ls',
  introspect: 'Introspect',
  knowledge: 'Knowledge',
  report: 'Report',
  thinking: 'Thinking',
  todo: 'TaskList',
  aws: 'AWS',
  subagent: 'Subagent',
  web_fetch: 'WebFetch',
  web_search: 'WebSearch',
};

/**
 * Resolve the display label for a tool given its id.
 * The label is now state-independent (no active/completed distinction).
 */
export function getToolLabel(toolId: BuiltinToolId): string {
  return TOOL_LABELS[toolId];
}

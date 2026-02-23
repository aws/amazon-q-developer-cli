/**
 * Centralized tool display metadata for all built-in tools.
 *
 * Each entry defines the present participle (active) and past tense (completed)
 * labels shown in the TUI when a tool is running or has finished.
 *
 * Tool components should import `getToolStatus` to resolve display names
 * instead of hardcoding strings.
 */

export interface ToolStatusLabels {
  /** Present participle shown while tool is running (e.g. "Searching") */
  active: string;
  /** Past tense shown when tool has finished (e.g. "Searched") */
  completed: string;
}

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

export const TOOL_STATUS: Record<BuiltinToolId, ToolStatusLabels> = {
  code: { active: 'Analyzing', completed: 'Analyzed' },
  shell: { active: 'Running', completed: 'Ran' },
  read: { active: 'Reading', completed: 'Read' },
  write: { active: 'Writing', completed: 'Wrote' },
  glob: { active: 'Finding', completed: 'Found' },
  grep: { active: 'Searching', completed: 'Searched' },
  ls: { active: 'Listing', completed: 'Listed' },
  introspect: { active: 'Introspecting', completed: 'Introspected' },
  knowledge: { active: 'Querying', completed: 'Queried' },
  report: { active: 'Reporting', completed: 'Reported' },
  thinking: { active: 'Thinking', completed: 'Thought' },
  todo: { active: 'Tracking', completed: 'Tracked' },
  aws: { active: 'Calling', completed: 'Called' },
  subagent: { active: 'Delegating', completed: 'Delegated' },
  web_fetch: { active: 'Fetching', completed: 'Fetched' },
  web_search: { active: 'Searching', completed: 'Searched' },
};

/**
 * Resolve the display label for a tool given its id and finished state.
 * Returns the active or completed label, or a fallback for unknown tools.
 */
export function getToolLabel(
  toolId: BuiltinToolId,
  isFinished: boolean
): string {
  const labels = TOOL_STATUS[toolId];
  return isFinished ? labels.completed : labels.active;
}

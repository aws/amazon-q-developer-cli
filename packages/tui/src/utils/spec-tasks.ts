import type { TaskItem } from '../types/tasks.js';
import type { SpecTaskExecutionStatus } from '../types/agent-events.js';

/**
 * tasks.md parsing for the activity tray, matching the agent's own contract.
 *
 * The agent identifies a task by the full text following its checkbox, derives
 * hierarchy from dotted task numbers when they are present, and reports status
 * only for leaf tasks. All three facts are load-bearing: a tray seeded on any
 * other identity or granularity accepts the file but silently matches nothing,
 * showing a run that never advances.
 */

/**
 * Checkbox line. Group 3 is the status mark, group 4 the optional marker, and
 * group 5 the task text that serves as its identity.
 */
const TASK_LINE_RE = /^(\s*)([-*+])\s+\[([ xX\-~])\](\\?\*?)\s+(.+)$/;

/** A leading dotted number, written either "1.2 Title" or "1.2. Title". */
const TASK_NUMBER_RE = /^(\d+(?:\.\d+)*)\.?\s+(.*)$/;

/**
 * The tray's identity for a task the agent reported.
 *
 * The tray renders the id as a short label, so a numbered task is keyed by its
 * number; unnumbered tasks fall back to their text, which keeps them matchable
 * at the cost of a long label.
 */
export function trayTaskId(agentTaskId: string): string {
  const text = agentTaskId.trim();
  return TASK_NUMBER_RE.exec(text)?.[1] ?? text;
}

interface ParsedLine {
  indent: number;
  mark: string;
  isOptional: boolean;
  text: string;
  number: string | null;
  title: string;
}

function parseLine(line: string): ParsedLine | null {
  const m = TASK_LINE_RE.exec(line);
  if (!m) return null;
  const marker = m[4] ?? '';
  const text = (m[5] ?? '').trim();
  const numbered = TASK_NUMBER_RE.exec(text);
  return {
    indent: (m[1] ?? '').replace(/\t/g, '    ').length,
    mark: (m[3] ?? ' ').toLowerCase(),
    isOptional: marker === '*' || marker === '\\*',
    text,
    number: numbered?.[1] ?? null,
    title: (numbered?.[2] ?? text).trim(),
  };
}

function statusFromMark(mark: string): TaskItem['status'] {
  if (mark === 'x') return 'completed';
  if (mark === '-') return 'running';
  return 'pending';
}

/**
 * Whether a task has children, and so is a parent whose status is the sum of
 * theirs rather than something the agent reports.
 *
 * Numbering wins when the document uses it, because "1.1" is a child of "1"
 * however the two lines happen to be indented.
 */
function isParent(
  task: ParsedLine,
  index: number,
  all: ParsedLine[],
  numbered: boolean
): boolean {
  if (numbered) {
    if (!task.number) return false;
    const prefix = `${task.number}.`;
    return all.some((other) => other.number?.startsWith(prefix) === true);
  }
  const next = all[index + 1];
  return next !== undefined && next.indent > task.indent;
}

/**
 * Seed the tray from a tasks.md.
 *
 * Only leaf tasks are returned: counting parents would inflate every total
 * with rows that can never complete. Optional tasks are included only when the
 * run promotes them, matching what will actually execute.
 */
export function parseSpecTasks(
  markdown: string,
  options: { includeOptional: boolean }
): TaskItem[] {
  const parsed: ParsedLine[] = [];
  for (const line of markdown.replace(/\r\n?/g, '\n').split('\n')) {
    const entry = parseLine(line);
    if (entry) parsed.push(entry);
  }
  const numbered = parsed.some((task) => task.number !== null);

  const tasks: TaskItem[] = [];
  for (let i = 0; i < parsed.length; i++) {
    const task = parsed[i]!;
    if (isParent(task, i, parsed, numbered)) continue;
    if (task.isOptional && !options.includeOptional) continue;
    tasks.push({
      id: task.number ?? task.text,
      subject: task.title,
      status: statusFromMark(task.mark),
    });
  }
  return tasks;
}

/**
 * The tray status for a reported execution status.
 *
 * An aborted task returns to not-started, matching the checkbox the agent
 * rewrites; a failure keeps its own state instead, because a run the user has
 * to intervene in is the one thing the tray exists to surface. `yielded`
 * carries no task-level meaning, so the current status stands.
 */
export function taskStatusFromExecution(
  executionStatus: SpecTaskExecutionStatus,
  current: TaskItem['status']
): TaskItem['status'] {
  switch (executionStatus) {
    case 'running':
      return 'running';
    case 'succeed':
      return 'completed';
    case 'failed':
      return 'failed';
    case 'queued':
    case 'aborted':
      return 'pending';
    case 'yielded':
      return current;
  }
}

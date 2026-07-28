import type { Key } from '../../../hooks/useKeypress.js';

export type WorkflowControlShortcut = 'pause' | 'resume';

export function workflowControlShortcut(
  input: string,
  key: Pick<Key, 'ctrl' | 'meta'>
): WorkflowControlShortcut | null {
  if (key.ctrl || key.meta) return null;
  if (input === 'p') return 'pause';
  if (input === 'r') return 'resume';
  return null;
}

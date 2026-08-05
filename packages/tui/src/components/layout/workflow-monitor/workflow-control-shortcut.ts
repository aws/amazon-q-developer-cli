import type { Key } from '../../../hooks/useKeypress.js';
import type { WorkflowStatus } from '../../../types/workflow.js';
import { isRetryableWorkflowStatus } from '../../../types/workflow-status.js';

export type WorkflowControlShortcut = 'pause' | 'resume' | 'retry';

export function workflowControlShortcut(
  input: string,
  key: Pick<Key, 'ctrl' | 'meta'>,
  status: WorkflowStatus | undefined
): WorkflowControlShortcut | null {
  if (key.ctrl || key.meta) return null;
  if (input === 'p' && status === 'running') return 'pause';
  if (input === 'r' && status === 'paused') return 'resume';
  if (input === 'r' && isRetryableWorkflowStatus(status)) return 'retry';
  return null;
}

import type { Key } from '../../../hooks/useKeypress.js';

export type WorkflowStopKeyAction =
  | 'pass'
  | 'arm'
  | 'confirm'
  | 'dismiss'
  | 'block';

export function classifyWorkflowStopKey(
  input: string,
  key: Key,
  armed: boolean,
  canStop: boolean
): WorkflowStopKeyAction {
  if (armed) {
    if (key.escape) return 'dismiss';
    if (key.ctrl && input === 'x') return 'confirm';
    return 'block';
  }
  if (canStop && key.ctrl && input === 'x') return 'arm';
  return 'pass';
}

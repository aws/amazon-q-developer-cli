import type { Key } from '../../../hooks/useKeypress.js';

export interface ActivityTrayInputOwnership {
  rowNavigationActive: boolean;
  tabNavigationActive: boolean;
  workflowNavigationActive: boolean;
}

export type ActivityTrayInputAction =
  | 'previous-workflow'
  | 'next-workflow'
  | 'previous-workflow-node'
  | 'next-workflow-node'
  | 'previous-row'
  | 'next-row'
  | 'next-tab';

export type ActivityTrayQueueInputAction =
  | 'remove-queue-entry'
  | 'edit-queue-entry';

export function resolveActivityTrayInputAction(
  input: string,
  key: Key,
  ownership: ActivityTrayInputOwnership
): ActivityTrayInputAction | null {
  if (ownership.workflowNavigationActive && key.shift) {
    if (key.leftArrow) return 'previous-workflow';
    if (key.rightArrow) return 'next-workflow';
    if (key.upArrow) return 'previous-workflow-node';
    if (key.downArrow) return 'next-workflow-node';
  }

  if (ownership.rowNavigationActive) {
    if ((key.shift || key.meta) && key.upArrow) return 'previous-row';
    if ((key.shift || key.meta) && key.downArrow) return 'next-row';
    if (key.ctrl && input === 'p') return 'previous-row';
    if (key.ctrl && input === 'n') return 'next-row';
  }

  if (ownership.tabNavigationActive && key.tab && !key.shift && !key.ctrl) {
    return 'next-tab';
  }

  return null;
}

export function activityTrayOwnsInput(
  input: string,
  key: Key,
  ownership: ActivityTrayInputOwnership
): boolean {
  return resolveActivityTrayInputAction(input, key, ownership) !== null;
}

export function resolveActivityTrayQueueInputAction(
  key: Key,
  queueInputActive: boolean,
  promptIsEmpty: boolean
): ActivityTrayQueueInputAction | null {
  if (!queueInputActive || !promptIsEmpty) return null;
  if (key.delete || key.backspace) return 'remove-queue-entry';
  if (key.return) return 'edit-queue-entry';
  return null;
}

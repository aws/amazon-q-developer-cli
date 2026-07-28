import type { WorkflowNodeState } from '../types/workflow.js';

export interface WorkflowStateEntry {
  state: WorkflowNodeState;
  nodePath: readonly string[];
  parentId: string | null;
  depth: number;
}

export function workflowNodePathsEqual(
  left: readonly string[] | undefined,
  right: readonly string[] | undefined
): boolean {
  if (!left || !right) return left === right;
  return (
    left.length === right.length &&
    left.every((segment, index) => segment === right[index])
  );
}

/** Normalize KAS repeat wrappers (`step#N`) to lifecycle path segments. */
export function workflowStatePathSegment(
  node: Pick<WorkflowNodeState, 'nodeId' | 'iteration'>,
  parentIsRepeat: boolean
): string {
  if (parentIsRepeat) {
    if (node.iteration !== undefined) return `iter-${node.iteration}`;
    const suffix = /#(\d+)$/.exec(node.nodeId)?.[1];
    if (suffix !== undefined) return `iter-${suffix}`;
  }
  return node.nodeId;
}

export function workflowStateEntries(
  root: WorkflowNodeState
): WorkflowStateEntry[] {
  const entries: WorkflowStateEntry[] = [];
  const visit = (
    state: WorkflowNodeState,
    parentId: string | null,
    parentPath: readonly string[],
    parentIsRepeat: boolean,
    depth: number
  ): void => {
    const nodePath = [
      ...parentPath,
      workflowStatePathSegment(state, parentIsRepeat),
    ];
    entries.push({ state, nodePath, parentId, depth });
    for (const child of state.children ?? []) {
      visit(child, state.nodeId, nodePath, state.type === 'repeat', depth + 1);
    }
  };
  visit(root, null, [], false, 0);
  return entries;
}

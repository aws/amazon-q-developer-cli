import type {
  WorkflowMonitorNode,
  WorkflowNodeConversation,
  WorkflowRunView,
} from '../types/workflow-monitor.js';
import { isLiveWorkflowStatus } from '../types/workflow-status.js';
import { workflowNodePathsEqual } from '../utils/workflow-node-path.js';

export function buildWorkflowNodeConversations(
  workflow: WorkflowRunView
): WorkflowNodeConversation[] {
  if (!workflow.parentSessionId) return [];
  return workflow.stepSessions.map((session) => {
    const exactNode = workflow.nodes.find(
      (node) => node.sessionId === session.sessionId
    );
    const logicalNode =
      exactNode ??
      workflow.nodes.find(
        (node) =>
          node.id === session.nodeId &&
          (node.nodePath === undefined ||
            workflowNodePathsEqual(node.nodePath, session.nodePath))
      );
    const fallbackStatus = isLiveWorkflowStatus(workflow.status)
      ? 'pending'
      : workflow.status;
    const label = logicalNode?.label ?? session.agentName ?? session.nodeId;
    return {
      target: {
        workflowId: workflow.workflowId,
        parentSessionId: workflow.parentSessionId!,
        nodeId: session.nodeId,
        nodePath: session.nodePath,
        sessionId: session.sessionId,
        iteration: session.iteration,
        branchId: session.branchId,
      },
      label:
        session.iteration === undefined
          ? label
          : `${label} - iteration ${session.iteration}`,
      agentName: session.agentName ?? logicalNode?.agentName,
      nodeStatus: session.status ?? logicalNode?.status ?? fallbackStatus,
    };
  });
}

export function buildWorkflowNodeConversation(
  workflow: WorkflowRunView,
  node: WorkflowMonitorNode
): WorkflowNodeConversation | null {
  if (node.type !== 'step' || node.sessionId === undefined) return null;
  return (
    buildWorkflowNodeConversations(workflow).find(
      (conversation) => conversation.target.sessionId === node.sessionId
    ) ?? null
  );
}

export function workflowProgress(nodes: readonly WorkflowMonitorNode[]): {
  completed: number;
  total: number;
} {
  const steps = nodes.filter((node) => node.type === 'step');
  return {
    completed: steps.filter((node) => node.status === 'completed').length,
    total: steps.length,
  };
}

export function workflowActivityCounts(workflows: Iterable<WorkflowRunView>): {
  running: number;
  paused: number;
} {
  let running = 0;
  let paused = 0;
  for (const workflow of workflows) {
    if (workflow.status === 'running') running += 1;
    if (workflow.status === 'paused') paused += 1;
  }
  return { running, paused };
}

import type {
  WorkflowMonitorNode,
  WorkflowNodeConversation,
  WorkflowRunView,
} from '../types/workflow-monitor.js';
import { isLiveWorkflowStatus } from '../types/workflow-status.js';
import { workflowNodePathsEqual } from '../utils/workflow-node-path.js';
import type {
  WorkflowInspectResponse,
  WorkflowRunSummary,
} from '../types/workflow-history.js';
import {
  buildWorkflowNodesFromState,
  collectWorkflowSessions,
} from './workflow-reducer.js';
import { isTerminalWorkflowStatus } from '../types/workflow-status.js';

function timestamp(value: string | undefined): number | null {
  if (!value) return null;
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? null : parsed;
}

export function buildHistoricalWorkflowRun(
  summary: WorkflowRunSummary,
  inspected: WorkflowInspectResponse
): WorkflowRunView {
  const { state } = inspected;
  const status = state.status ?? summary.status;
  return {
    workflowId: summary.workflowId,
    parentSessionId: state.parentSessionId ?? summary.parentSessionId,
    name: state.workflowName || summary.name,
    status,
    nodes: buildWorkflowNodesFromState(inspected.nodePlan, state.root),
    stepSessions: collectWorkflowSessions(state.root),
    startedAt:
      timestamp(summary.startedAt) ??
      timestamp(state.createdAt) ??
      timestamp(summary.createdAt),
    completedAt: isTerminalWorkflowStatus(status)
      ? (timestamp(summary.endedAt) ?? timestamp(summary.updatedAt))
      : null,
    pauseReason: state.pauseReason,
  };
}

export function summarizeWorkflowRuns(
  workflows: Iterable<WorkflowRunView>
): WorkflowRunSummary[] {
  return [...workflows].map((workflow) => {
    const startedAt =
      workflow.startedAt === null
        ? undefined
        : new Date(workflow.startedAt).toISOString();
    const endedAt =
      workflow.completedAt === null
        ? undefined
        : new Date(workflow.completedAt).toISOString();
    const createdAt = startedAt ?? endedAt ?? new Date(0).toISOString();
    return {
      workflowId: workflow.workflowId,
      name: workflow.name,
      status: workflow.status,
      createdAt,
      updatedAt: endedAt ?? startedAt ?? createdAt,
      startedAt,
      endedAt,
      parentSessionId: workflow.parentSessionId,
    };
  });
}

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
  const { running, paused } = workflowActivitySummary(workflows);
  return { running, paused };
}

export interface WorkflowActivitySummary {
  running: number;
  paused: number;
  completedSteps: number;
  totalSteps: number;
}

export function workflowActivitySummary(
  workflows: Iterable<WorkflowRunView>
): WorkflowActivitySummary {
  let running = 0;
  let paused = 0;
  let completedSteps = 0;
  let totalSteps = 0;

  for (const workflow of workflows) {
    switch (workflow.status) {
      case 'running':
        running += 1;
        break;
      case 'paused':
        paused += 1;
        break;
      case 'completed':
      case 'failed':
      case 'aborted':
        continue;
    }

    const progress = workflowProgress(workflow.nodes);
    completedSteps += progress.completed;
    totalSteps += progress.total;
  }

  return { running, paused, completedSteps, totalSteps };
}

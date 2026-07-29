import type {
  WorkflowProgressEvent,
  WorkflowStatus,
} from '../types/workflow.js';
import { isWorkflowLaunchTool } from '../types/agent-events.js';
import { MessageRole } from '../types/message-role.js';
import type {
  TerminalWorkflowStatus,
  WorkflowLifecycleMessage,
  WorkflowLifecycleNotice,
} from '../types/workflow-lifecycle.js';

function isTerminalStatus(
  status: WorkflowStatus
): status is TerminalWorkflowStatus {
  return status === 'completed' || status === 'failed' || status === 'aborted';
}

export function resolveWorkflowCompletion(
  event: WorkflowProgressEvent
): WorkflowLifecycleNotice | null {
  if (event.type !== 'run_complete' || !isTerminalStatus(event.status)) {
    return null;
  }

  return {
    workflowId: event.workflowId,
    workflowName: event.finalState.workflowName,
    status: event.status,
  };
}

export function workflowLifecycleContent(
  notice: WorkflowLifecycleNotice
): string {
  return `Workflow "${notice.workflowName}" ${notice.status}`;
}

export function appendWorkflowLifecycleMessage<T extends { id: string }>(
  messages: T[],
  notice: WorkflowLifecycleNotice
): Array<T | WorkflowLifecycleMessage> {
  const id = `workflow-lifecycle:${notice.workflowId}:${notice.status}`;
  if (messages.some((message) => message.id === id)) return messages;

  return [
    ...messages,
    {
      id,
      role: MessageRole.System,
      content: workflowLifecycleContent(notice),
      success: notice.status === 'started' || notice.status === 'completed',
      kind:
        notice.status === 'started'
          ? 'workflow-lifecycle'
          : 'workflow-completion',
      workflowId: notice.workflowId,
      workflowName: notice.workflowName,
      workflowStatus: notice.status,
      ...(notice.workflowTurnId
        ? { workflowTurnId: notice.workflowTurnId }
        : {}),
    },
  ];
}

interface WorkflowOriginCandidate {
  id: string;
  role: MessageRole;
  content?: string;
  name?: string;
  steered?: boolean;
}

/** Avoid a redundant start row when the launch tool already owns the turn. */
export function hasWorkflowLaunchToolForTurn(
  messages: readonly WorkflowOriginCandidate[],
  workflowTurnId: string | undefined
): boolean {
  if (!workflowTurnId) return false;
  const turnStart = messages.findIndex(
    (message) =>
      message.id === workflowTurnId && message.role === MessageRole.User
  );
  if (turnStart < 0) return false;

  for (let index = turnStart + 1; index < messages.length; index += 1) {
    const message = messages[index];
    if (!message) continue;
    if (message.role === MessageRole.User && message.steered !== true) {
      return false;
    }
    if (
      message.role === MessageRole.ToolUse &&
      isWorkflowLaunchTool(message.name)
    ) {
      return true;
    }
  }
  return false;
}

export function activeWorkflowOriginTurnId(
  messages: readonly WorkflowOriginCandidate[],
  isProcessing: boolean,
  workflowName?: string
): string | undefined {
  const candidate = messages.findLast(
    (message) => message.role === MessageRole.User && message.steered !== true
  );
  if (!candidate) return undefined;
  if (isProcessing) return candidate.id;

  if (
    workflowName === 'goal' &&
    candidate.content?.trimStart().startsWith('/goal ') === true
  ) {
    return candidate.id;
  }
  return undefined;
}

export interface WorkflowLifecycleTracker {
  recordOriginTurn(turnId: string): void;
  consume(
    event: WorkflowProgressEvent,
    fallbackOriginTurnId?: string
  ): WorkflowLifecycleNotice | null;
}

export function createWorkflowLifecycleTracker(
  initialOriginTurnId?: string
): WorkflowLifecycleTracker {
  let originTurnId = initialOriginTurnId;
  const workflowTurnIds = new Map<string, string>();

  return {
    recordOriginTurn(turnId) {
      originTurnId = turnId;
    },
    consume(event, fallbackOriginTurnId) {
      if (event.type === 'run_start') {
        const turnId = originTurnId ?? fallbackOriginTurnId;
        if (turnId) workflowTurnIds.set(event.workflowId, turnId);
        return {
          workflowId: event.workflowId,
          workflowName: event.workflowName,
          status: 'started',
          ...(turnId ? { workflowTurnId: turnId } : {}),
        };
      }

      const completion = resolveWorkflowCompletion(event);
      if (!completion) return null;

      const workflowTurnId = workflowTurnIds.get(completion.workflowId);
      workflowTurnIds.delete(completion.workflowId);
      return {
        ...completion,
        ...(workflowTurnId ? { workflowTurnId } : {}),
      };
    },
  };
}

import type { ExecutionTarget } from './session-client';

export type SessionStatus =
  | 'idle'
  | 'busy'
  | 'terminated'
  | 'failed'
  | 'pending';
export type SessionType = 'persistent' | 'ephemeral';

export enum SessionLifecycleOwner {
  WorkflowExtension = 'workflow-extension',
}

export interface AgentSession {
  id: string;
  name: string;
  agentName?: string;
  role?: string;
  group?: string;
  status: SessionStatus;
  type: SessionType;
  created: Date;
  lastActivity: Date;
  summary?: string;
  parentSession?: string;
  stageInfo?: {
    name: string;
    role: string;
  };
  dependsOn?: string[]; // DAG edges: names of stages this one depends on
  hasLoop?: boolean; // Whether this stage has a loop-back config
  loopIteration?: number; // Current loop iteration (0 if not looping)
  loopMaxIterations?: number; // Max loop iterations
  /**
   * Where this session's agent runs. Absent == local (today's behavior).
   * Populated for remote/cloud sessions; read by {@link isRemoteSession}.
   */
  executionTarget?: ExecutionTarget;
  /** Prevents generic session cleanup from taking over extension-owned state. */
  lifecycleOwner?: SessionLifecycleOwner;
}

/**
 * True when an execution-target *kind* string denotes a cloud (non-local)
 * placement. Fail-closed: absent/`'local'` is local; every other value is
 * treated as cloud and gated accordingly — `'cloud-sandbox'` (the cloud-session
 * feature), and, defensively, any future or unknown kind so a new value can
 * never be silently mislabeled local. Note `'remote-control'` is a separate,
 * not-yet-shipped feature; until it has dedicated handling this fail-closed
 * check also returns true for it. Single source of truth for the
 * local-vs-cloud classification (mirrors the Rust `is_definitively_local`
 * gate); prefer it over inline `=== 'cloud-sandbox'` checks.
 */
export function isCloudExecutionTargetKind(
  kind: string | undefined | null
): boolean {
  return kind !== undefined && kind !== null && kind !== 'local';
}

/**
 * True when a session runs somewhere other than the local machine (cloud
 * sandbox or remote-control). Absent/`local` execution target == not remote.
 * This is the single gate used to vary TUI behavior for remote sessions.
 */
export function isRemoteSession(
  session: Pick<AgentSession, 'executionTarget'> | undefined | null
): boolean {
  return isCloudExecutionTargetKind(session?.executionTarget?.kind);
}

/**
 * True when lifecycle and conversation cleanup belong to the workflow
 * extension.
 */
export function isWorkflowSession(
  session: Pick<AgentSession, 'lifecycleOwner'> | undefined | null
): boolean {
  return session?.lifecycleOwner === SessionLifecycleOwner.WorkflowExtension;
}

export type SessionLifecycleEvent =
  | {
      type: 'session_created';
      session: AgentSession;
    }
  | {
      type: 'session_terminated';
      sessionId: string;
    };

export type WorkflowSessionControlEvent =
  | {
      type: 'session_removed';
      sessionId: string;
    }
  | {
      type: 'session_status_changed';
      sessionId: string;
      status: SessionStatus;
    }
  | {
      type: 'session_conversation_reset' | 'session_turn_started';
      sessionId: string;
    }
  | {
      type: 'session_approvals_cancelled';
      sessionId: string;
    };

export type SessionEvent = SessionLifecycleEvent | WorkflowSessionControlEvent;

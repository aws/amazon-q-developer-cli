import type { AgentStreamEvent } from './agent-events';

import type { ExecutionTarget } from './session-client';

export type SessionStatus =
  | 'idle'
  | 'busy'
  | 'terminated'
  | 'failed'
  | 'pending';
export type SessionType = 'persistent' | 'ephemeral';

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
}

/**
 * True when a session runs somewhere other than the local machine (cloud
 * sandbox or remote-control). Absent/`local` execution target == not remote.
 * This is the single gate used to vary TUI behavior for remote sessions.
 */
export function isRemoteSession(
  session: Pick<AgentSession, 'executionTarget'> | undefined | null
): boolean {
  const kind = session?.executionTarget?.kind;
  return kind !== undefined && kind !== 'local';
}

export interface InboxMessage {
  id: string;
  from: string;
  to: string;
  content: string;
  timestamp: Date;
  priority: 'normal' | 'escalation';
  read: boolean;
}

export interface SessionEvent {
  type: 'session_created' | 'session_terminated' | 'session_status_changed';
  sessionId: string;
  session: AgentSession;
}

export interface MessageEvent {
  type: 'message_received' | 'message_sent';
  sessionId: string;
  message: InboxMessage;
}

export interface MultiAgentEvent {
  sessionId: string;
  event: AgentStreamEvent;
}

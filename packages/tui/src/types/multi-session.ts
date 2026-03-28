import type { AgentStreamEvent } from './agent-events';

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

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
 * Subagent lifecycle event: `session_created` carries the full roster entry
 * for a newly started subagent session; `session_terminated` carries only
 * the ended session's id. Emitted on the v3 engine path only - the V2 engine
 * conveys lifecycle implicitly through roster snapshots.
 */
export type SessionLifecycleEvent =
  | { type: 'session_created'; session: AgentSession }
  | { type: 'session_terminated'; sessionId: string };

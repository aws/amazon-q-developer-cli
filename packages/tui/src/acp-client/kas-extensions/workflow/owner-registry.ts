import type {
  WorkflowEvent,
  WorkflowLoadResponse,
  WorkflowNodeSessionTarget,
  WorkflowNodeState,
  WorkflowNodeStatus,
  WorkflowStateSnapshot,
  WorkflowStepSessionRef,
} from '../../../types/workflow.js';
import {
  workflowNodePathsEqual,
  workflowStateEntries,
  type WorkflowStateEntry,
} from '../../../utils/workflow-node-path.js';

export interface WorkflowSessionOwner extends WorkflowNodeSessionTarget {
  status: WorkflowNodeStatus;
  agentName?: string;
}

export interface WorkflowLoadedRunRegistration {
  owners: WorkflowSessionOwner[];
  removedSessionIds: string[];
}

interface WorkflowOwnerRegistration {
  owner: WorkflowSessionOwner;
  removedSessionIds: string[];
}

export type WorkflowRouteRejectionReason =
  | 'missing parent session ownership'
  | 'workflow belongs to a different parent session'
  | 'conflicting workflow terminal identity'
  | 'conflicting workflow session ownership';

export type WorkflowRouteResult =
  | {
      accepted: true;
      parentSessionId: string;
      changedOwners: WorkflowSessionOwner[];
      removedSessionIds: string[];
    }
  | {
      accepted: false;
      parentSessionId?: string;
      changedOwners: [];
      reason: WorkflowRouteRejectionReason;
    };

function targetsMatch(
  target: WorkflowNodeSessionTarget,
  owner: WorkflowSessionOwner
): boolean {
  return (
    target.sessionId === owner.sessionId &&
    target.workflowId === owner.workflowId &&
    target.parentSessionId === owner.parentSessionId &&
    target.nodeId === owner.nodeId &&
    workflowNodePathsEqual(target.nodePath, owner.nodePath) &&
    (target.iteration === undefined || target.iteration === owner.iteration) &&
    (target.branchId === undefined || target.branchId === owner.branchId)
  );
}

function ownersCompatible(
  existing: WorkflowSessionOwner,
  incoming: WorkflowSessionOwner
): boolean {
  return (
    existing.sessionId === incoming.sessionId &&
    existing.workflowId === incoming.workflowId &&
    existing.parentSessionId === incoming.parentSessionId &&
    existing.nodeId === incoming.nodeId &&
    workflowNodePathsEqual(existing.nodePath, incoming.nodePath) &&
    (existing.iteration === undefined ||
      incoming.iteration === undefined ||
      existing.iteration === incoming.iteration) &&
    (existing.branchId === undefined ||
      incoming.branchId === undefined ||
      existing.branchId === incoming.branchId)
  );
}

function stateRefMatchesOwner(
  ref: WorkflowStepSessionRef,
  owner: WorkflowSessionOwner
): boolean {
  return (
    ref.nodeId === owner.nodeId &&
    workflowNodePathsEqual(ref.nodePath, owner.nodePath) &&
    (ref.iteration === undefined ||
      owner.iteration === undefined ||
      ref.iteration === owner.iteration) &&
    (ref.branchId === undefined ||
      owner.branchId === undefined ||
      ref.branchId === owner.branchId)
  );
}

function collectStateSessionRefs(
  entries: readonly WorkflowStateEntry[]
): WorkflowStepSessionRef[] {
  return entries.flatMap(({ state, nodePath }) =>
    state.sessionId
      ? [
          {
            nodeId: state.nodeId,
            nodePath,
            sessionId: state.sessionId,
            iteration: state.iteration,
            branchId: state.branchId,
          },
        ]
      : []
  );
}

function stateSessions(
  entries: readonly WorkflowStateEntry[]
): Map<string, WorkflowNodeState> {
  const sessions = new Map<string, WorkflowNodeState>();
  for (const { state } of entries) {
    if (state.sessionId) sessions.set(state.sessionId, state);
  }
  return sessions;
}

function mergeSessionRefs(
  existing: WorkflowStepSessionRef,
  incoming: WorkflowStepSessionRef
): WorkflowStepSessionRef | undefined {
  if (
    existing.nodeId !== incoming.nodeId ||
    !workflowNodePathsEqual(existing.nodePath, incoming.nodePath) ||
    (existing.iteration !== undefined &&
      incoming.iteration !== undefined &&
      existing.iteration !== incoming.iteration) ||
    (existing.branchId !== undefined &&
      incoming.branchId !== undefined &&
      existing.branchId !== incoming.branchId)
  ) {
    return undefined;
  }
  return {
    nodeId: existing.nodeId,
    sessionId: existing.sessionId,
    nodePath: incoming.nodePath,
    iteration: incoming.iteration ?? existing.iteration,
    branchId: incoming.branchId ?? existing.branchId,
  };
}

function refsOverlapNodeIdentity(
  left: WorkflowStepSessionRef,
  right: WorkflowStepSessionRef
): boolean {
  return (
    left.nodeId === right.nodeId &&
    workflowNodePathsEqual(left.nodePath, right.nodePath) &&
    (left.iteration === undefined ||
      right.iteration === undefined ||
      left.iteration === right.iteration) &&
    (left.branchId === undefined ||
      right.branchId === undefined ||
      left.branchId === right.branchId)
  );
}

/**
 * Session ownership registry for KAS workflows.
 *
 * This class has no transport or UI dependencies. It accepts only explicit
 * KAS identities and rejects missing or conflicting parent ownership.
 */
export class WorkflowOwnerRegistry {
  private readonly workflowParents = new Map<string, string>();
  private readonly owners = new Map<string, WorkflowSessionOwner>();

  retainParent(parentSessionId: string): string[] {
    const removed: string[] = [];
    for (const [sessionId, owner] of this.owners) {
      if (owner.parentSessionId === parentSessionId) continue;
      this.owners.delete(sessionId);
      removed.push(sessionId);
    }
    for (const [workflowId, ownerParent] of this.workflowParents) {
      if (ownerParent !== parentSessionId) {
        this.workflowParents.delete(workflowId);
      }
    }
    return removed;
  }

  clear(): void {
    this.workflowParents.clear();
    this.owners.clear();
  }

  routeLifecycle(
    event: WorkflowEvent,
    activeParentSessionId: string | undefined
  ): WorkflowRouteResult {
    if (
      event.type === 'run_complete' &&
      (event.finalState.workflowId !== event.workflowId ||
        event.finalState.status !== event.status ||
        (event.parentSessionId !== undefined &&
          event.finalState.parentSessionId !== undefined &&
          event.parentSessionId !== event.finalState.parentSessionId))
    ) {
      return {
        accepted: false,
        changedOwners: [],
        reason: 'conflicting workflow terminal identity',
      };
    }
    if (
      event.type === 'run_complete' &&
      this.hasConflictingTerminalOwner(event)
    ) {
      return {
        accepted: false,
        changedOwners: [],
        reason: 'conflicting workflow terminal identity',
      };
    }

    const existingParent = this.workflowParents.get(event.workflowId);
    const reportedParent =
      event.parentSessionId ??
      (event.type === 'run_complete'
        ? event.finalState.parentSessionId
        : undefined);
    const parentSessionId = reportedParent ?? existingParent;

    if (!activeParentSessionId || !parentSessionId) {
      return {
        accepted: false,
        changedOwners: [],
        reason: 'missing parent session ownership',
      };
    }
    if (
      parentSessionId !== activeParentSessionId ||
      (existingParent !== undefined && existingParent !== parentSessionId)
    ) {
      return {
        accepted: false,
        parentSessionId,
        changedOwners: [],
        reason: 'workflow belongs to a different parent session',
      };
    }

    this.workflowParents.set(event.workflowId, parentSessionId);
    const changedOwners: WorkflowSessionOwner[] = [];
    const removedSessionIds: string[] = [];

    if (event.type === 'node_start' && event.sessionId) {
      const registration = this.upsertOwner({
        workflowId: event.workflowId,
        parentSessionId,
        nodeId: event.nodeId,
        nodePath: event.nodePath,
        sessionId: event.sessionId,
        iteration: event.iteration,
        branchId: event.branchId,
        status: 'running',
        agentName: event.agentName,
      });
      if (!registration) {
        if (existingParent === undefined) {
          this.workflowParents.delete(event.workflowId);
        }
        return {
          accepted: false,
          parentSessionId,
          changedOwners: [],
          reason: 'conflicting workflow session ownership',
        };
      }
      changedOwners.push(registration.owner);
      removedSessionIds.push(...registration.removedSessionIds);
    } else if (event.type === 'node_complete' || event.type === 'node_paused') {
      const status = event.type === 'node_paused' ? 'paused' : event.status;
      const owner = this.updateMatchingOwner(
        event.workflowId,
        event.nodeId,
        event.nodePath,
        event.sessionId,
        event.iteration,
        event.branchId,
        status
      );
      if (owner) changedOwners.push(owner);
    } else if (event.type === 'run_complete') {
      const statesBySession = stateSessions(
        workflowStateEntries(event.finalState.root)
      );
      for (const owner of this.owners.values()) {
        if (owner.workflowId !== event.workflowId) continue;
        const state = statesBySession.get(owner.sessionId);
        if (!state || state.status === owner.status) continue;
        const updated = { ...owner, status: state.status };
        this.owners.set(owner.sessionId, updated);
        changedOwners.push(updated);
      }
    }

    return {
      accepted: true,
      parentSessionId,
      changedOwners,
      removedSessionIds,
    };
  }

  registerLoadedRun(
    response: WorkflowLoadResponse,
    activeParentSessionId: string | undefined
  ): WorkflowLoadedRunRegistration | undefined {
    const parentSessionId = response.state.parentSessionId;
    if (
      !activeParentSessionId ||
      !parentSessionId ||
      parentSessionId !== activeParentSessionId
    ) {
      return undefined;
    }
    const existingParent = this.workflowParents.get(response.workflowId);
    if (existingParent && existingParent !== parentSessionId) return undefined;
    const refsBySession = new Map<string, WorkflowStepSessionRef>();
    const addRef = (ref: WorkflowStepSessionRef): boolean => {
      if (ref.sessionId === parentSessionId) return false;
      const existing = refsBySession.get(ref.sessionId);
      const merged = existing ? mergeSessionRefs(existing, ref) : ref;
      if (!merged) return false;
      for (const candidate of refsBySession.values()) {
        if (
          candidate.sessionId !== ref.sessionId &&
          refsOverlapNodeIdentity(candidate, merged)
        ) {
          return false;
        }
      }
      refsBySession.set(ref.sessionId, merged);
      return true;
    };
    const stateEntries = workflowStateEntries(response.state.root);
    const statesBySession = stateSessions(stateEntries);
    for (const ref of collectStateSessionRefs(stateEntries)) {
      if (!addRef(ref)) return undefined;
    }
    for (const ref of response.stepSessions) {
      if (!addRef(ref)) return undefined;
    }

    const stagedOwners: WorkflowSessionOwner[] = [];
    const removedSessionIds = new Set<string>();
    for (const ref of refsBySession.values()) {
      const state = statesBySession.get(ref.sessionId);
      const incoming: WorkflowSessionOwner = {
        ...ref,
        workflowId: response.workflowId,
        parentSessionId,
        status: state?.status ?? 'pending',
        agentName: state?.agentName,
      };
      const existing = this.owners.get(ref.sessionId);
      if (existing && !ownersCompatible(existing, incoming)) return undefined;
      stagedOwners.push({
        ...existing,
        ...incoming,
        iteration: incoming.iteration ?? existing?.iteration,
        branchId: incoming.branchId ?? existing?.branchId,
        agentName: incoming.agentName ?? existing?.agentName,
      });
    }
    for (const candidate of this.owners.values()) {
      if (
        candidate.workflowId !== response.workflowId ||
        candidate.parentSessionId !== parentSessionId
      ) {
        continue;
      }
      for (const owner of stagedOwners) {
        if (
          candidate.sessionId !== owner.sessionId &&
          refsOverlapNodeIdentity(candidate, owner)
        ) {
          removedSessionIds.add(candidate.sessionId);
        }
      }
    }

    this.workflowParents.set(response.workflowId, parentSessionId);
    for (const sessionId of removedSessionIds) {
      this.owners.delete(sessionId);
    }
    for (const owner of stagedOwners) {
      this.owners.set(owner.sessionId, owner);
    }
    return {
      owners: stagedOwners,
      removedSessionIds: [...removedSessionIds],
    };
  }

  resolveTarget(
    target: WorkflowNodeSessionTarget
  ): WorkflowSessionOwner | undefined {
    const owner = this.owners.get(target.sessionId);
    return owner && targetsMatch(target, owner) ? owner : undefined;
  }

  ownerForSession(sessionId: string): WorkflowSessionOwner | undefined {
    return this.owners.get(sessionId);
  }

  ownersForWorkflow(workflowId: string): WorkflowSessionOwner[] {
    return [...this.owners.values()].filter(
      (owner) => owner.workflowId === workflowId
    );
  }

  private hasConflictingTerminalOwner(
    event: Extract<WorkflowEvent, { type: 'run_complete' }> & {
      finalState: WorkflowStateSnapshot;
    }
  ): boolean {
    const refs = collectStateSessionRefs(
      workflowStateEntries(event.finalState.root)
    );
    for (const owner of this.owners.values()) {
      if (owner.workflowId !== event.workflowId) continue;
      const matchingSessionRefs = refs.filter(
        (ref) => ref.sessionId === owner.sessionId
      );
      if (
        matchingSessionRefs.length > 1 ||
        matchingSessionRefs.some((ref) => !stateRefMatchesOwner(ref, owner))
      ) {
        return true;
      }
    }
    return false;
  }

  private upsertOwner(
    owner: WorkflowSessionOwner
  ): WorkflowOwnerRegistration | undefined {
    if (owner.sessionId === owner.parentSessionId) return undefined;
    const existing = this.owners.get(owner.sessionId);
    if (existing && !ownersCompatible(existing, owner)) return undefined;
    const merged: WorkflowSessionOwner = {
      ...existing,
      ...owner,
      nodePath: owner.nodePath ?? existing?.nodePath,
      iteration: owner.iteration ?? existing?.iteration,
      branchId: owner.branchId ?? existing?.branchId,
      agentName: owner.agentName ?? existing?.agentName,
    };
    const removedSessionIds = [...this.owners.values()]
      .filter(
        (candidate) =>
          candidate.sessionId !== merged.sessionId &&
          candidate.workflowId === merged.workflowId &&
          candidate.parentSessionId === merged.parentSessionId &&
          refsOverlapNodeIdentity(candidate, merged)
      )
      .map((candidate) => candidate.sessionId);
    if (removedSessionIds.length > 1) return undefined;

    this.owners.set(owner.sessionId, merged);
    for (const sessionId of removedSessionIds) {
      this.owners.delete(sessionId);
    }
    return { owner: merged, removedSessionIds };
  }

  private updateMatchingOwner(
    workflowId: string,
    nodeId: string,
    nodePath: readonly string[] | undefined,
    sessionId: string | undefined,
    iteration: number | undefined,
    branchId: string | undefined,
    status: WorkflowNodeStatus
  ): WorkflowSessionOwner | undefined {
    const candidates = [...this.owners.values()].filter((owner) => {
      if (owner.workflowId !== workflowId || owner.nodeId !== nodeId) {
        return false;
      }
      if (sessionId && owner.sessionId !== sessionId) return false;
      if (nodePath && !workflowNodePathsEqual(nodePath, owner.nodePath)) {
        return false;
      }
      if (iteration !== undefined && owner.iteration !== iteration)
        return false;
      if (branchId !== undefined && owner.branchId !== branchId) return false;
      return true;
    });
    if (candidates.length !== 1) return undefined;
    const updated = { ...candidates[0]!, status };
    this.owners.set(updated.sessionId, updated);
    return updated;
  }
}

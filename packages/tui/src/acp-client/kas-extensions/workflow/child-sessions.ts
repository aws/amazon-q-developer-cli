import * as acp from '@agentclientprotocol/sdk';
import { logger } from '../../../utils/logger.js';
import type {
  AgentStreamEvent,
  ApprovalRequestInfo,
} from '../../../types/agent-events.js';
import { isTerminalWorkflowNodeStatus } from '../../../types/workflow-status.js';
import type { AcpSessionUpdate } from '../../base.js';
import type { Disposable, KasExtensionRuntime } from '../runtime.js';
import { WorkflowChildMessageState } from './event-routing.js';
import type { WorkflowSessionOwner } from './owner-registry.js';
import type { WorkflowExtensionHost } from './ports.js';

export interface WorkflowChildSessionOwnership {
  ownerForSession(sessionId: string): WorkflowSessionOwner | undefined;
  isOwnerActive(owner: WorkflowSessionOwner): boolean;
}

interface ReplayBuffer {
  events: AgentStreamEvent[];
}

/** Owns child ACP leases, transcript replay, and prompt/steer delivery. */
export class WorkflowChildSessions {
  private readonly leases = new Map<string, Disposable>();
  private readonly transcriptLoads = new Map<string, Promise<void>>();
  private readonly loadedTranscripts = new Set<string>();
  private readonly liveTranscripts = new Set<string>();
  private readonly replayBuffers = new Map<string, ReplayBuffer>();
  private readonly pendingApprovals = new Map<
    string,
    Set<ApprovalRequestInfo>
  >();
  private readonly activeOperations = new Map<string, number>();
  private readonly deferredLeaseDisposals = new Set<string>();
  private readonly messages = new WorkflowChildMessageState();
  private disposed = false;

  constructor(
    private readonly runtime: KasExtensionRuntime,
    private readonly host: WorkflowExtensionHost,
    private readonly ownership: WorkflowChildSessionOwnership
  ) {}

  ensure(owner: WorkflowSessionOwner, hideInitialPrompt = false): boolean {
    this.assertActive();
    if (!this.ownership.isOwnerActive(owner)) {
      throw new Error('Workflow node ownership changed');
    }
    if (hideInitialPrompt) {
      this.messages.hideInitialPrompt(owner.sessionId);
    }
    if (this.leases.has(owner.sessionId)) return false;

    const lease = this.runtime.leaseSession(owner.sessionId, {
      onUpdate: async (notification) => {
        this.handleSessionUpdate(owner.sessionId, notification);
      },
      onPermission: (request) =>
        this.handlePermissionRequest(owner.sessionId, request),
    });
    this.leases.set(owner.sessionId, lease);
    return true;
  }

  routeExternalEvent(
    owner: WorkflowSessionOwner,
    event: AgentStreamEvent
  ): void {
    if (!this.ownership.isOwnerActive(owner)) return;
    this.routeEvent(owner.sessionId, event);
  }

  async messageNode(
    owner: WorkflowSessionOwner,
    content: string
  ): Promise<void> {
    const text = content.trim();
    if (!text) throw new Error('Workflow node message cannot be empty');

    this.beginOperation(owner.sessionId);
    try {
      this.ensure(owner);
      let delivery = this.messages.messageDelivery(owner);
      if (delivery === 'steer') {
        await this.host.steerSession(owner.sessionId, text);
        return;
      }

      try {
        await this.loadTranscript(owner, { requirePersisted: true });
      } catch (error) {
        if (isTerminalWorkflowNodeStatus(owner.status)) {
          this.disposeLeaseWhenIdle(owner.sessionId);
        }
        throw error;
      }
      this.assertOwnerActive(owner);

      // Another caller may have started a prompt while both waited for the
      // same transcript replay. Re-evaluate so only one prompt is opened.
      delivery = this.messages.messageDelivery(owner);
      if (delivery === 'steer') {
        await this.host.steerSession(owner.sessionId, text);
        return;
      }

      this.messages.beginPrompt(owner.sessionId);
      this.host.emitEffect({
        type: 'child_turn_started',
        sessionId: owner.sessionId,
      });
      this.host.emitEffect({ type: 'child_busy', sessionId: owner.sessionId });

      try {
        try {
          await this.runtime.promptSession(owner.sessionId, text);
        } catch (error) {
          if (!this.isSessionNotFoundError(error)) throw error;
          this.loadedTranscripts.delete(owner.sessionId);
          this.liveTranscripts.delete(owner.sessionId);
          await this.loadTranscript(owner, {
            force: true,
            requirePersisted: true,
          });
          this.assertOwnerActive(owner);
          await this.runtime.promptSession(owner.sessionId, text);
        }
      } finally {
        this.messages.endPrompt(owner.sessionId);
        const currentOwner = this.ownership.ownerForSession(owner.sessionId);
        if (currentOwner && this.ownership.isOwnerActive(currentOwner)) {
          this.host.emitEffect({
            type: 'child_status_restored',
            owner: currentOwner,
          });
          if (isTerminalWorkflowNodeStatus(currentOwner.status)) {
            this.disposeLeaseWhenIdle(currentOwner.sessionId);
          }
        }
      }
    } finally {
      this.endOperation(owner.sessionId);
    }
  }

  disposeLease(sessionId: string): void {
    this.deferredLeaseDisposals.delete(sessionId);
    this.cancelPendingApprovals(sessionId);
    this.leases.get(sessionId)?.dispose();
    this.leases.delete(sessionId);
    this.host.clearSessionState(sessionId);
  }

  disposeLeaseWhenIdle(sessionId: string): void {
    if ((this.activeOperations.get(sessionId) ?? 0) > 0) {
      this.deferredLeaseDisposals.add(sessionId);
      return;
    }
    this.disposeLease(sessionId);
  }

  remove(sessionId: string): void {
    this.disposeLease(sessionId);
    this.messages.removeSession(sessionId);
    this.transcriptLoads.delete(sessionId);
    this.loadedTranscripts.delete(sessionId);
    this.liveTranscripts.delete(sessionId);
    this.replayBuffers.delete(sessionId);
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    const sessionIds = new Set([
      ...this.leases.keys(),
      ...this.pendingApprovals.keys(),
    ]);
    for (const sessionId of sessionIds) {
      this.disposeLease(sessionId);
    }
    this.messages.clear();
    this.transcriptLoads.clear();
    this.loadedTranscripts.clear();
    this.liveTranscripts.clear();
    this.replayBuffers.clear();
    this.pendingApprovals.clear();
    this.activeOperations.clear();
    this.deferredLeaseDisposals.clear();
  }

  private handleSessionUpdate(
    sessionId: string,
    notification: acp.SessionNotification
  ): void {
    if (
      notification.sessionId !== sessionId ||
      !this.ownership.ownerForSession(sessionId)
    ) {
      return;
    }
    const sink = (event: AgentStreamEvent) => this.routeEvent(sessionId, event);
    const event = this.host.convertUpdate(
      notification.update as AcpSessionUpdate,
      sessionId,
      sink
    );
    if (event) sink(event);
  }

  private handlePermissionRequest(
    sessionId: string,
    request: acp.RequestPermissionRequest
  ): Promise<acp.RequestPermissionResponse> {
    if (
      request.sessionId !== sessionId ||
      !this.ownership.ownerForSession(sessionId)
    ) {
      logger.warn(
        '[acp-client] Rejected permission request for an unowned workflow session',
        { requestSessionId: request.sessionId, sessionId }
      );
      return Promise.resolve({ outcome: { outcome: 'cancelled' } });
    }
    return this.host.routePermissionRequest(request, sessionId, (event) => {
      const approval = this.trackApproval(sessionId, event.value);
      this.host.emitEffect({
        type: 'child_approval_requested',
        sessionId,
        event: { ...event, value: approval },
      });
    });
  }

  private trackApproval(
    sessionId: string,
    approval: ApprovalRequestInfo
  ): ApprovalRequestInfo {
    const pending =
      this.pendingApprovals.get(sessionId) ?? new Set<ApprovalRequestInfo>();
    this.pendingApprovals.set(sessionId, pending);

    const tracked: ApprovalRequestInfo = {
      ...approval,
      sessionId,
      originSessionId: approval.originSessionId ?? sessionId,
      resolve: (response) => {
        if (!pending.delete(tracked)) return;
        if (pending.size === 0) this.pendingApprovals.delete(sessionId);
        approval.resolve(response);
      },
    };
    pending.add(tracked);
    return tracked;
  }

  private cancelPendingApprovals(sessionId: string): void {
    const pending = this.pendingApprovals.get(sessionId);
    if (!pending?.size) return;
    for (const approval of [...pending]) {
      approval.resolve({ outcome: 'cancelled' });
    }
    this.host.emitEffect({
      type: 'child_approvals_cancelled',
      sessionId,
    });
  }

  private routeEvent(sessionId: string, event: AgentStreamEvent): void {
    if (this.messages.shouldHideInitialPrompt(sessionId, event)) return;
    const replay = this.replayBuffers.get(sessionId);
    if (replay) {
      replay.events.push(event);
      return;
    }
    this.liveTranscripts.add(sessionId);
    this.host.emitEffect({ type: 'child_event', sessionId, event });
  }

  private async loadTranscript(
    owner: WorkflowSessionOwner,
    options: { force?: boolean; requirePersisted?: boolean } = {}
  ): Promise<void> {
    this.assertOwnerActive(owner);
    if (!options.force) {
      if (this.loadedTranscripts.has(owner.sessionId)) return;
      if (
        !options.requirePersisted &&
        this.liveTranscripts.has(owner.sessionId)
      ) {
        return;
      }
    }
    const pending = this.transcriptLoads.get(owner.sessionId);
    if (pending) return pending;

    const replay: ReplayBuffer = { events: [] };
    this.replayBuffers.set(owner.sessionId, replay);
    this.messages.hideInitialPrompt(owner.sessionId);

    const load = (async () => {
      this.ensure(owner);
      try {
        await this.runtime.replaySession(owner.sessionId);
        if (this.replayBuffers.get(owner.sessionId) !== replay) {
          throw new Error('Workflow node ownership changed during replay');
        }
        this.assertOwnerActive(owner);
        this.host.emitEffect({
          type: 'child_conversation_reset',
          sessionId: owner.sessionId,
        });
        for (const event of replay.events) {
          this.host.emitEffect({
            type: 'child_event',
            sessionId: owner.sessionId,
            event,
          });
        }
        this.loadedTranscripts.add(owner.sessionId);
      } finally {
        if (this.replayBuffers.get(owner.sessionId) === replay) {
          this.replayBuffers.delete(owner.sessionId);
        }
        this.messages.finishInitialPromptSuppression(owner.sessionId);
      }
    })().finally(() => {
      if (this.transcriptLoads.get(owner.sessionId) === load) {
        this.transcriptLoads.delete(owner.sessionId);
      }
    });

    this.transcriptLoads.set(owner.sessionId, load);
    return load;
  }

  private assertOwnerActive(owner: WorkflowSessionOwner): void {
    this.assertActive();
    if (!this.ownership.isOwnerActive(owner)) {
      throw new Error('Workflow node ownership changed during the operation');
    }
  }

  private beginOperation(sessionId: string): void {
    this.assertActive();
    this.activeOperations.set(
      sessionId,
      (this.activeOperations.get(sessionId) ?? 0) + 1
    );
  }

  private endOperation(sessionId: string): void {
    const remaining = (this.activeOperations.get(sessionId) ?? 0) - 1;
    if (remaining > 0) {
      this.activeOperations.set(sessionId, remaining);
      return;
    }
    this.activeOperations.delete(sessionId);
    if (this.deferredLeaseDisposals.delete(sessionId)) {
      this.disposeLease(sessionId);
    }
  }

  private assertActive(): void {
    if (this.disposed) {
      throw new Error('Workflow child sessions are disposed');
    }
  }

  private isSessionNotFoundError(error: unknown): boolean {
    const message = error instanceof Error ? error.message : String(error);
    return /session(?:\s+id)?\s+(?:was\s+)?not\s+found|unknown session/i.test(
      message
    );
  }
}

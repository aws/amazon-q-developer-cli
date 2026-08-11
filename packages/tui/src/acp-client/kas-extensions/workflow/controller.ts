import {
  AgentEventType,
  type AgentStreamEvent,
  type KiroMeta,
} from '../../../types/agent-events.js';
import type {
  WorkflowConversationApi,
  WorkflowEvent,
  WorkflowLoadResponse,
  WorkflowNodeSessionTarget,
  WorkflowRestoreSummary,
} from '../../../types/workflow.js';
import type {
  WorkflowActionAttribution,
  WorkflowCancelResponse,
  WorkflowControlApi,
  WorkflowInspectResponse,
  WorkflowPauseResponse,
  WorkflowRetryResponse,
  WorkflowResumeResponse,
  WorkflowRunSummary,
} from '../../../types/workflow-history.js';
import type {
  WorkflowCreateRequest,
  WorkflowCreateResponse,
  WorkflowInvokeResponse,
  WorkflowRecipeDescriptor,
} from '../../../types/workflow-launch.js';
import { isTerminalWorkflowNodeStatus } from '../../../types/workflow-status.js';
import { logger } from '../../../utils/logger.js';
import type { Disposable, KasExtensionRuntime } from '../runtime.js';
import { WorkflowChildSessions } from './child-sessions.js';
import {
  WORKFLOW_CANCEL_CONTRACT,
  WORKFLOW_CREATE_CONTRACT,
  WORKFLOW_INSPECT_CONTRACT,
  WORKFLOW_INVOKE_CONTRACT,
  WORKFLOW_LIFECYCLE_CONTRACTS,
  WORKFLOW_LIST_CONTRACT,
  WORKFLOW_LIST_RECIPES_CONTRACT,
  WORKFLOW_LOAD_CONTRACT,
  WORKFLOW_PAUSE_CONTRACT,
  WORKFLOW_RETRY_CONTRACT,
  WORKFLOW_RESUME_CONTRACT,
} from './contracts.js';
import { WorkflowParentRelayFilter } from './event-routing.js';
import type { WorkflowExtensionHost } from './ports.js';
import {
  WorkflowOwnerRegistry,
  type WorkflowSessionOwner,
} from './owner-registry.js';

/**
 * Coordinates the typed KAS workflow extension.
 *
 * Transport leases, replay, and prompt delivery belong to
 * `WorkflowChildSessions`; identity belongs to `WorkflowOwnerRegistry`; and
 * parent-stream filtering belongs to `WorkflowParentRelayFilter`.
 */
export class KasWorkflowExtension
  implements WorkflowConversationApi, WorkflowControlApi
{
  private readonly owners = new WorkflowOwnerRegistry();
  private readonly parentRelays = new WorkflowParentRelayFilter();
  private readonly children: WorkflowChildSessions;
  private readonly notificationDisposables: Disposable[] = [];
  private readonly registeredSessions = new Set<string>();
  private parentSessionId?: string;
  private started = false;

  constructor(
    private readonly runtime: KasExtensionRuntime,
    private readonly host: WorkflowExtensionHost
  ) {
    this.children = new WorkflowChildSessions(runtime, host, {
      ownerForSession: (sessionId) => this.owners.ownerForSession(sessionId),
      isOwnerActive: (owner) => this.isOwnerActive(owner),
    });
  }

  start(): void {
    if (this.started) return;
    this.started = true;
    for (const contract of WORKFLOW_LIFECYCLE_CONTRACTS) {
      this.notificationDisposables.push(
        this.runtime.subscribe(contract, (event) => {
          const accepted = this.acceptLifecycle(event);
          if (accepted) {
            this.host.emitEffect({
              type: 'workflow_progress',
              event: accepted,
            });
          }
        })
      );
    }
  }

  setParentSession(sessionId: string): void {
    for (const removedSessionId of this.owners.retainParent(sessionId)) {
      this.removeSession(removedSessionId);
    }
    this.parentRelays.clear();
    this.parentSessionId = sessionId;
  }

  dispose(): void {
    if (!this.started && !this.parentSessionId) return;
    this.started = false;
    for (const disposable of this.notificationDisposables) {
      disposable.dispose();
    }
    this.notificationDisposables.length = 0;
    this.children.dispose();
    this.owners.clear();
    this.parentRelays.clear();
    this.registeredSessions.clear();
    this.parentSessionId = undefined;
  }

  interceptParentEvent(
    event: AgentStreamEvent,
    meta: KiroMeta | undefined,
    sourceParentSessionId: string
  ): AgentStreamEvent | null {
    let intercepted = event;
    if (
      intercepted.type === AgentEventType.WorkflowProgress &&
      intercepted.event.type !== 'run_snapshot'
    ) {
      const accepted = this.acceptLifecycle(
        intercepted.event,
        sourceParentSessionId
      );
      if (!accepted) return null;
      intercepted = { ...intercepted, event: accepted };
    }
    return this.parentRelays.shouldSuppress(intercepted, meta)
      ? null
      : intercepted;
  }

  routeToolCallChunk(
    originSessionId: string | undefined,
    event: AgentStreamEvent,
    meta: KiroMeta | undefined
  ): boolean {
    const owner = originSessionId
      ? this.owners.ownerForSession(originSessionId)
      : undefined;
    if (owner) {
      this.children.routeExternalEvent(owner, event);
      return true;
    }
    return this.parentRelays.shouldSuppress(event, meta);
  }

  shouldSuppressParentRelay(
    event: AgentStreamEvent,
    meta: KiroMeta | undefined
  ): boolean {
    return this.parentRelays.shouldSuppress(event, meta);
  }

  async sendMessage(
    target: WorkflowNodeSessionTarget,
    content: string
  ): Promise<void> {
    const owner = await this.resolveOwner(target);
    this.activateOwner(owner);
    await this.children.messageNode(owner, content);
  }

  async listRuns(
    workspacePaths: readonly string[]
  ): Promise<WorkflowRunSummary[]> {
    const response = await this.runtime.request(WORKFLOW_LIST_CONTRACT, {
      workspacePaths,
    });
    return response.runs;
  }

  async listRecipes(
    workspacePaths: readonly string[]
  ): Promise<WorkflowRecipeDescriptor[]> {
    const response = await this.runtime.request(
      WORKFLOW_LIST_RECIPES_CONTRACT,
      { workspacePaths }
    );
    return response.recipes;
  }

  createRun(request: WorkflowCreateRequest): Promise<WorkflowCreateResponse> {
    return this.runtime.request(WORKFLOW_CREATE_CONTRACT, request);
  }

  invokeRun(workflowId: string): Promise<WorkflowInvokeResponse> {
    return this.runtime.request(WORKFLOW_INVOKE_CONTRACT, { workflowId });
  }

  async restoreParentRuns(
    workspacePaths: readonly string[]
  ): Promise<WorkflowRestoreSummary> {
    const summary: WorkflowRestoreSummary = {
      restored: 0,
      discovery_failed: 0,
      load_failed: 0,
      rejected: 0,
      _other_: 0,
    };
    const parentSessionId = this.parentSessionId;
    if (!parentSessionId) return summary;

    let runs: WorkflowRunSummary[];
    try {
      runs = await this.listRuns(workspacePaths);
    } catch (error) {
      summary.discovery_failed += 1;
      logger.debug('[acp-client] Workflow restore is unavailable', { error });
      return summary;
    }

    for (const run of runs) {
      if (
        run.parentSessionId !== parentSessionId ||
        (run.status !== 'running' && run.status !== 'paused')
      ) {
        continue;
      }

      let response: WorkflowLoadResponse;
      try {
        response = await this.runtime.request(WORKFLOW_LOAD_CONTRACT, {
          workflowId: run.workflowId,
        });
      } catch (error) {
        summary.load_failed += 1;
        logger.warn('[acp-client] Failed to load active workflow', {
          workflowId: run.workflowId,
          error,
        });
        continue;
      }

      if (
        this.parentSessionId !== parentSessionId ||
        response.workflowId !== run.workflowId ||
        response.state.parentSessionId !== parentSessionId ||
        (response.state.status !== 'running' &&
          response.state.status !== 'paused')
      ) {
        summary.rejected += 1;
        continue;
      }

      try {
        const registration = this.owners.registerLoadedRun(
          response,
          parentSessionId
        );
        if (!registration) {
          summary.rejected += 1;
          continue;
        }
        for (const sessionId of registration.removedSessionIds) {
          this.removeSession(sessionId);
        }
        this.host.emitEffect({
          type: 'workflow_progress',
          event: {
            type: 'run_snapshot',
            workflowId: response.workflowId,
            parentSessionId,
            state: response.state,
            stepSessions: response.stepSessions,
            ...(response.nodePlan === undefined
              ? {}
              : { nodePlan: response.nodePlan }),
          },
        });
        for (const owner of registration.owners) {
          this.activateOwner(owner);
          this.restoreStatus(owner);
          if (isTerminalWorkflowNodeStatus(owner.status)) {
            this.children.disposeLeaseWhenIdle(owner.sessionId);
          }
        }
        summary.restored += 1;
      } catch (error) {
        summary._other_ += 1;
        logger.warn('[acp-client] Failed to restore active workflow', {
          workflowId: run.workflowId,
          error,
        });
      }
    }
    return summary;
  }

  inspectRun(workflowId: string): Promise<WorkflowInspectResponse> {
    return this.runtime.request(WORKFLOW_INSPECT_CONTRACT, { workflowId });
  }

  pauseRun(
    workflowId: string,
    attribution?: WorkflowActionAttribution
  ): Promise<WorkflowPauseResponse> {
    return this.runtime.request(WORKFLOW_PAUSE_CONTRACT, {
      workflowId,
      ...attribution,
    });
  }

  resumeRun(
    workflowId: string,
    attribution?: WorkflowActionAttribution
  ): Promise<WorkflowResumeResponse> {
    return this.runtime.request(WORKFLOW_RESUME_CONTRACT, {
      workflowId,
      ...attribution,
    });
  }

  retryRun(
    workflowId: string,
    nodeId?: string
  ): Promise<WorkflowRetryResponse> {
    return this.runtime.request(WORKFLOW_RETRY_CONTRACT, {
      workflowId,
      nodeId,
    });
  }

  cancelRun(
    workflowId: string,
    targetStatus?: 'aborted' | 'completed',
    attribution?: WorkflowActionAttribution
  ): Promise<WorkflowCancelResponse> {
    return this.runtime.request(WORKFLOW_CANCEL_CONTRACT, {
      workflowId,
      targetStatus,
      ...attribution,
    });
  }

  private acceptLifecycle(
    event: WorkflowEvent,
    sourceParentSessionId?: string
  ): WorkflowEvent | null {
    const ownedEvent =
      !event.parentSessionId && sourceParentSessionId
        ? { ...event, parentSessionId: sourceParentSessionId }
        : event;
    const newSessionId =
      ownedEvent.type === 'node_start' &&
      ownedEvent.sessionId !== undefined &&
      this.owners.ownerForSession(ownedEvent.sessionId) === undefined
        ? ownedEvent.sessionId
        : undefined;
    const result = this.owners.routeLifecycle(ownedEvent, this.parentSessionId);
    if (!result.accepted) {
      logger.warn('[acp-client] Ignored unowned workflow lifecycle event', {
        workflowId: event.workflowId,
        eventType: event.type,
        reason: result.reason,
      });
      return null;
    }

    for (const sessionId of result.removedSessionIds) {
      this.removeSession(sessionId);
    }
    const normalizedEvent =
      ownedEvent.parentSessionId || !result.parentSessionId
        ? ownedEvent
        : { ...ownedEvent, parentSessionId: result.parentSessionId };
    const changedSessionIds = new Set<string>();
    for (const owner of result.changedOwners) {
      changedSessionIds.add(owner.sessionId);
      this.activateOwner(owner, owner.sessionId === newSessionId);
      this.restoreStatus(owner);
    }

    if (
      normalizedEvent.type === 'run_complete' &&
      normalizedEvent.status !== 'paused'
    ) {
      for (const owner of this.owners.ownersForWorkflow(
        normalizedEvent.workflowId
      )) {
        if (!changedSessionIds.has(owner.sessionId)) {
          this.restoreStatus(owner);
        }
        this.children.disposeLeaseWhenIdle(owner.sessionId);
      }
    }
    return normalizedEvent;
  }

  private activateOwner(
    owner: WorkflowSessionOwner,
    hideInitialPrompt = false
  ): void {
    if (!this.isOwnerActive(owner)) {
      throw new Error('Workflow node ownership changed during the operation');
    }
    this.children.ensure(owner, hideInitialPrompt);
    if (!this.registeredSessions.has(owner.sessionId)) {
      this.registeredSessions.add(owner.sessionId);
      this.host.emitEffect({ type: 'child_registered', owner });
    }
  }

  private restoreStatus(owner: WorkflowSessionOwner): void {
    this.host.emitEffect({ type: 'child_status_restored', owner });
  }

  private removeSession(sessionId: string): void {
    this.children.remove(sessionId);
    if (this.registeredSessions.delete(sessionId)) {
      this.host.emitEffect({ type: 'child_removed', sessionId });
    }
  }

  private assertTarget(target: WorkflowNodeSessionTarget): void {
    if (
      !target.workflowId ||
      !target.parentSessionId ||
      !target.nodeId ||
      !target.sessionId
    ) {
      throw new Error('Workflow node target is incomplete');
    }
    if (!this.parentSessionId) {
      throw new Error(
        'Cannot access a workflow node without a primary session'
      );
    }
    if (target.parentSessionId !== this.parentSessionId) {
      throw new Error('Workflow node belongs to a different chat session');
    }
    if (target.sessionId === this.parentSessionId) {
      throw new Error('The primary session cannot be a workflow node');
    }
  }

  private isOwnerActive(owner: WorkflowSessionOwner): boolean {
    return (
      owner.parentSessionId === this.parentSessionId &&
      this.owners.resolveTarget(owner) !== undefined
    );
  }

  private async resolveOwner(
    target: WorkflowNodeSessionTarget
  ): Promise<WorkflowSessionOwner> {
    this.assertTarget(target);
    const knownOwner = this.owners.resolveTarget(target);
    if (knownOwner) return knownOwner;

    const response = await this.runtime.request(WORKFLOW_LOAD_CONTRACT, {
      workflowId: target.workflowId,
    });
    if (response.workflowId !== target.workflowId) {
      throw new Error('Workflow ownership could not be verified');
    }
    const registration = this.owners.registerLoadedRun(
      response,
      this.parentSessionId
    );
    if (registration) {
      for (const sessionId of registration.removedSessionIds) {
        this.removeSession(sessionId);
      }
    }
    const loadedOwner = this.owners.resolveTarget(target);
    if (!loadedOwner) {
      throw new Error('Session is not owned by the requested workflow node');
    }
    return loadedOwner;
  }
}

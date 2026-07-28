import {
  AgentEventType,
  type AgentStreamEvent,
  type KiroMeta,
} from '../../../types/agent-events.js';
import type { WorkflowSessionOwner } from './owner-registry.js';

type WorkflowMessageDelivery = 'prompt' | 'steer';

const INITIAL_PROMPT_BOUNDARIES: ReadonlySet<AgentEventType> = new Set([
  AgentEventType.Content,
  AgentEventType.Thought,
  AgentEventType.ToolCall,
  AgentEventType.ToolCallUpdate,
  AgentEventType.ToolCallFinished,
  AgentEventType.ApprovalRequest,
  AgentEventType.TurnSummary,
  AgentEventType.RateLimitError,
  AgentEventType.RetryWarning,
  AgentEventType.AuthError,
  AgentEventType.SessionError,
]);

const PARENT_RELAY_TYPES: ReadonlySet<AgentEventType> = new Set([
  AgentEventType.UserMessage,
  AgentEventType.Content,
  AgentEventType.Thought,
  AgentEventType.ToolCall,
  AgentEventType.ToolCallUpdate,
  AgentEventType.ToolCallFinished,
]);

/** Per-child prompt echo suppression and in-flight prompt state. */
export class WorkflowChildMessageState {
  private readonly hiddenInitialPrompts = new Set<string>();
  private readonly promptDepths = new Map<string, number>();

  clear(): void {
    this.hiddenInitialPrompts.clear();
    this.promptDepths.clear();
  }

  removeSession(sessionId: string): void {
    this.hiddenInitialPrompts.delete(sessionId);
    this.promptDepths.delete(sessionId);
  }

  hideInitialPrompt(sessionId: string): void {
    this.hiddenInitialPrompts.add(sessionId);
  }

  finishInitialPromptSuppression(sessionId: string): void {
    this.hiddenInitialPrompts.delete(sessionId);
  }

  beginPrompt(sessionId: string): void {
    this.promptDepths.set(
      sessionId,
      (this.promptDepths.get(sessionId) ?? 0) + 1
    );
  }

  endPrompt(sessionId: string): void {
    const depth = this.promptDepths.get(sessionId) ?? 0;
    if (depth <= 1) this.promptDepths.delete(sessionId);
    else this.promptDepths.set(sessionId, depth - 1);
  }

  hasActiveTurn(owner: WorkflowSessionOwner): boolean {
    return (
      owner.status === 'running' ||
      (this.promptDepths.get(owner.sessionId) ?? 0) > 0
    );
  }

  messageDelivery(owner: WorkflowSessionOwner): WorkflowMessageDelivery {
    if (
      owner.status === 'failed' ||
      owner.status === 'aborted' ||
      owner.status === 'skipped' ||
      owner.status === 'pending'
    ) {
      throw new Error(
        `Workflow node cannot receive messages while ${owner.status}`
      );
    }
    return this.hasActiveTurn(owner) ? 'steer' : 'prompt';
  }

  shouldHideInitialPrompt(sessionId: string, event: AgentStreamEvent): boolean {
    if (!this.hiddenInitialPrompts.has(sessionId)) return false;
    if (event.type === AgentEventType.UserMessage) return true;
    if (INITIAL_PROMPT_BOUNDARIES.has(event.type)) {
      this.hiddenInitialPrompts.delete(sessionId);
    }
    return false;
  }
}

/** Correlates workflow-owned relay copies on the active parent stream. */
export class WorkflowParentRelayFilter {
  private readonly parentRelayToolCalls = new Set<string>();

  clear(): void {
    this.parentRelayToolCalls.clear();
  }

  shouldSuppress(event: AgentStreamEvent, meta: KiroMeta | undefined): boolean {
    if (!PARENT_RELAY_TYPES.has(event.type)) return false;
    const hidden =
      meta?.visibility === 'hidden' ||
      (meta?.agentInitiated === true &&
        event.type === AgentEventType.UserMessage);
    const workflowOwned =
      hidden ||
      meta?.workflow !== undefined ||
      meta?.kind === 'workflow-progress' ||
      meta?.notification?.kind === 'workflow-progress' ||
      meta?.notification?.workflowId !== undefined;

    if (event.type === AgentEventType.ToolCall) {
      const tracked = this.parentRelayToolCalls.has(event.id);
      if (workflowOwned) this.parentRelayToolCalls.add(event.id);
      return workflowOwned || tracked;
    }
    if (
      event.type === AgentEventType.ToolCallUpdate ||
      event.type === AgentEventType.ToolCallFinished
    ) {
      const tracked = this.parentRelayToolCalls.has(event.id);
      if (event.type === AgentEventType.ToolCallFinished) {
        this.parentRelayToolCalls.delete(event.id);
      }
      return workflowOwned || tracked;
    }
    return workflowOwned;
  }
}

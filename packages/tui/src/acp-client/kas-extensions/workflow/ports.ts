import * as acp from '@agentclientprotocol/sdk';
import type {
  AgentStreamEvent,
  ApprovalRequestEvent,
} from '../../../types/agent-events.js';
import type { AcpSessionUpdate } from '../../base.js';
import type { WorkflowExtensionEffect } from './effects.js';

/** Host capabilities required by the workflow extension transport. */
export interface WorkflowExtensionHost {
  convertUpdate(
    update: AcpSessionUpdate,
    sessionId: string,
    sideEffectSink: (event: AgentStreamEvent) => void
  ): AgentStreamEvent | null;
  routePermissionRequest(
    request: acp.RequestPermissionRequest,
    sessionId: string,
    eventSink: (event: ApprovalRequestEvent) => void
  ): Promise<acp.RequestPermissionResponse>;
  steerSession(sessionId: string, content: string): Promise<void>;
  emitEffect(effect: WorkflowExtensionEffect): void;
  clearSessionState(sessionId: string): void;
}

import type {
  WorkflowMonitorLayout,
  WorkflowMonitorNode,
} from '../../../types/workflow-monitor.js';
import type { WorkflowStatus } from '../../../types/workflow.js';
import {
  isRetryableWorkflowStatus,
  isTerminalWorkflowStatus,
} from '../../../types/workflow-status.js';
import { messageModeForNode } from './workflow-message-mode.js';
import { retryIsStepScoped } from './workflow-retry-scope.js';

export interface MonitorFooterContext {
  selectedNode?: WorkflowMonitorNode | null;
  status?: WorkflowStatus;
  monitorLayout: WorkflowMonitorLayout;
  mouseModeEnabled: boolean;
  stopConfirmationArmed: boolean;
  inputOpen: boolean;
}

export function buildMonitorFooterHints(ctx: MonitorFooterContext): string {
  if (ctx.stopConfirmationArmed) {
    return 'Ctrl+X stop workflow | Esc keep running';
  }
  if (ctx.inputOpen) return 'Enter send | Up/Down nodes | Esc close';

  const hints: string[] = [];
  const node = ctx.selectedNode;
  // Asking the composer itself, so the footer can't advertise a send it refuses:
  // `s respond` on a container is a dead key, since a message needs a session.
  const messageMode = messageModeForNode(node);
  if (messageMode === 'steer') hints.push('s steer');
  // `respond`/`message` match the composer's own labels.
  if (messageMode === 'respond') hints.push('s respond');
  if (messageMode === 'message') hints.push('s message');

  if (!isTerminalWorkflowStatus(ctx.status)) {
    hints.push(ctx.status === 'paused' ? 'r resume' : 'p pause');
    hints.push('Ctrl+X stop');
  } else if (isRetryableWorkflowStatus(ctx.status)) {
    hints.push(retryIsStepScoped(node) ? 'r retry step' : 'r retry');
  }
  if (node) hints.push('Up/Down nodes');
  hints.push('Left/Right workflows');
  hints.push(ctx.monitorLayout === 'stacked' ? 'l split' : 'l stack');
  hints.push(ctx.mouseModeEnabled ? 'm mouse:on' : 'm mouse:off');
  hints.push('[ ] resize');
  hints.push('Tab agent monitor');
  hints.push('Esc back');
  return hints.join(' | ');
}

export function buildWorkflowNavigationHints(
  workflowCount: number
): string | null {
  return workflowCount < 2 ? null : '1-9 jump';
}

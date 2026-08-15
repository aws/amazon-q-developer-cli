import type { Glyphs } from '../../../utils/glyphs.js';
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
  glyphs: Glyphs;
}

// A single control's key + description, rendered as two colors (key in
// primary, label in secondary) to match the rest of the app's footer hints
// (ApprovalPanel's trust menu, Question, SpecReviewScreen).
export interface MonitorFooterHint {
  key: string;
  label: string;
}

export function buildMonitorFooterHints(
  ctx: MonitorFooterContext
): MonitorFooterHint[] {
  if (ctx.stopConfirmationArmed) {
    return [
      { key: 'ctrl+x', label: 'stop workflow' },
      { key: 'esc', label: 'keep running' },
    ];
  }
  if (ctx.inputOpen) {
    return [
      { key: 'enter', label: 'send' },
      { key: `${ctx.glyphs.arrowUp}${ctx.glyphs.arrowDown}`, label: 'nodes' },
      { key: 'esc', label: 'close' },
    ];
  }

  const hints: MonitorFooterHint[] = [];
  const node = ctx.selectedNode;
  // Asking the composer itself, so the footer can't advertise a send it refuses:
  // `s respond` on a container is a dead key, since a message needs a session.
  const messageMode = messageModeForNode(node);
  if (messageMode === 'steer') hints.push({ key: 's', label: 'steer' });
  // `respond`/`message` match the composer's own labels.
  if (messageMode === 'respond') hints.push({ key: 's', label: 'respond' });
  if (messageMode === 'message') hints.push({ key: 's', label: 'message' });

  if (!isTerminalWorkflowStatus(ctx.status)) {
    hints.push(
      ctx.status === 'paused'
        ? { key: 'r', label: 'resume' }
        : { key: 'p', label: 'pause' }
    );
    hints.push({ key: 'ctrl+x', label: 'stop' });
  } else if (isRetryableWorkflowStatus(ctx.status)) {
    hints.push({
      key: 'r',
      label: retryIsStepScoped(node) ? 'retry step' : 'retry',
    });
  }
  if (node) {
    hints.push({
      key: `${ctx.glyphs.arrowUp}${ctx.glyphs.arrowDown}`,
      label: 'nodes',
    });
  }
  hints.push({
    key: `${ctx.glyphs.arrowLeft}${ctx.glyphs.arrow}`,
    label: 'workflows',
  });
  hints.push({
    key: 'l',
    label: ctx.monitorLayout === 'stacked' ? 'split' : 'stack',
  });
  hints.push({
    key: 'm',
    label: ctx.mouseModeEnabled ? 'mouse:on' : 'mouse:off',
  });
  hints.push({ key: '[ ]', label: 'resize' });
  hints.push({ key: 'tab', label: 'agent monitor' });
  hints.push({ key: 'esc', label: 'back' });
  return hints;
}

export function buildWorkflowNavigationHints(
  workflowCount: number
): string | null {
  return workflowCount < 2 ? null : '1-9 jump';
}

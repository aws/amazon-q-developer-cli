import type {
  WorkflowMonitorLayout,
  WorkflowMonitorNode,
} from '../../../types/workflow-monitor.js';
import type { WorkflowStatus } from '../../../types/workflow.js';
import { isTerminalWorkflowStatus } from '../../../types/workflow-status.js';

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
  if (ctx.inputOpen) return 'Enter send | Esc cancel';

  const hints: string[] = [];
  const node = ctx.selectedNode;
  if (node?.status === 'running') hints.push('s steer');
  if (node?.status === 'paused' && node.completionSignal === 'need_input') {
    hints.push('s respond');
  }
  if (node?.status === 'completed') hints.push('s message');

  if (!isTerminalWorkflowStatus(ctx.status)) {
    hints.push(ctx.status === 'paused' ? 'r resume' : 'p pause');
    hints.push('Ctrl+X stop');
  }
  if (node) hints.push('Up/Down nodes');
  hints.push('Left/Right workflows');
  hints.push(ctx.monitorLayout === 'stacked' ? 'l split' : 'l stack');
  hints.push(ctx.mouseModeEnabled ? 'm mouse:on' : 'm mouse:off');
  hints.push('[ ] resize');
  hints.push('Tab agents');
  hints.push('Esc back');
  return hints.join(' | ');
}

export function buildWorkflowNavigationHints(
  workflowCount: number
): string | null {
  return workflowCount < 2 ? null : '1-9 jump';
}

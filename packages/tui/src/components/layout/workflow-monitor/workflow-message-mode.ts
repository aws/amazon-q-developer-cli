import type { WorkflowMonitorNode } from '../../../types/workflow-monitor.js';
import type { WorkflowMessageMode } from './WorkflowMessageComposer.js';

/**
 * Which composer a node can accept, if any. `paused` is deliberately not gated on
 * `need_input`: KAS parks interactive steps with no such signal, so gating left
 * the user refused a message at the very step waiting on them. `failed` accepts
 * one because fixing a step is chat-then-retry, and this is motion one.
 *
 * Steps with a session only, because a message is addressed to a session:
 * `buildWorkflowNodeConversation` returns null for anything else, so a container
 * offered a reply hits "This workflow step cannot receive a message." The footer
 * shares this so it never advertises a motion the composer refuses.
 */
export function messageModeForNode(
  node: WorkflowMonitorNode | null | undefined
): WorkflowMessageMode | null {
  if (node?.type !== 'step' || node.sessionId === undefined) return null;
  if (node.status === 'running') return 'steer';
  if (node.status === 'paused') return 'respond';
  if (node.status === 'completed' || node.status === 'failed') {
    return 'message';
  }
  return null;
}

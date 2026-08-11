import type { WorkflowRunView } from '../../../types/workflow-monitor.js';

/**
 * The one-line banner explaining why a run isn't moving. `pauseReason` is
 * mechanical ("paused before node 'review'"), while a deliberate stop carries
 * `stopInitiator: 'user'`. Leading with the deliberate stop keeps the banner from
 * implying something went wrong when the user just pressed Ctrl+X.
 *
 * A running run gets no notice: KAS keeps `stopInitiator` on the record across a
 * later resume, and only `node_start` clears it, so a snapshot restoring a live
 * run would otherwise caption "Stopped by you." over one visibly moving.
 */
export function runStopNotice(
  run: Pick<
    WorkflowRunView,
    'status' | 'pauseReason' | 'stopInitiator' | 'stopReason'
  >
): string | null {
  if (run.status === 'running') return null;
  if (run.stopInitiator === 'user') {
    const reason = run.stopReason?.trim();
    return reason ? `Stopped by you: ${reason}` : 'Stopped by you.';
  }
  const pauseReason = run.pauseReason?.trim();
  return pauseReason ? pauseReason : null;
}

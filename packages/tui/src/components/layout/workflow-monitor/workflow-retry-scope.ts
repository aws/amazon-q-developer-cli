import type { WorkflowMonitorNode } from '../../../types/workflow-monitor.js';

/**
 * Whether `r` will retry just this step or fall back to the whole run.
 *
 * Steps only, and not inside a loop. `_kiro/workflow/retry` carries a bare
 * `nodeId` and KAS resolves it with a first-DFS walk, so every iteration answers
 * to the same id; and a container is not a unit of work to rerun — retrying one
 * means rerunning whatever it holds, which is the whole-run path anyway.
 * Rerunning the run is recoverable; rerunning a different iteration than the one
 * on screen is not. Shared so the footer hint, the alert text, and the request
 * itself cannot disagree about the scope.
 */
export function retryIsStepScoped(
  node: WorkflowMonitorNode | null | undefined
): boolean {
  return (
    node?.type === 'step' &&
    node.status === 'failed' &&
    node.iteration === undefined
  );
}

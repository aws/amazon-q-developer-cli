import type { Glyphs } from '../../../utils/glyphs.js';
import type { WorkflowStatus } from '../../../types/workflow.js';

export const RUN_STATUS_COLOR_TOKEN: Record<WorkflowStatus, string> = {
  running: 'info',
  paused: 'warning',
  completed: 'success',
  failed: 'error',
  aborted: 'error',
};

export function runStatusGlyph(status: WorkflowStatus, glyphs: Glyphs): string {
  switch (status) {
    case 'running':
      return glyphs.triangleRight;
    case 'paused':
      return glyphs.pause;
    case 'completed':
      return glyphs.checkmark;
    case 'failed':
    case 'aborted':
      return glyphs.cross;
  }
}

export function runStatusLabel(
  status: WorkflowStatus,
  pauseRequested: boolean
): string {
  return pauseRequested && status === 'running' ? 'pausing...' : status;
}

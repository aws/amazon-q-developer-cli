import type { Glyphs } from '../../utils/glyphs.js';
import { InterruptMode } from '../../constants/interrupt-mode.js';

export function getPlaceholder(opts: {
  glyphs: Glyphs;
  editingQueueIndex: number | null;
  pendingApproval: boolean;
  isShellEscape: boolean;
  isProcessing: boolean;
  isInitialized: boolean;
  pendingSteerContent: string | null;
  activeInterruptMode: InterruptMode;
  toggleHintLabel: string;
  agentName: string | undefined;
  tangentName?: string | null;
  /** Feature name of the pending /spec new description-collection step. */
  specDescriptionFeature?: string | null;
  goalStatus?: {
    state: string;
    iteration: number;
    maxIterations: number;
    message?: string;
  } | null;
  cancelLabel?: string;
}): string {
  const dot = opts.glyphs.smallDot;
  if (opts.editingQueueIndex != null) {
    return `Editing queued message ${opts.editingQueueIndex + 1} ${dot} esc to cancel`;
  }
  if (!opts.isInitialized) {
    return opts.pendingSteerContent != null
      ? `Initializing ${dot} type to queue another message`
      : `Initializing ${dot} type to queue a message`;
  }
  if (opts.goalStatus && opts.goalStatus.state === 'active') {
    const desc =
      opts.goalStatus.message && opts.goalStatus.message.length > 50
        ? opts.goalStatus.message.slice(0, 47) + '...'
        : (opts.goalStatus.message ?? 'Running');
    const cancel = opts.cancelLabel ?? 'Ctrl+C';
    return `Goal Active: ${desc} ${dot} Iteration ${opts.goalStatus.iteration + 1}/${opts.goalStatus.maxIterations} ${dot} ${cancel} to pause`;
  }
  if (opts.pendingApproval || opts.isProcessing) {
    if (opts.activeInterruptMode === InterruptMode.STEER) {
      return `Kiro is working ${dot} Type to steer ${dot} ${opts.toggleHintLabel} to queue`;
    }
    return `Kiro is working ${dot} Type to queue ${dot} ${opts.toggleHintLabel} to steer`;
  }
  if (opts.specDescriptionFeature) {
    return `describe what "${opts.specDescriptionFeature}" should do ${dot} esc to cancel`;
  }
  if (opts.isShellEscape) {
    return `running shell command ${dot} ctrl+c to cancel`;
  }
  if (opts.agentName === 'kiro_planner') {
    return `ask a question or describe a task ${opts.glyphs.enter}  ${dot}  exit plan mode: shift+tab`;
  }
  if (opts.tangentName) {
    return `ask a question or describe a task ${dot} /tangent to go back ${dot} /tangent ls to view`;
  }
  return `ask a question or describe a task ${opts.glyphs.enter}`;
}

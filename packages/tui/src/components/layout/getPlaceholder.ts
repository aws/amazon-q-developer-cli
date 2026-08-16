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
  /** The last Ctrl+C goal cancel failed — the quit key exits instead. */
  goalCancelFailed?: boolean;
  cancelLabel?: string;
  /** Label of the quit binding, which cancels a paused goal. */
  quitLabel?: string;
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
  if (
    opts.goalStatus &&
    (opts.goalStatus.state === 'active' || opts.goalStatus.state === 'paused')
  ) {
    const desc =
      opts.goalStatus.message && opts.goalStatus.message.length > 50
        ? opts.goalStatus.message.slice(0, 47) + '...'
        : (opts.goalStatus.message ?? 'Running');
    const cancel = opts.cancelLabel ?? 'Ctrl+C';
    const iter = `Iteration ${opts.goalStatus.iteration + 1}/${opts.goalStatus.maxIterations}`;
    if (
      opts.goalStatus.state === 'active' &&
      (opts.isProcessing || opts.pendingApproval)
    ) {
      return `Goal Active: ${desc} ${dot} ${iter} ${dot} ${cancel} to pause`;
    }
    // Turn interrupted but the goal is still set — it resumes on the next
    // prompt. The quit key cancels it outright, unless a previous cancel
    // already failed (the key falls through to process exit then, so point
    // at the slash command instead).
    const cancelHint = opts.goalCancelFailed
      ? '/goal clear to cancel'
      : `${opts.quitLabel ?? 'Ctrl+C'} to cancel`;
    return `Goal Paused: ${desc} ${dot} ${iter} ${dot} type to resume ${dot} ${cancelHint}`;
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

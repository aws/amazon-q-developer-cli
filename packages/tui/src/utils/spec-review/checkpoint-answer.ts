/**
 * What answering a spec phase checkpoint should do, given the comments staged
 * against its document.
 *
 * Kept apart from the layout that renders the question so the refusal — the
 * only thing standing between staged comments and being silently discarded —
 * can be tested without a terminal.
 */

import {
  commentCount,
  composeRevisionRequest,
  type ReviewAction,
} from './review-actions.js';

/** Just enough of a checkpoint to route an answer. */
export interface AnsweringCheckpoint {
  phase: string;
  comments: readonly ReviewAction[];
}

export type CheckpointAnswer =
  /** Send the comments as the answer the agent sees. */
  | { kind: 'send'; answerForAgent: string }
  /** Refuse: advancing would drop comments with nothing left to send them to. */
  | { kind: 'refuse'; message: string }
  /** Nothing staged, or no checkpoint — answer as any other question. */
  | { kind: 'pass' };

/**
 * The extra option the checkpoint offers while comments are staged, or null
 * when there is nothing to send.
 */
export function stagedCommentsOption(
  checkpoint: AnsweringCheckpoint | null
): string | null {
  const staged = checkpoint?.comments.length ?? 0;
  return staged > 0 ? `Send ${commentCount(staged)} and revise` : null;
}

export function routeCheckpointAnswer(
  checkpoint: AnsweringCheckpoint | null,
  answer: string
): CheckpointAnswer {
  const option = stagedCommentsOption(checkpoint);
  if (!option || !checkpoint) return { kind: 'pass' };
  if (answer === option) {
    return {
      kind: 'send',
      answerForAgent: composeRevisionRequest(
        `${checkpoint.phase}.md`,
        checkpoint.comments
      ),
    };
  }
  return {
    kind: 'refuse',
    message: `${commentCount(
      checkpoint.comments.length
    )} staged — pick "${option}" to send, or ctrl+X to review and remove`,
  };
}

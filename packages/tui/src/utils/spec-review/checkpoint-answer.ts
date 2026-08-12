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

/** Just enough of a staged review to route an answer. */
export interface AnsweringReview {
  document: string;
  comments: readonly ReviewAction[];
}

export type CheckpointAnswer =
  /** Send the comments as the answer the agent sees. */
  | { kind: 'send'; answerForAgent: string }
  /** Refuse: advancing would drop comments with nothing left to send them to. */
  | { kind: 'refuse'; message: string }
  /** Nothing staged for this document — answer as any other question. */
  | { kind: 'pass' };

/**
 * The extra option the checkpoint offers while comments are staged, or null
 * when there is nothing to send.
 */
export function stagedCommentsOption(
  review: AnsweringReview | null
): string | null {
  const staged = review?.comments.length ?? 0;
  return staged > 0 ? `Send ${commentCount(staged)} and revise` : null;
}

export function routeCheckpointAnswer(
  review: AnsweringReview | null,
  answer: string
): CheckpointAnswer {
  const option = stagedCommentsOption(review);
  if (!option || !review) return { kind: 'pass' };
  if (answer === option) {
    return {
      kind: 'send',
      answerForAgent: composeRevisionRequest(
        `${review.document}.md`,
        review.comments
      ),
    };
  }
  return {
    kind: 'refuse',
    message: `${commentCount(
      review.comments.length
    )} staged — pick "${option}" to send, or review and remove them first`,
  };
}

import {
  composeRevisionRequest,
  summarizeRevision,
  commentCount,
  type ReviewAction,
} from './review-actions.js';

/** Whether the agent can take a message right now, and why not if it can't. */
export interface SendReadiness {
  /** A turn is in flight, a compaction is running, or the session isn't up. */
  busy: boolean;
}

export type SendDecision =
  /** `request` goes to the agent; `summary` stands in for it in the transcript. */
  | { kind: 'send'; document: string; request: string; summary: string }
  /** Nothing to send. */
  | { kind: 'nothing' }
  /** Refuse, keeping the comments staged, and say why. */
  | { kind: 'refuse'; message: string };

/**
 * What pressing send on a document's staged comments should do.
 *
 * Refusing while the agent is busy is not politeness: a message sent then is
 * queued as the text the transcript shows rather than the tagged request, so the
 * quotes and the instruction to revise would be dropped while the send still
 * looked like it succeeded. Comments are hand-typed, so the only safe move is to
 * keep them and say so.
 */
export function decideSend(
  document: string,
  comments: readonly ReviewAction[],
  readiness: SendReadiness
): SendDecision {
  if (comments.length === 0) return { kind: 'nothing' };
  if (readiness.busy) {
    return {
      kind: 'refuse',
      message: `${commentCount(
        comments.length
      )} still staged — wait for the current turn to finish, then press S again`,
    };
  }
  const documentName = `${document}.md`;
  return {
    kind: 'send',
    document,
    request: composeRevisionRequest(documentName, comments),
    summary: summarizeRevision(documentName, comments),
  };
}

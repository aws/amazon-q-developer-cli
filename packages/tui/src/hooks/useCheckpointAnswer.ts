/**
 * Shared checkpoint-answer routing for both Lite and Inline layouts.
 *
 * A checkpoint can send staged comments alongside its answer. This hook
 * provides the extra option to offer and the routing that respects staged
 * comments — refusing to advance the phase while they are staged unless the
 * user picks the send option.
 *
 * Without this, a layout that calls `respondToQuestion` directly bypasses the
 * refusal and silently deletes comments via `withoutStagedComments`.
 */

import { useAppStore, commentsForCheckpoint } from '../stores/app-store.js';
import {
  stagedCommentsOption,
  routeCheckpointAnswer,
} from '../utils/spec-review/checkpoint-answer.js';
import type { QuestionRequestInfo } from '../types/agent-events.js';
import type { UserInputOption } from '@kiro/acp-type-covenant';

export interface CheckpointAnswerHook {
  /** Prepend the staged option to a list of existing options. */
  checkpointOptions: (options: UserInputOption[]) => UserInputOption[];
  /**
   * Route an answer through the checkpoint guard. Returns true if the question
   * was answered (by whatever path), false if it was refused.
   */
  answerCheckpoint: (
    answer: string,
    answerForAgent: string | undefined,
    question: QuestionRequestInfo
  ) => boolean;
}

export function useCheckpointAnswer(): CheckpointAnswerHook {
  const checkpointDocument = useAppStore(
    (state) => state.specPhaseCheckpoint?.phase ?? null
  );
  const checkpointComments = useAppStore(commentsForCheckpoint);
  const respondToQuestion = useAppStore((s) => s.respondToQuestion);
  const showTransientAlert = useAppStore((s) => s.showTransientAlert);

  const checkpointReview =
    checkpointDocument && checkpointComments.length > 0
      ? { document: checkpointDocument, comments: checkpointComments }
      : null;
  const stagedOption = stagedCommentsOption(checkpointReview);

  const checkpointOptions = (options: UserInputOption[]): UserInputOption[] =>
    stagedOption ? [{ title: stagedOption }, ...options] : options;

  const answerCheckpoint = (
    answer: string,
    answerForAgent: string | undefined,
    question: QuestionRequestInfo
  ): boolean => {
    const route = routeCheckpointAnswer(checkpointReview, answer);
    if (route.kind === 'send') {
      return respondToQuestion(answer, question, route.answerForAgent);
    }
    if (route.kind === 'refuse') {
      showTransientAlert({
        message: route.message,
        status: 'warning',
        autoHideMs: 5000,
      });
      return false;
    }
    return respondToQuestion(answer, question, answerForAgent);
  };

  return { checkpointOptions, answerCheckpoint };
}

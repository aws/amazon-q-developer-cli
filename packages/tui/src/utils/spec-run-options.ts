/**
 * The task-run choices a spec checkpoint offers after the tasks phase.
 *
 * The agent asks the question, but running the tasks is the client's to carry
 * out: the answer would otherwise return into a turn that has no way to start a
 * run. These titles are what the agent offers, so an answer is matched against
 * them exactly — a title that has drifted leaves the option inert, which is the
 * behaviour before any of this existed, rather than running something the user
 * didn't ask for.
 */

/** Runs the tasks marked required, leaving optional ones alone. */
export const RUN_REQUIRED_TASKS = 'Run required tasks';
/** Promotes every optional task to required first, then runs them all. */
export const RUN_REQUIRED_AND_OPTIONAL_TASKS =
  'Run required and optional tasks';

/**
 * The run an answer asks for, or null when the answer is anything else —
 * declining, or free-text feedback.
 */
export function runFromAnswer(
  answer: string
): { makeAllRequired: boolean } | null {
  const trimmed = answer.trim();
  if (trimmed === RUN_REQUIRED_TASKS) return { makeAllRequired: false };
  if (trimmed === RUN_REQUIRED_AND_OPTIONAL_TASKS) {
    return { makeAllRequired: true };
  }
  return null;
}

/** Exact task-scope choices accepted from a tasks-phase confirmation. */

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

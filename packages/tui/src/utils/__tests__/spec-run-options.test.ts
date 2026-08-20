/** Tests task-scope parsing for the explicit `/spec run` confirmation. */
import { describe, it, expect } from 'bun:test';
import {
  RUN_REQUIRED_AND_OPTIONAL_TASKS,
  RUN_REQUIRED_TASKS,
  runFromAnswer,
} from '../spec-run-options';

describe('runFromAnswer', () => {
  it('reads the required-only run', () => {
    expect(runFromAnswer(RUN_REQUIRED_TASKS)).toEqual({
      makeAllRequired: false,
    });
  });

  it('reads the run that promotes optional tasks', () => {
    expect(runFromAnswer(RUN_REQUIRED_AND_OPTIONAL_TASKS)).toEqual({
      makeAllRequired: true,
    });
  });

  it('tolerates surrounding whitespace', () => {
    expect(runFromAnswer(`  ${RUN_REQUIRED_TASKS}\n`)).toEqual({
      makeAllRequired: false,
    });
  });

  it('reports nothing for declining or for feedback', () => {
    expect(runFromAnswer('Not now')).toBeNull();
    expect(runFromAnswer('split task 3 into two')).toBeNull();
    expect(runFromAnswer('')).toBeNull();
  });

  it('reports nothing for a title that only resembles a run', () => {
    // Substring matching would promote every optional task on a near miss.
    expect(runFromAnswer('Run required tasks now')).toBeNull();
    expect(runFromAnswer('run required tasks')).toBeNull();
  });
});

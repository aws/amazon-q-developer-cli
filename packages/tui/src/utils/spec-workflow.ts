/**
 * Pure helper: given a workflow type, return the ordered list of
 * artifact stages the user is expected to move through.
 *
 * The order is the same set in both workflows; only the position of
 * `requirements` vs `design` differs. `tasks` is always last.
 */

import type { ArtifactKind } from './spec-artifact-loader.js';
import type { WorkflowType } from './spec-config.js';

const REQUIREMENTS_FIRST: readonly ArtifactKind[] = [
  'requirements',
  'design',
  'tasks',
] as const;

const DESIGN_FIRST: readonly ArtifactKind[] = [
  'design',
  'requirements',
  'tasks',
] as const;

export function workflowStages(
  workflowType: WorkflowType
): readonly ArtifactKind[] {
  switch (workflowType) {
    case 'design-first':
      return DESIGN_FIRST;
    case 'requirements-first':
      return REQUIREMENTS_FIRST;
  }
}

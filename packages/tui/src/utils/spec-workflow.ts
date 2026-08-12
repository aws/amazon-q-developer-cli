/**
 * Pure helper: given a spec's config, return the ordered list of artifact stages
 * the user is expected to move through.
 *
 * A bugfix spec opens with `bugfix.md` in place of requirements; the two feature
 * workflows differ only in whether requirements or design comes first, and tasks
 * is always last.
 */

import type { ArtifactKind } from './spec-artifact-loader.js';
import type { SpecType, WorkflowType } from './spec-config.js';

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

const BUGFIX: readonly ArtifactKind[] = ['bugfix', 'design', 'tasks'] as const;

export function workflowStages(
  workflowType: WorkflowType,
  specType: SpecType = 'feature'
): readonly ArtifactKind[] {
  if (specType === 'bugfix') return BUGFIX;
  switch (workflowType) {
    case 'design-first':
      return DESIGN_FIRST;
    case 'requirements-first':
      return REQUIREMENTS_FIRST;
  }
}

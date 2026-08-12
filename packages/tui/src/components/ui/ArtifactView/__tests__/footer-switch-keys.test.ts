import { describe, expect, it } from 'bun:test';
import { switchKeyFor } from '../useArtifactKeybinds.js';
import { workflowStages } from '../../../../utils/spec-workflow.js';

/** What the panel footer advertises after `switch`. */
const advertised = (
  workflowType: 'requirements-first' | 'design-first',
  specType: 'feature' | 'bugfix'
) => workflowStages(workflowType, specType).map(switchKeyFor).join('/');

describe('the switch keys the footer advertises', () => {
  it('names only the documents this spec writes', () => {
    // A bugfix spec has no requirements.md to switch to, so offering R would
    // point at a document its workflow never writes.
    expect(advertised('requirements-first', 'bugfix')).toBe('B/D/T');
    expect(advertised('requirements-first', 'feature')).toBe('R/D/T');
  });

  it('follows the order the stage bar shows', () => {
    // Previously hardcoded as R/D/T, which read wrong under design-first even
    // for a feature spec.
    expect(advertised('design-first', 'feature')).toBe('D/R/T');
  });

  it('has a key for every stage it can show', () => {
    for (const workflowType of [
      'requirements-first',
      'design-first',
    ] as const) {
      for (const specType of ['feature', 'bugfix'] as const) {
        for (const stage of workflowStages(workflowType, specType)) {
          expect(switchKeyFor(stage), stage).not.toBe('');
        }
      }
    }
  });
});

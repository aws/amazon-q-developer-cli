import { describe, expect, it } from 'bun:test';
import { workflowStages } from '../spec-workflow.js';

describe('workflowStages', () => {
  it('orders the feature documents by workflow type', () => {
    expect(workflowStages('requirements-first')).toEqual([
      'requirements',
      'design',
      'tasks',
    ]);
    expect(workflowStages('design-first')).toEqual([
      'design',
      'requirements',
      'tasks',
    ]);
  });

  it('opens a bugfix spec with bugfix in place of requirements', () => {
    expect(workflowStages('requirements-first', 'bugfix')).toEqual([
      'bugfix',
      'design',
      'tasks',
    ]);
  });

  it('ignores the workflow type for a bugfix spec, which has no requirements to order', () => {
    expect(workflowStages('design-first', 'bugfix')).toEqual(
      workflowStages('requirements-first', 'bugfix')
    );
  });

  it('treats an unstated spec type as a feature', () => {
    expect(workflowStages('requirements-first')).toEqual(
      workflowStages('requirements-first', 'feature')
    );
  });
});

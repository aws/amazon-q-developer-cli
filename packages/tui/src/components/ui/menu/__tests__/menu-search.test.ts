import { describe, it, expect } from 'bun:test';
import { rankMenuItems } from '../menu-search';
import type { MenuItem } from '../Menu';

const item = (label: string, description = ''): MenuItem => ({
  label,
  description,
});

describe('rankMenuItems', () => {
  it('ranks a label substring match above description-only matches', () => {
    // Long descriptions contain almost any short subsequence; the item whose
    // name literally contains the query must still rank first.
    const items = [
      item(
        '/fix-deployment-failure',
        'Prompt to fix failed pipeline deployments and resolve deployment issues'
      ),
      item(
        '/agent-sop:eval',
        'EvalKit is a conversational evaluation framework for AI agents that guides you through creating robust evaluations using the Strands Evals SDK.'
      ),
      item('/agent-sop:pdd'),
    ];
    const result = rankMenuItems(items, 'pdd');
    expect(result[0]!.label).toBe('/agent-sop:pdd');
  });

  it('ranks label prefix above label substring', () => {
    const items = [item('/agent-sop:pdd'), item('/pdd-quick')];
    expect(rankMenuItems(items, 'pdd').map((i) => i.label)).toEqual([
      '/pdd-quick',
      '/agent-sop:pdd',
    ]);
  });

  it('ignores a display-only leading slash for prefix ranking', () => {
    const items = [item('/other-plan-thing'), item('/plan')];
    expect(rankMenuItems(items, 'plan')[0]!.label).toBe('/plan');
  });

  it('still surfaces fuzzy label and description matches below', () => {
    const items = [
      item('/deploy', 'Ship it'),
      item('/other', 'a scattered d-e-p match lives here: dizzy elephant pony'),
    ];
    const result = rankMenuItems(items, 'dep');
    expect(result.map((i) => i.label)).toEqual(['/deploy', '/other']);
  });

  it('drops items matching neither label nor description', () => {
    const items = [item('/save', 'Save session')];
    expect(rankMenuItems(items, 'xyz')).toEqual([]);
  });

  it('is case-insensitive', () => {
    const items = [item('/Agent-SOP:PDD')];
    expect(rankMenuItems(items, 'pdd')).toHaveLength(1);
  });
});

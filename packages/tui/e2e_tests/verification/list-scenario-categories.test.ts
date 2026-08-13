import { describe, expect, it } from 'bun:test';
import { listScenarioCategories } from './list-scenario-categories';

describe('listScenarioCategories', () => {
  const scenarios = [
    { id: 'boot', category: 'basic', engine: ['v2', 'kas'] as const },
    {
      id: 'help',
      category: 'slash-commands',
      engine: ['kas'] as const,
      backend: ['acp-mock'] as const,
    },
    {
      id: 'goal',
      category: 'slash-commands',
      engine: ['v2'] as const,
      tags: ['smoke'] as const,
      priority: 'p0' as const,
    },
    {
      id: 'tool',
      category: 'tool-use',
      backend: ['live'] as const,
      tags: ['workflow'] as const,
      priority: 'p1' as const,
    },
  ];

  it('returns unique sorted categories', () => {
    expect(listScenarioCategories(scenarios)).toEqual([
      'basic',
      'slash-commands',
      'tool-use',
    ]);
  });

  it('filters by backend and engine compatibility', () => {
    expect(
      listScenarioCategories(scenarios, {
        backend: 'acp-mock',
        engine: 'kas',
      })
    ).toEqual(['basic', 'slash-commands']);
  });

  it('filters by priority and category selection', () => {
    expect(
      listScenarioCategories(scenarios, {
        priorities: ['p0'],
        categories: ['slash-commands', 'tool-use'],
      })
    ).toEqual(['slash-commands']);
  });

  it('filters by tag selection', () => {
    expect(
      listScenarioCategories(scenarios, {
        tags: ['workflow'],
      })
    ).toEqual(['tool-use']);
  });
});

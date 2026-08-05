import { describe, it, expect } from 'bun:test';
import {
  filterPromptsByQuery,
  filterSlashMenuCommands,
  buildAtMenuItems,
  findPromptByMenuLabel,
  atMenuShowsPrompts,
  commandMatchesQuery,
  commandMatchRank,
  isPromptMenuOpen,
} from './command-menu-utils';
import type { SlashCommand } from '../../stores/app-store';

const makePrompt = (
  name: string,
  desc = '',
  args?: { name: string; required?: boolean }[]
): SlashCommand => ({
  name,
  description: desc,
  source: 'backend',
  meta: { type: 'prompt', arguments: args },
});

const makeCommand = (name: string, desc = ''): SlashCommand => ({
  name,
  description: desc,
  source: 'backend',
  meta: { type: 'action' },
});

const commands: SlashCommand[] = [
  makePrompt('/research', 'Research codebase'),
  makePrompt('/plan', 'Create plan'),
  makeCommand('/save', 'Save session'),
  makePrompt('/review', 'Review code'),
];

describe('filterPromptsByQuery', () => {
  it('returns empty for empty query', () => {
    expect(filterPromptsByQuery(commands, '')).toEqual([]);
  });

  it('filters prompts matching prefix, case-insensitive', () => {
    const result = filterPromptsByQuery(commands, 'Re');
    expect(result.map((c) => c.name)).toEqual(['/research', '/review']);
  });

  it('excludes non-prompt commands', () => {
    expect(filterPromptsByQuery(commands, 'sa')).toEqual([]);
  });

  it('matches exact prompt name', () => {
    const result = filterPromptsByQuery(commands, 'plan');
    expect(result).toHaveLength(1);
    expect(result[0]!.name).toBe('/plan');
  });

  it('returns empty when nothing matches', () => {
    expect(filterPromptsByQuery(commands, 'xyz')).toEqual([]);
  });

  it('skips commands with no meta', () => {
    const cmds: SlashCommand[] = [
      { name: '/bare', description: '', source: 'backend' },
    ];
    expect(filterPromptsByQuery(cmds, 'bare')).toEqual([]);
  });
});

describe('commandMatchesQuery', () => {
  it('matches a prefix', () => {
    expect(commandMatchesQuery('/compact', 'co')).toBe(true);
  });

  it('matches a substring after a namespace prefix', () => {
    expect(commandMatchesQuery('/agent-sop:pdd', 'pdd')).toBe(true);
  });

  it('is case-insensitive', () => {
    expect(commandMatchesQuery('/agent-sop:PDD', 'pdd')).toBe(true);
    expect(commandMatchesQuery('/agent-sop:pdd', 'PDD')).toBe(true);
  });

  it('matches everything on empty query', () => {
    expect(commandMatchesQuery('/help', '')).toBe(true);
  });

  it('rejects non-matches', () => {
    expect(commandMatchesQuery('/help', 'pdd')).toBe(false);
  });
});

describe('commandMatchRank', () => {
  it('ranks prefix matches before substring matches', () => {
    expect(commandMatchRank('/pdd-tool', 'pdd')).toBe(0);
    expect(commandMatchRank('/agent-sop:pdd', 'pdd')).toBe(1);
  });
});

describe('filterPromptsByQuery substring matching', () => {
  const namespaced: SlashCommand[] = [
    makePrompt('/agent-sop:pdd', 'PDD workflow'),
    makePrompt('/pdd-quick', 'Quick PDD'),
    makePrompt('/plan', 'Create plan'),
  ];

  it('finds namespaced prompts by suffix', () => {
    const result = filterPromptsByQuery(namespaced, 'pdd');
    expect(result.map((c) => c.name)).toEqual(['/pdd-quick', '/agent-sop:pdd']);
  });

  it('orders prefix matches before substring matches', () => {
    const result = filterPromptsByQuery(namespaced, 'pdd');
    expect(result[0]!.name).toBe('/pdd-quick');
  });
});

describe('filterSlashMenuCommands', () => {
  it('ranks a prefix-matched prompt above a substring-matched command', () => {
    // A substring-only command must not steal the top slot (and Enter/Tab)
    // from a prompt that was the sole match under prefix-only filtering.
    const cmds: SlashCommand[] = [
      makeCommand('/context', 'Manage context'),
      makePrompt('/text-review', 'Review text'),
    ];
    expect(filterSlashMenuCommands(cmds, 'text').map((c) => c.name)).toEqual([
      '/text-review',
      '/context',
    ]);
  });

  it('sorts commands before prompts within the same rank', () => {
    const cmds: SlashCommand[] = [
      makePrompt('/plan', 'Create plan'),
      makeCommand('/planner', 'Planner command'),
    ];
    expect(filterSlashMenuCommands(cmds, 'plan').map((c) => c.name)).toEqual([
      '/planner',
      '/plan',
    ]);
  });

  it('sorts alphabetically within the same rank and group', () => {
    const cmds: SlashCommand[] = [
      makeCommand('/save', 'Save'),
      makeCommand('/sandbox', 'Sandbox'),
    ];
    expect(filterSlashMenuCommands(cmds, 'sa').map((c) => c.name)).toEqual([
      '/sandbox',
      '/save',
    ]);
  });

  it('excludes hidden non-prompt commands', () => {
    const cmds: SlashCommand[] = [
      {
        name: '/secret',
        description: '',
        source: 'backend',
        meta: { type: 'action', hidden: true },
      },
    ];
    expect(filterSlashMenuCommands(cmds, 'sec')).toEqual([]);
  });

  it('keeps the same top item as prefix-only filtering for prefix queries', () => {
    const cmds: SlashCommand[] = [
      makeCommand('/compact', 'Compact'),
      makeCommand('/context', 'Context'),
      makePrompt('/agent-sop:composer', 'Composer'),
    ];
    expect(filterSlashMenuCommands(cmds, 'co')[0]!.name).toBe('/compact');
  });
});

describe('filterPromptsByQuery ordering', () => {
  it('sorts same-rank prompts alphabetically, not by registration order', () => {
    const cmds: SlashCommand[] = [
      makePrompt('/zeta-review', 'Z'),
      makePrompt('/alpha-review', 'A'),
    ];
    expect(filterPromptsByQuery(cmds, 'review').map((c) => c.name)).toEqual([
      '/alpha-review',
      '/zeta-review',
    ]);
  });
});

describe('buildAtMenuItems', () => {
  it('puts prompts before files', () => {
    const items = buildAtMenuItems(
      [makePrompt('/research', 'Research')],
      ['src/index.ts']
    );
    expect(items).toEqual([
      { label: 'research', description: 'Research', group: 'Prompt' },
      { label: 'src/index.ts', description: '' },
    ]);
  });

  it('returns only files when no prompts', () => {
    const items = buildAtMenuItems([], ['file.txt']);
    expect(items).toEqual([{ label: 'file.txt', description: '' }]);
  });

  it('returns only prompts when no files', () => {
    const items = buildAtMenuItems([makePrompt('/plan', 'Plan')], []);
    expect(items).toEqual([
      { label: 'plan', description: 'Plan', group: 'Prompt' },
    ]);
  });

  it('returns empty when nothing matches', () => {
    expect(buildAtMenuItems([], [])).toEqual([]);
  });

  it('strips leading slash from prompt names', () => {
    const items = buildAtMenuItems([makePrompt('/research', '')], []);
    expect(items[0]!.label).toBe('research');
  });
});

describe('findPromptByMenuLabel', () => {
  const prompts: SlashCommand[] = [
    makePrompt('/agent-sop:code-assist', 'Code assist', [
      { name: 'task', required: true },
    ]),
    makePrompt('/explain-tools', 'Explain tools'),
    makeCommand('/save', 'Save session'),
  ];

  it('finds prompt when label already has leading slash', () => {
    const result = findPromptByMenuLabel(prompts, '/agent-sop:code-assist');
    expect(result?.name).toBe('/agent-sop:code-assist');
  });

  it('finds prompt when label has no leading slash', () => {
    const result = findPromptByMenuLabel(prompts, 'explain-tools');
    expect(result?.name).toBe('/explain-tools');
  });

  it('returns undefined for non-existent label', () => {
    expect(findPromptByMenuLabel(prompts, 'nonexistent')).toBeUndefined();
  });

  it('ignores non-prompt commands', () => {
    expect(findPromptByMenuLabel(prompts, '/save')).toBeUndefined();
  });

  it('handles empty commands list', () => {
    expect(findPromptByMenuLabel([], 'anything')).toBeUndefined();
  });
});

describe('atMenuShowsPrompts', () => {
  const atTrigger = (position: number) => ({ key: '@', position });

  it('is true when the text after @ prefix-matches a prompt', () => {
    expect(atMenuShowsPrompts(commands, '@rese', atTrigger(0))).toBe(true);
  });

  it('is true for an exact full prompt name', () => {
    expect(atMenuShowsPrompts(commands, '@research', atTrigger(0))).toBe(true);
  });

  it('is false once args are typed after the name - Enter belongs to submit, not the menu', () => {
    expect(
      atMenuShowsPrompts(commands, '@research some topic', atTrigger(0))
    ).toBe(false);
    expect(atMenuShowsPrompts(commands, '@research ', atTrigger(0))).toBe(
      false
    );
  });

  it('is false when nothing matches', () => {
    expect(atMenuShowsPrompts(commands, '@nomatch', atTrigger(0))).toBe(false);
  });

  it('is false when the query matches only non-prompt commands', () => {
    expect(atMenuShowsPrompts(commands, '@save', atTrigger(0))).toBe(false);
  });

  it('is false for a bare @ (empty query)', () => {
    expect(atMenuShowsPrompts(commands, '@', atTrigger(0))).toBe(false);
  });

  it('is false when there is no active trigger', () => {
    expect(atMenuShowsPrompts(commands, '@research', null)).toBe(false);
  });

  it('is false when the trigger is not @', () => {
    expect(
      atMenuShowsPrompts(commands, '/research', { key: '/', position: 0 })
    ).toBe(false);
  });

  it('is false for an inline @ mid-message - prompts are leading-only, mid-message @ stays file attach', () => {
    expect(atMenuShowsPrompts(commands, 'hello @rese', atTrigger(6))).toBe(
      false
    );
    expect(atMenuShowsPrompts(commands, 'hello @research', atTrigger(6))).toBe(
      false
    );
  });
});

describe('isPromptMenuOpen', () => {
  const base = {
    activeCommandOpen: false,
    activeTrigger: null,
    commandInputValue: '',
    filePickerHasResults: false,
    slashCommands: commands,
    uiMode: 'tui' as const,
  };

  it('tracks slash, prompt, file, and active-command menus', () => {
    expect(
      isPromptMenuOpen({
        ...base,
        activeTrigger: { key: '/', position: 0 },
        commandInputValue: '/re',
      })
    ).toBe(true);
    expect(
      isPromptMenuOpen({
        ...base,
        activeTrigger: { key: '@', position: 0 },
        commandInputValue: '@rese',
      })
    ).toBe(true);
    expect(
      isPromptMenuOpen({
        ...base,
        activeTrigger: { key: '@', position: 4 },
        commandInputValue: 'see @file',
        filePickerHasResults: true,
      })
    ).toBe(true);
    expect(isPromptMenuOpen({ ...base, activeCommandOpen: true })).toBe(true);
  });

  it('is false when a trigger has no visible menu items', () => {
    expect(
      isPromptMenuOpen({
        ...base,
        activeTrigger: { key: '/', position: 0 },
        commandInputValue: '/missing',
      })
    ).toBe(false);
    expect(
      isPromptMenuOpen({
        ...base,
        activeTrigger: { key: '@', position: 0 },
        commandInputValue: '@missing',
      })
    ).toBe(false);
  });
});

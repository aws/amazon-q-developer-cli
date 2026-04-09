import { describe, it, expect } from 'bun:test';
import {
  filterPromptsByQuery,
  buildAtMenuItems,
  findPromptByMenuLabel,
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

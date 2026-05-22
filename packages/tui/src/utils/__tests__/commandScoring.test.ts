import { describe, expect, test } from 'bun:test';
import { scoreCommands } from '../commandScoring.js';

const commands = [
  { name: '/clear', description: 'Clear conversation history' },
  { name: '/compact', description: 'Compact conversation' },
  { name: '/context', description: 'Manage context files' },
  { name: '/copy', description: 'Copy last response' },
  { name: '/editor', description: 'Switch to editor mode' },
  { name: '/exit', description: 'Exit the application' },
  { name: '/theme', description: 'Change theme settings' },
];

describe('scoreCommands', () => {
  test('empty partial returns all commands with score 1', () => {
    const results = scoreCommands(commands, '');
    expect(results).toHaveLength(commands.length);
    expect(results.every((r) => r.score === 1)).toBe(true);
  });

  test('exact name match scores highest', () => {
    const results = scoreCommands(commands, 'context');
    expect(results[0]!.command.name).toBe('/context');
  });

  test('prefix match works', () => {
    const results = scoreCommands(commands, 'con');
    const names = results.map((r) => r.command.name);
    expect(names).toContain('/context');
    expect(names).toContain('/compact');
  });

  test('fuzzy subsequence matches', () => {
    const results = scoreCommands(commands, 'contxt');
    const match = results.find((r) => r.command.name === '/context');
    expect(match).toBeDefined();
    expect(match!.score).toBeGreaterThan(0);
  });

  test('description match works', () => {
    const results = scoreCommands(commands, 'switch');
    const match = results.find((r) => r.command.name === '/editor');
    expect(match).toBeDefined();
    expect(match!.score).toBeGreaterThan(0);
  });

  test('no match returns empty array', () => {
    expect(scoreCommands(commands, 'zzz')).toHaveLength(0);
  });

  test('case insensitive', () => {
    const results = scoreCommands(commands, 'CON');
    const names = results.map((r) => r.command.name);
    expect(names).toContain('/context');
  });

  test('results sorted by score descending', () => {
    const results = scoreCommands(commands, 'co');
    for (let i = 1; i < results.length; i++) {
      expect(results[i - 1]!.score).toBeGreaterThanOrEqual(results[i]!.score);
    }
  });

  test('equal scores sort alphabetically', () => {
    const cmds = [
      { name: '/cob', description: '' },
      { name: '/coa', description: '' },
    ];
    const results = scoreCommands(cmds, 'co');
    expect(results.map((r) => r.command.name)).toEqual(['/coa', '/cob']);
  });

  test('leading slash stripped from command names', () => {
    const results = scoreCommands(commands, 'context');
    expect(results[0]!.command.name).toBe('/context');
  });
});

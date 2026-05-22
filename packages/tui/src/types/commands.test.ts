import { describe, it, expect } from 'bun:test';
import { parseCommand } from './commands';

describe('parseCommand', () => {
  it('recognizes a normal slash command', () => {
    expect(parseCommand('/save')).toEqual({
      isCommand: true,
      name: 'save',
      args: '',
    });
  });

  it('recognizes a slash command with args', () => {
    expect(parseCommand('/context add foo.txt')).toEqual({
      isCommand: true,
      name: 'context',
      args: 'add foo.txt',
    });
  });

  it('returns isCommand false for non-slash input', () => {
    expect(parseCommand('hello world')).toEqual({
      isCommand: false,
      name: '',
      args: '',
    });
  });

  it('returns isCommand false for tilde paths (no leading slash)', () => {
    expect(parseCommand('~/does/not/start/with/slash')).toEqual({
      isCommand: false,
      name: '',
      args: '',
    });
  });

  describe('file path detection', () => {
    it('treats absolute unix paths as non-commands', () => {
      const result = parseCommand('/Users/user/file.txt');
      expect(result.isCommand).toBe(false);
    });

    it('treats paths with escaped spaces as non-commands', () => {
      const result = parseCommand(
        '/Users/user/Desktop/Screenshot\\ 2025-06-30\\ at\\ 2.13.34 PM.png read this'
      );
      expect(result.isCommand).toBe(false);
    });

    it('treats /path/to/file.json as non-command', () => {
      const result = parseCommand('/path/to/file.json');
      expect(result.isCommand).toBe(false);
    });

    it('treats dotfiles as non-commands', () => {
      const result = parseCommand('/.env');
      expect(result.isCommand).toBe(false);
    });

    it('treats windows-style paths as non-commands', () => {
      const result = parseCommand('/some\\path\\file.txt');
      expect(result.isCommand).toBe(false);
    });

    it('does NOT treat /save output.json as a file path', () => {
      const result = parseCommand('/save output.json');
      expect(result.isCommand).toBe(true);
      expect(result.name).toBe('save');
      expect(result.args).toBe('output.json');
    });
  });

  describe('double-slash and non-path inputs', () => {
    it('treats "// hello world" as non-command (looksLikeFilePath catches the /)', () => {
      const result = parseCommand('// hello world');
      expect(result.isCommand).toBe(false);
    });

    it('treats "//" as non-command', () => {
      const result = parseCommand('//');
      expect(result.isCommand).toBe(false);
    });
  });
});

import { groupOptions, filterCommands } from './commands';

describe('groupOptions', () => {
  it('groups options by their group field', () => {
    const options = [
      { value: 'a', label: 'A', group: 'First' },
      { value: 'b', label: 'B', group: 'Second' },
      { value: 'c', label: 'C', group: 'First' },
    ];
    const groups = groupOptions(options as any);
    expect(groups.get('First')).toHaveLength(2);
    expect(groups.get('Second')).toHaveLength(1);
  });

  it('uses "Other" as default group', () => {
    const options = [{ value: 'x', label: 'X' }];
    const groups = groupOptions(options as any);
    expect(groups.get('Other')).toHaveLength(1);
  });

  it('returns empty map for empty input', () => {
    expect(groupOptions([])).toEqual(new Map());
  });
});

describe('filterCommands', () => {
  const commands = [
    { name: '/clear', description: 'Clear conversation' },
    { name: '/help', description: 'Show help panel' },
    { name: '/model', description: 'Change model' },
  ];

  it('filters by name prefix', () => {
    const result = filterCommands(commands as any, '/cl');
    expect(result).toHaveLength(1);
    expect(result[0]!.name).toBe('/clear');
  });

  it('filters by description content', () => {
    const result = filterCommands(commands as any, 'panel');
    expect(result).toHaveLength(1);
    expect(result[0]!.name).toBe('/help');
  });

  it('returns all for empty partial', () => {
    const result = filterCommands(commands as any, '');
    expect(result).toHaveLength(3);
  });

  it('is case-insensitive', () => {
    const result = filterCommands(commands as any, '/CLEAR');
    expect(result).toHaveLength(1);
  });
});

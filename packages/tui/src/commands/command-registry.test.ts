import { describe, expect, it } from 'bun:test';
import {
  COMMAND_REGISTRY,
  getCommandEffect,
  getCommandPanelState,
  getLocalSlashCommands,
  isKasHandlerCommandName,
  isTurnAffectingCommand,
} from './command-registry.js';
import { KAS_COMMANDS, KasCommandName } from '../kas-commands.js';

describe('command registry', () => {
  it('derives every frontend command from an effect-bearing registration', () => {
    for (const command of getLocalSlashCommands(true)) {
      expect(getCommandEffect(command.name.slice(1))).toBeDefined();
    }
  });

  it('preserves the lite rollout command surface', () => {
    const off = getLocalSlashCommands(false);
    const on = getLocalSlashCommands(true);

    expect(off.some((command) => command.name === '/lite')).toBe(false);
    expect(on.some((command) => command.name === '/lite')).toBe(true);
    expect(on.find((command) => command.name === '/lite')?.meta.tuiOnly).toBe(
      true
    );
    expect(on.find((command) => command.name === '/tui')?.meta.liteOnly).toBe(
      true
    );
    expect(
      off.find((command) => command.name === '/verbosity')?.meta.liteOnly
    ).toBe(true);
    expect(
      on.find((command) => command.name === '/verbosity')?.meta.liteOnly
    ).toBeUndefined();
  });

  it('keeps effect handlers addressable from a single registry', () => {
    expect(getCommandEffect('help')).toBe('showHelpPanel');
    expect(getCommandEffect('changelog')).toBe('showChangelogPanel');
    expect(getCommandEffect('not-registered')).toBeUndefined();
    expect(Object.keys(COMMAND_REGISTRY).length).toBeGreaterThan(0);
  });

  it('registers every KAS panel command with the shared panel cluster', () => {
    for (const command of KAS_COMMANDS) {
      if (command.meta?.inputType === 'panel') {
        expect(getCommandPanelState(command.name), command.name).toBeDefined();
        expect(
          getCommandEffect(command.name) !== undefined ||
            isKasHandlerCommandName(command.name),
          command.name
        ).toBe(true);
      }
    }
  });

  it('classifies every KAS command as handler- or backend-owned', () => {
    for (const name of Object.values(KasCommandName)) {
      expect(typeof isKasHandlerCommandName(name)).toBe('boolean');
    }
    expect(isKasHandlerCommandName(KasCommandName.Repo)).toBe(true);
    expect(isKasHandlerCommandName(KasCommandName.Mcp)).toBe(true);
  });

  describe('turn-affecting classification', () => {
    it('holds commands that change what the agent does next', () => {
      for (const name of [
        'model',
        'effort',
        'agent',
        'plan',
        'clear',
        'rewind',
      ]) {
        expect(isTurnAffectingCommand(name), name).toBe(true);
      }
    });

    it('lets read-only and local-UI commands run alongside a turn', () => {
      for (const name of [
        'help',
        'context',
        'usage',
        'mcp',
        'tools',
        'changelog',
        'session-id',
      ]) {
        expect(isTurnAffectingCommand(name), name).toBe(false);
      }
    });

    it('holds commands that send a prompt of their own', () => {
      for (const name of ['code', 'paste', 'editor', 'reply']) {
        expect(isTurnAffectingCommand(name), name).toBe(true);
      }
    });

    it('holds commands that hand the terminal to a child process', () => {
      expect(isTurnAffectingCommand('transcript')).toBe(true);
    });

    it('holds commands that consume partial in-flight output', () => {
      expect(isTurnAffectingCommand('copy')).toBe(true);
    });

    it('splits /goal on whether it carries a description', () => {
      expect(isTurnAffectingCommand('goal')).toBe(false);
      expect(isTurnAffectingCommand('goal', '')).toBe(false);
      expect(isTurnAffectingCommand('goal', '   ')).toBe(false);
      expect(isTurnAffectingCommand('goal', 'fix all the tests')).toBe(true);
    });

    it('holds mutating subcommands of otherwise read-only panels', () => {
      for (const [name, args] of [
        ['context', 'clear'],
        ['context', 'add notes.md'],
        ['mcp', 'auth server'],
        ['tools', 'trust-all'],
        ['knowledge', 'update docs'],
      ] as const) {
        expect(isTurnAffectingCommand(name, args), `${name} ${args}`).toBe(
          true
        );
      }
    });

    it('runs explicit read-only subcommands alongside a turn', () => {
      expect(isTurnAffectingCommand('context', 'show')).toBe(false);
      expect(isTurnAffectingCommand('mcp', 'list')).toBe(false);
      expect(isTurnAffectingCommand('knowledge', 'show')).toBe(false);
    });

    it('holds every settings command until the turn ends', () => {
      for (const subcommand of [
        '',
        'display',
        'verbosity truncation',
        'theme',
        'terminal',
        'terminal:interrupt',
        'keybindings',
        'history',
        'terminal:newlines',
        'terminal:interrupt:steer',
        'terminal:interrupt:queue',
        'history:session',
        'history:global',
        'not-a-subcommand',
        'not-a-subcommand extra',
      ]) {
        expect(isTurnAffectingCommand('settings', subcommand), subcommand).toBe(
          true
        );
      }
    });

    it('runs read-only stats views and holds writes or invalid arguments', () => {
      expect(isTurnAffectingCommand('stats')).toBe(false);
      expect(isTurnAffectingCommand('stats', '10')).toBe(false);
      expect(isTurnAffectingCommand('stats', 'save stats.json')).toBe(true);
      expect(isTurnAffectingCommand('stats', 'not-a-limit')).toBe(true);
      expect(isTurnAffectingCommand('stats', '4294967296')).toBe(true);
    });

    it('treats an unregistered command as turn-affecting', () => {
      expect(isTurnAffectingCommand('not-registered')).toBe(true);
    });

    it('accepts a leading slash', () => {
      expect(isTurnAffectingCommand('/model')).toBe(true);
      expect(isTurnAffectingCommand('/help')).toBe(false);
    });

    it('classifies regardless of the case the user typed', () => {
      expect(getCommandEffect('HELP')).toBe('showHelpPanel');
      expect(isTurnAffectingCommand('HELP')).toBe(false);
      expect(isTurnAffectingCommand('/Help')).toBe(false);
      expect(isTurnAffectingCommand('MODEL')).toBe(true);
      expect(isTurnAffectingCommand('GoAl', 'ship it')).toBe(true);
    });
  });
});

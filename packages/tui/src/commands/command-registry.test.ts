import { describe, expect, it } from 'bun:test';
import {
  COMMAND_REGISTRY,
  getCommandEffect,
  getCommandPanelState,
  getLocalSlashCommands,
  isKasHandlerCommandName,
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
});

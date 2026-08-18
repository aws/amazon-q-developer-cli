import { describe, expect, it } from 'bun:test';

import { KAS_COMMANDS, KasCommandName } from './kas-commands';

describe('KAS_COMMANDS subcommand metadata', () => {
  it('advertises the knowledge subcommands the agent implements', () => {
    const knowledge = KAS_COMMANDS.find(
      (c) => c.name === KasCommandName.Knowledge
    );

    expect(knowledge?.meta?.subcommands).toEqual([
      'show',
      'add',
      'remove',
      'update',
      'clear',
      'cancel',
    ]);
  });

  // A description or hint keyed to a name that is not advertised is dead
  // metadata the dropdown can never surface, and the usual symptom of the
  // advertised list drifting away from what the agent accepts.
  for (const command of KAS_COMMANDS.filter(
    (c) => c.meta?.subcommands?.length
  )) {
    it(`keys every description and hint of ${command.name} to an advertised subcommand`, () => {
      const subcommands = command.meta?.subcommands ?? [];
      const keyed = [
        ...Object.keys(command.meta?.subcommandDescriptions ?? {}),
        ...Object.keys(command.meta?.subcommandHints ?? {}),
      ];

      expect(subcommands).toEqual(expect.arrayContaining(keyed));
    });
  }
});

import { describe, it, expect } from 'bun:test';
import type { Kiro } from '../kiro';
import type { KasCommand } from '../kas-commands';

describe('kas-commands', () => {
  describe('KAS_COMMANDS array', () => {
    it('contains /spec command definition', async () => {
      const { KAS_COMMANDS } = await import('../kas-commands');
      const specCmd = KAS_COMMANDS.find(
        (cmd: KasCommand) => cmd.name === '/spec'
      );
      expect(specCmd).toBeDefined();
      expect(specCmd!.description).toContain('spec');
      expect(specCmd!.meta).toEqual({
        local: true,
        subcommands: ['new', 'run', 'view', 'analyze_requirements'],
        subcommandHints: {
          new: '<feature-name>',
          run: '<feature-name>',
          view: '<feature-name> [requirements|design|tasks]',
          analyze_requirements: '↵ to select a spec',
        },
      });
    });

    it('contains /chat command definition', async () => {
      const { KAS_COMMANDS } = await import('../kas-commands');
      const chatCmd = KAS_COMMANDS.find(
        (cmd: KasCommand) => cmd.name === '/chat'
      );
      expect(chatCmd).toBeDefined();
      expect(chatCmd!.meta?.subcommands).toEqual(['new', 'save', 'load']);
    });
  });

  describe('Conditional /spec command visibility', () => {
    it('/spec is NOT in app-store static slashCommands (V2 engine)', async () => {
      const { createAppStore } = await import('../stores/app-store');

      const mockKiro = {} as InstanceType<typeof Kiro>;
      const store = createAppStore({ kiro: mockKiro, agentEngine: 'v2' });
      const state = store.getState();

      const specInStatic = state.slashCommands.find(
        (cmd) => cmd.name === '/spec'
      );
      expect(specInStatic).toBeUndefined();
    });

    it('/spec appears in kasCommands when agentEngine === kas', async () => {
      const { createAppStore } = await import('../stores/app-store');
      const { KasCommandName } = await import('../kas-commands');

      const mockKiro = {} as InstanceType<typeof Kiro>;
      const store = createAppStore({ kiro: mockKiro, agentEngine: 'kas' });

      const state = store.getState();
      const specInKas = state.kasCommands.find(
        (cmd: KasCommand) => cmd.name === KasCommandName.Spec
      );
      expect(specInKas).toBeDefined();
      expect(specInKas!.name).toBe(KasCommandName.Spec);
      expect(specInKas!.description).toContain('spec');
    });

    it('/spec is included in KAS_COMMANDS for KAS mode', async () => {
      const { KAS_COMMANDS } = await import('../kas-commands');

      const specCmd = KAS_COMMANDS.find(
        (cmd: KasCommand) => cmd.name === '/spec'
      );
      expect(specCmd).toBeDefined();
    });
  });
});

import { describe, it, expect } from 'bun:test';
import type { Kiro } from '../kiro';

describe('slash-commands', () => {
  describe('SLASH_COMMANDS array', () => {
    it('contains /spec command definition', async () => {
      const { SLASH_COMMANDS } = await import('../slash-commands');
      const specCmd = SLASH_COMMANDS.find((cmd) => cmd.name === '/spec');
      expect(specCmd).toBeDefined();
      expect(specCmd!.description).toContain('spec');
      expect(specCmd!.meta).toEqual({
        local: true,
        subcommands: ['new', 'run'],
        subcommandHints: { new: '<feature-name>', run: '<feature-name>' },
      });
    });

    it('/spec is available whenever KAS is active (no gating on extension methods)', async () => {
      const { SLASH_COMMANDS } = await import('../slash-commands');
      const specCmd = SLASH_COMMANDS.find((cmd) => cmd.name === '/spec');
      expect(specCmd).toBeDefined();
    });
  });

  describe('Conditional /spec command visibility', () => {
    it('/spec is NOT in app-store static slashCommands (non-KAS engines)', async () => {
      // The app-store's static slashCommands array is defined inline in
      // createAppStore. We import it and verify /spec is absent from the
      // initial state. This confirms non-KAS users never see /spec.
      //
      // Note: we cannot rely on the real `Kiro` class here because other
      // test files in this suite call `mock.module('../kiro', ...)` which
      // replaces `Kiro` with a `mock(() => ...)` arrow function for the
      // rest of the test run. Arrow functions have no `.prototype`, so
      // `Object.create(Kiro.prototype)` throws "Object prototype may only
      // be an Object or null" depending on test execution order.
      // `createAppStore` only stores the `kiro` reference without calling
      // any methods on it during initialization, so a typed placeholder is
      // sufficient.
      const { createAppStore } = await import('../stores/app-store');

      const mockKiro = {} as InstanceType<typeof Kiro>;
      const store = createAppStore({ kiro: mockKiro });
      const state = store.getState();

      const specInStatic = state.slashCommands.find(
        (cmd) => cmd.name === '/spec'
      );
      expect(specInStatic).toBeUndefined();
    });

    it('/spec appears in command list when KAS engine broadcasts extension methods', async () => {
      // When KAS is active, KasAcpClient broadcasts ExtensionMethodsDiscovered
      // which calls setExtensionCommands. The combined list (extensionCommands
      // + slashCommands) then includes /spec.
      const { createAppStore } = await import('../stores/app-store');

      const mockKiro = {} as InstanceType<typeof Kiro>;
      const store = createAppStore({ kiro: mockKiro });

      // Simulate what KasAcpClient does after initialize():
      // It filters SLASH_COMMANDS and broadcasts them as extension commands
      store.getState().setExtensionCommands([
        {
          name: '/spec',
          description: 'List specs, switch to spec mode, or run spec tasks',
          source: 'backend' as const,
          meta: {
            local: true,
            subcommands: ['new', 'run'],
            subcommandHints: {
              new: '<feature-name>',
              run: '<feature-name>',
            },
          },
        },
      ]);

      const state = store.getState();
      const specInExtension = state.extensionCommands.find(
        (cmd) => cmd.name === '/spec'
      );
      expect(specInExtension).toBeDefined();
      expect(specInExtension!.name).toBe('/spec');
      expect(specInExtension!.description).toContain('spec');
    });

    it('/spec is included in SLASH_COMMANDS for KAS mode', async () => {
      // All SLASH_COMMANDS are broadcast by KasAcpClient on initialize().
      // /spec is always present in the array.
      const { SLASH_COMMANDS } = await import('../slash-commands');

      const specCmd = SLASH_COMMANDS.find((cmd) => cmd.name === '/spec');
      expect(specCmd).toBeDefined();
    });
  });
});

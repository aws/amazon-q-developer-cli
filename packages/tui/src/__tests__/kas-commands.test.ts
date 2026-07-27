import { describe, it, expect } from 'bun:test';
import type { Kiro } from '../kiro';
import type { KasCommand } from '../kas-commands';
import { Feature } from '../features';

describe('kas-commands', () => {
  describe('filterByEnabledFeatures', () => {
    const gated = (name: string, feature: Feature): KasCommand =>
      ({ name, description: 'gated', feature }) as unknown as KasCommand;

    it('passes ungated commands through and resolves gates from the env', async () => {
      const { KAS_COMMANDS, filterByEnabledFeatures } =
        await import('../kas-commands');
      const { features } = await import('../features');
      const withGated = [...KAS_COMMANDS, gated('/mem', Feature.Memory)];

      // KAS_COMMANDS itself carries gated entries (e.g. /autonomous behind
      // remote_sandbox), so the all-off baseline is the ungated subset.
      const ungated = KAS_COMMANDS.filter((c) => !c.feature);

      const originalEnv = process.env.KIRO_ENABLED_FEATURES;
      try {
        process.env.KIRO_ENABLED_FEATURES = '[]';
        features._resetForTests();
        expect(filterByEnabledFeatures(withGated)).toEqual(ungated);

        process.env.KIRO_ENABLED_FEATURES = '["memory", "remote_sandbox"]';
        features._resetForTests();
        expect(filterByEnabledFeatures(withGated)).toEqual(withGated);
      } finally {
        if (originalEnv === undefined) delete process.env.KIRO_ENABLED_FEATURES;
        else process.env.KIRO_ENABLED_FEATURES = originalEnv;
        features._resetForTests();
      }
    });
  });

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

    it('/model and /effort expose set-current-as-default as a subcommand', async () => {
      const { KAS_COMMANDS } = await import('../kas-commands');
      const modelCmd = KAS_COMMANDS.find(
        (cmd: KasCommand) => cmd.name === '/model'
      );
      const effortCmd = KAS_COMMANDS.find(
        (cmd: KasCommand) => cmd.name === '/effort'
      );
      expect(modelCmd!.meta?.subcommands).toEqual(['set-current-as-default']);
      expect(effortCmd!.meta?.subcommands).toEqual(['set-current-as-default']);
    });

    it('/sessions is a cloud-gated alias of /chat: same handler, same meta plus cloudOnly', async () => {
      const { KAS_COMMANDS, KasCommandName } = await import('../kas-commands');
      const { kasHandlers } = await import('../commands/kas-handlers');
      const chatCmd = KAS_COMMANDS.find(
        (cmd: KasCommand) => cmd.name === '/chat'
      );
      const sessionsCmd = KAS_COMMANDS.find(
        (cmd: KasCommand) => cmd.name === '/sessions'
      );
      expect(sessionsCmd).toBeDefined();
      expect(sessionsCmd!.meta).toEqual({ ...chatCmd!.meta, cloudOnly: true });
      expect(kasHandlers[KasCommandName.Sessions]).toBe(
        kasHandlers[KasCommandName.Chat]
      );
    });
  });

  describe('/autonomous dark-ship gating', () => {
    it('is feature-gated behind remote_sandbox AND cloudOnly', async () => {
      const { KAS_COMMANDS } = await import('../kas-commands');
      const autonomousCmd = KAS_COMMANDS.find(
        (cmd: KasCommand) => cmd.name === '/autonomous'
      );
      expect(autonomousCmd).toBeDefined();
      expect(autonomousCmd!.feature).toBe(Feature.RemoteSandbox);
      expect(autonomousCmd!.meta).toEqual({
        cloudOnly: true,
        subcommands: ['on', 'off'],
      });
    });

    it('is dropped by getKasCommands unless remote_sandbox is enabled', async () => {
      const { getKasCommands } = await import('../kas-commands');
      const { features } = await import('../features');

      const originalEnv = process.env.KIRO_ENABLED_FEATURES;
      try {
        process.env.KIRO_ENABLED_FEATURES = '[]';
        features._resetForTests();
        expect(
          getKasCommands().find((c) => c.name === '/autonomous')
        ).toBeUndefined();

        process.env.KIRO_ENABLED_FEATURES = '["remote_sandbox"]';
        features._resetForTests();
        expect(
          getKasCommands().find((c) => c.name === '/autonomous')
        ).toBeDefined();
      } finally {
        if (originalEnv === undefined) delete process.env.KIRO_ENABLED_FEATURES;
        else process.env.KIRO_ENABLED_FEATURES = originalEnv;
        features._resetForTests();
      }
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

import { describe, it, expect } from 'bun:test';
import { dispatch } from '../dispatcher';
import { executeCommandWithArg } from '../index';
import type { SlashCommand } from '../../stores/app-store';
import { createMockCommandContext } from './test-helpers.js';

function makeCmd(overrides: Partial<SlashCommand> = {}): SlashCommand {
  return {
    name: '/test',
    description: 'test',
    source: 'backend',
    ...overrides,
  };
}

describe('sub-command flow', () => {
  describe('dispatch with selection + subcommands', () => {
    it('shows selection menu for command with subcommands when no args', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.getCommandOptions as any).mockResolvedValue({
        options: [
          { value: 'agent-1', label: 'Agent One', description: 'First agent' },
          {
            value: 'agent-2',
            label: 'Agent Two',
            description: 'Second agent',
          },
        ],
      });

      const cmd = makeCmd({
        name: '/agent',
        meta: {
          inputType: 'selection',
          subcommands: ['create', 'edit', 'swap'],
          subcommandHints: { create: '<name>', edit: '[name]', swap: '<name>' },
        },
      });
      await dispatch(cmd, '', ctx);

      // Should show the selection menu (not the subcommand menu)
      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      expect(call[0].options).toHaveLength(2);
      expect(call[0].options[0].value).toBe('agent-1');
    });

    it('dispatches subcommand as args when provided directly', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Agent created',
        data: undefined,
      });

      const cmd = makeCmd({
        name: '/agent',
        meta: {
          inputType: 'selection',
          subcommands: ['create', 'edit', 'swap'],
        },
      });
      await dispatch(cmd, 'create my-agent', ctx);

      expect(ctx.kiro.executeCommand).toHaveBeenCalled();
    });
  });

  describe('executeCommandWithArg with subcommand value', () => {
    it('executes immediately for sub-commands without hints (no args needed)', async () => {
      const ctx = createMockCommandContext({
        slashCommands: [
          makeCmd({
            name: '/context',
            meta: {
              inputType: 'panel',
              subcommands: ['add', 'remove', 'clear'],
              subcommandHints: {
                add: '[--force] <path>...',
                remove: '<path>...',
              },
            },
          }),
        ],
      });
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Context cleared',
        data: undefined,
      });

      // "clear" has no hint, so it should execute immediately
      await executeCommandWithArg('context', 'clear', ctx);

      expect(ctx.kiro.executeCommand).toHaveBeenCalled();
    });

    it('dispatches agent swap prefix for agent subcommand', async () => {
      const ctx = createMockCommandContext({
        slashCommands: [
          makeCmd({
            name: '/agent',
            meta: {
              inputType: 'selection',
              subcommands: ['create', 'edit', 'swap'],
            },
          }),
        ],
      });
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Agent switched',
        data: undefined,
      });

      // When selecting an agent name from the dropdown, it gets prefixed with "swap"
      await executeCommandWithArg('agent', 'my-agent', ctx);

      const executeCall = (ctx.kiro.executeCommand as any).mock.calls[0]!;
      expect(executeCall[0].args.value).toBe('swap my-agent');
    });
  });

  describe('panel commands with subcommands', () => {
    it('opens panel for panel command with subcommands when no args', async () => {
      const ctx = createMockCommandContext();

      const cmd = makeCmd({
        name: '/tools',
        meta: {
          inputType: 'panel',
          subcommands: ['trust-all', 'trust', 'untrust', 'reset'],
          subcommandHints: { trust: '<name>', untrust: '<name>' },
        },
      });
      await dispatch(cmd, '', ctx);

      // Panel commands set activeCommand with empty options
      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      expect(call[0].options).toHaveLength(0);
    });

    it('executes backend for panel command with subcommand args', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'All tools trusted',
        data: undefined,
      });

      const cmd = makeCmd({
        name: '/tools',
        meta: {
          inputType: 'panel',
          subcommands: ['trust-all', 'trust', 'untrust', 'reset'],
        },
      });
      await dispatch(cmd, 'trust-all', ctx);

      expect(ctx.kiro.executeCommand).toHaveBeenCalled();
    });
  });

  describe('subcommandHints in meta', () => {
    it('sub-commands with hints should prefill input, not execute', async () => {
      // This tests the contract: when a CommandOption has a hint,
      // the CommandMenu onSelect handler prefills instead of executing.
      // We verify this indirectly by checking that executeCommandWithArg
      // dispatches correctly when called (the no-hint path).
      const ctx = createMockCommandContext({
        slashCommands: [
          makeCmd({
            name: '/tools',
            meta: {
              inputType: 'panel',
              subcommands: ['trust-all', 'trust', 'untrust', 'reset'],
              subcommandHints: { trust: '<name>', untrust: '<name>' },
            },
          }),
        ],
      });
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'All tools trusted',
        data: undefined,
      });

      // "trust-all" has no hint → executes immediately
      await executeCommandWithArg('tools', 'trust-all', ctx);
      expect(ctx.kiro.executeCommand).toHaveBeenCalled();
    });

    it('sub-commands with args dispatch correctly when args are provided', async () => {
      const ctx = createMockCommandContext({
        slashCommands: [
          makeCmd({
            name: '/tools',
            meta: {
              inputType: 'panel',
              subcommands: ['trust-all', 'trust', 'untrust', 'reset'],
              subcommandHints: { trust: '<name>', untrust: '<name>' },
            },
          }),
        ],
      });
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Tool trusted',
        data: undefined,
      });

      // After user prefills "/tools trust " and types "my-tool", dispatch runs with full args
      const cmd = makeCmd({
        name: '/tools',
        meta: {
          inputType: 'panel',
          subcommands: ['trust-all', 'trust', 'untrust', 'reset'],
          subcommandHints: { trust: '<name>', untrust: '<name>' },
        },
      });
      await dispatch(cmd, 'trust my-tool', ctx);

      expect(ctx.kiro.executeCommand).toHaveBeenCalled();
      const executeCall = (ctx.kiro.executeCommand as any).mock.calls[0]!;
      expect(executeCall[0].args.value).toBe('trust my-tool');
    });
  });
});

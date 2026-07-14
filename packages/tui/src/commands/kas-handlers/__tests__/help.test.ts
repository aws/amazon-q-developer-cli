import { describe, it, expect, mock } from 'bun:test';
import { handleHelp } from '../help';
import { createMockCommandContext } from '../../__tests__/test-helpers';

mock.module('../../../kiro', () => ({
  Kiro: mock(() => ({ initialize: mock() })),
}));

describe('/help KAS handler — client-side command list', () => {
  it('calls setShowHelpPanel with merged kas + local commands sorted alphabetically', async () => {
    const kasCommands = [
      { name: '/help', description: 'Show help', meta: {} },
      { name: '/clear', description: 'Clear conversation', meta: {} },
      {
        name: '/agent',
        description: 'Switch agents',
        meta: { subcommands: ['create', 'swap'] },
      },
    ];
    const localCommands = [
      {
        name: '/quit',
        description: 'Exit the TUI',
        source: 'local' as const,
        meta: { local: true },
      },
      {
        name: '/settings',
        description: 'Open settings',
        source: 'local' as const,
        meta: { local: true },
      },
    ];

    const ctx = createMockCommandContext({
      kasCommands: kasCommands as any,
      slashCommands: localCommands as any,
    });
    (ctx as any).agentEngine = 'kas';

    await handleHelp(kasCommands[0] as any, '', ctx);

    expect(ctx._spies.setShowHelpPanel).toHaveBeenCalledTimes(1);
    const [visible, commands] = ctx._spies.setShowHelpPanel!.mock.calls[0]!;
    expect(visible).toBe(true);

    const names = (commands as any[]).map((c: any) => c.name);
    const sortedNames = [...names].sort((a, b) => a.localeCompare(b));
    expect(names).toEqual(sortedNames);
  });

  it('hides liteOnly local commands in TUI mode, shows them in lite mode', async () => {
    // /verbosity is liteOnly; KAS /help used to leak it in TUI mode.
    const slashCommands = [
      {
        name: '/quit',
        description: 'Exit',
        source: 'local' as const,
        meta: {},
      },
      {
        name: '/verbosity',
        description: 'Configure lite-mode rendering',
        source: 'local' as const,
        meta: { local: true, liteOnly: true },
      },
    ];

    const tuiCtx = createMockCommandContext({
      slashCommands: slashCommands as any,
    });
    (tuiCtx.getUiMode as any).mockReturnValue('tui');
    await handleHelp({ name: '/help' } as any, '', tuiCtx);
    const tuiNames = (
      tuiCtx._spies.setShowHelpPanel!.mock.calls[0]![1] as any[]
    ).map((c) => c.name);
    expect(tuiNames).not.toContain('/verbosity');
    expect(tuiNames).toContain('/quit');

    const liteCtx = createMockCommandContext({
      slashCommands: slashCommands as any,
    });
    (liteCtx.getUiMode as any).mockReturnValue('lite');
    await handleHelp({ name: '/help' } as any, '', liteCtx);
    const liteNames = (
      liteCtx._spies.setShowHelpPanel!.mock.calls[0]![1] as any[]
    ).map((c) => c.name);
    expect(liteNames).toContain('/verbosity');
  });

  it('includes subcommands metadata from kas commands', async () => {
    const kasCommands = [
      {
        name: '/agent',
        description: 'Switch agents',
        meta: { subcommands: ['create', 'edit', 'swap'] },
      },
    ];

    const ctx = createMockCommandContext({ kasCommands: kasCommands as any });
    (ctx as any).agentEngine = 'kas';

    await handleHelp(kasCommands[0] as any, '', ctx);

    const [, commands] = ctx._spies.setShowHelpPanel!.mock.calls[0]!;
    const agentCmd = (commands as any[]).find((c: any) => c.name === '/agent');
    expect(agentCmd?.subcommands).toEqual(['create', 'edit', 'swap']);
  });

  it('hides cloud-only kas commands outside a cloud session, shows them inside', async () => {
    const kasCommands = [
      { name: '/help', description: 'Show help', meta: {} },
      {
        name: '/repo',
        description: 'Attach a repository to the cloud session',
        meta: { inputType: 'panel', cloudOnly: true },
      },
    ];

    const localCtx = createMockCommandContext({
      kasCommands: kasCommands as any,
    });
    await handleHelp(kasCommands[0] as any, '', localCtx);
    const localNames = (
      localCtx._spies.setShowHelpPanel!.mock.calls[0]![1] as any[]
    ).map((c: any) => c.name);
    expect(localNames).not.toContain('/repo');
    expect(localNames).toContain('/help');

    const cloudCtx = createMockCommandContext({
      kasCommands: kasCommands as any,
    });
    cloudCtx.cloudSessionActive = true;
    await handleHelp(kasCommands[0] as any, '', cloudCtx);
    const cloudNames = (
      cloudCtx._spies.setShowHelpPanel!.mock.calls[0]![1] as any[]
    ).map((c: any) => c.name);
    expect(cloudNames).toContain('/repo');
  });
});

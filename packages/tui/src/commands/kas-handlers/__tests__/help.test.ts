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
});

import { describe, expect, it, mock } from 'bun:test';
import { runMcpPanelAction } from '../mcp-panel-actions.js';

describe('runMcpPanelAction', () => {
  it('preserves the V2 action-then-refresh flow', async () => {
    const executeCommand = mock()
      .mockResolvedValueOnce({ success: true, message: '' })
      .mockResolvedValueOnce({
        success: true,
        message: '',
        data: {
          servers: [{ name: 'github', status: 'running', toolCount: 2 }],
          mode: 'remove',
        },
      });
    const setShowMcpPanel = mock(() => {});

    await runMcpPanelAction({
      kiro: { executeCommand } as any,
      value: 'remove github',
      refreshValue: 'remove',
      panelMode: 'remove',
      setShowMcpPanel,
    });

    expect(executeCommand.mock.calls).toEqual([
      [{ command: 'mcp', args: { subcommand: 'remove github' } }],
      [{ command: 'mcp', args: { subcommand: 'remove' } }],
    ]);
    expect(setShowMcpPanel).toHaveBeenCalledWith(
      true,
      [{ name: 'github', status: 'running', toolCount: 2 }],
      'remove'
    );
  });
});

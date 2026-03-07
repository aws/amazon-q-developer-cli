import { describe, it, expect, mock } from 'bun:test';
import { dispatch } from './dispatcher';
import type { CommandContext } from './types';
import type { SlashCommand } from '../stores/app-store';

/** Create a mock CommandContext with spies on all methods */
function createMockCtx(): CommandContext & {
  _spies: Record<string, ReturnType<typeof mock>>;
} {
  const spies: Record<string, ReturnType<typeof mock>> = {};
  const spy = (name: string) => {
    const fn = mock(() => {});
    spies[name] = fn;
    return fn;
  };

  return {
    kiro: {
      executeCommand: mock(() =>
        Promise.resolve({ success: true, message: '', data: undefined })
      ),
      getCommandOptions: mock(() => Promise.resolve({ options: [] })),
    } as any,
    slashCommands: [],
    prompts: [],
    showAlert: spy('showAlert') as any,
    setLoadingMessage: spy('setLoadingMessage') as any,
    setActiveCommand: spy('setActiveCommand') as any,
    setCurrentModel: spy('setCurrentModel') as any,
    setCurrentAgent: spy('setCurrentAgent') as any,
    setContextUsage: spy('setContextUsage') as any,
    setShowContextBreakdown: spy('setShowContextBreakdown') as any,
    setShowHelpPanel: spy('setShowHelpPanel') as any,
    setShowPromptsPanel: spy('setShowPromptsPanel') as any,
    setShowIssuePanel: spy('setShowIssuePanel') as any,
    setShowUsagePanel: spy('setShowUsagePanel') as any,
    setShowMcpPanel: spy('setShowMcpPanel') as any,
    setShowToolsPanel: spy('setShowToolsPanel') as any,
    clearMessages: spy('clearMessages') as any,
    sendMessage: spy('sendMessage') as any,
    clearUIState: spy('clearUIState') as any,
    _spies: spies,
  };
}

function makeCmd(overrides: Partial<SlashCommand> = {}): SlashCommand {
  return {
    name: '/test',
    description: 'test',
    source: 'backend',
    ...overrides,
  };
}

describe('dispatch', () => {
  describe('/issue command', () => {
    it('shows alert when browser opens successfully (no data)', async () => {
      const ctx = createMockCtx();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Opening in browser...',
        data: undefined,
      });

      const cmd = makeCmd({ name: '/issue' });
      await dispatch(cmd, '', ctx);

      expect(ctx._spies.showAlert!.mock.calls[0]).toEqual([
        'Opening in browser...',
        'success',
        3000,
      ]);
      // No activeCommand set
      expect(ctx._spies.setActiveCommand!).not.toHaveBeenCalled();
    });

    it('sets activeCommand and opens panel when browser fails (has data)', async () => {
      const ctx = createMockCtx();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Could not open browser.',
        data: { url: 'https://example.com' },
      });

      const cmd = makeCmd({ name: '/issue' });
      await dispatch(cmd, '', ctx);

      // Effect sets activeCommand to block input while panel is open
      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
      expect(ctx._spies.setShowIssuePanel!.mock.calls[0]).toEqual([
        true,
        'https://example.com',
      ]);
    });
  });
});

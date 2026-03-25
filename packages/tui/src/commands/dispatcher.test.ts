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
    showAlert: spy('showAlert') as any,
    setLoadingMessage: spy('setLoadingMessage') as any,
    setActiveCommand: spy('setActiveCommand') as any,
    setCurrentModel: spy('setCurrentModel') as any,
    setCurrentAgent: spy('setCurrentAgent') as any,
    setContextUsage: spy('setContextUsage') as any,
    setShowContextBreakdown: spy('setShowContextBreakdown') as any,
    setShowHelpPanel: spy('setShowHelpPanel') as any,
    setShowUsagePanel: spy('setShowUsagePanel') as any,
    setShowMcpPanel: spy('setShowMcpPanel') as any,
    setShowToolsPanel: spy('setShowToolsPanel') as any,
    setShowKnowledgePanel: spy('setShowKnowledgePanel') as any,
    setShowCodePanel: spy('setShowCodePanel') as any,
    clearMessages: spy('clearMessages') as any,
    sendMessage: spy('sendMessage') as any,
    clearUIState: spy('clearUIState') as any,
    createStreamEventHandler: spy('createStreamEventHandler') as any,
    setSessionId: spy('setSessionId') as any,
    addSystemMessage: spy('addSystemMessage') as any,
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
  describe('/feedback command', () => {
    it('shows selection menu when options are returned', async () => {
      const ctx = createMockCtx();
      (ctx.kiro.getCommandOptions as any).mockResolvedValue({
        options: [
          {
            value: 'general',
            label: 'General feedback',
            description: 'Share general thoughts',
          },
          {
            value: 'feature',
            label: 'Feature request',
            description: 'Request a feature',
          },
          {
            value: 'issue',
            label: 'Report an issue',
            description: 'Report a bug',
          },
        ],
      });

      const cmd = makeCmd({
        name: '/feedback',
        meta: { inputType: 'selection' },
      });
      await dispatch(cmd, '', ctx);

      // Should set activeCommand with the 3 options
      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      expect(call[0].options).toHaveLength(3);
      expect(call[0].options[0].value).toBe('general');
    });

    it('shows alert when executed with args (after selection)', async () => {
      const ctx = createMockCtx();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Opening in browser...',
        data: undefined,
      });

      const cmd = makeCmd({
        name: '/feedback',
        meta: { inputType: 'selection' },
      });
      await dispatch(cmd, 'issue', ctx);

      expect(ctx._spies.showAlert!.mock.calls[0]).toEqual([
        'Opening in browser...',
        'success',
        5000,
      ]);
    });
  });

  describe('/context command', () => {
    it('opens panel when no args provided', async () => {
      const ctx = createMockCtx();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Context breakdown - 42% used',
        data: {
          contextUsagePercentage: 42,
          breakdown: { contextFiles: { tokens: 100, percent: 10 } },
        },
      });

      const cmd = makeCmd({
        name: '/context',
        meta: { inputType: 'panel' },
      });
      await dispatch(cmd, '', ctx);

      expect(ctx._spies.setShowContextBreakdown!).toHaveBeenCalled();
      expect(ctx._spies.setShowContextBreakdown!.mock.calls[0]?.[0]).toBe(true);
    });

    it('shows alert for /context add with args', async () => {
      const ctx = createMockCtx();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: "Added 'foo.txt' to context",
        data: undefined,
      });

      const cmd = makeCmd({
        name: '/context',
        meta: { inputType: 'panel' },
      });
      await dispatch(cmd, 'add foo.txt', ctx);

      expect(ctx._spies.showAlert!).toHaveBeenCalled();
      expect(ctx._spies.showAlert!.mock.calls[0]?.[0]).toBe(
        "Added 'foo.txt' to context"
      );
      expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('success');
    });

    it('shows error alert for /context remove with missing path', async () => {
      const ctx = createMockCtx();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: false,
        message: 'Resource not found: nonexistent.txt',
        data: undefined,
      });

      const cmd = makeCmd({
        name: '/context',
        meta: { inputType: 'panel' },
      });
      await dispatch(cmd, 'remove nonexistent.txt', ctx);

      expect(ctx._spies.showAlert!).toHaveBeenCalled();
      expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('error');
    });

    it('does not open panel for /context add result without breakdown', async () => {
      const ctx = createMockCtx();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: "Added 'src/*.rs' to context",
        data: undefined,
      });

      const cmd = makeCmd({
        name: '/context',
        meta: { inputType: 'panel' },
      });
      await dispatch(cmd, 'add src/*.rs', ctx);

      const calls = ctx._spies.setShowContextBreakdown!.mock.calls;
      const openedPanel = calls.some(
        (c: unknown[]) => c[0] === true && c[1] != null
      );
      expect(openedPanel).toBe(false);
    });
  });

  describe('/chat command', () => {
    it('shows "No previous sessions found" when options list is empty', async () => {
      const ctx = createMockCtx();
      (ctx.kiro.listSessions as any) = mock(() =>
        Promise.resolve({ sessions: [] })
      );

      const cmd = makeCmd({
        name: '/chat',
        meta: { inputType: 'selection', local: true },
      });
      await dispatch(cmd, '', ctx);

      expect(ctx._spies.showAlert!.mock.calls[0]).toEqual([
        'No previous sessions found',
        'error',
        3000,
      ]);
    });
  });
});

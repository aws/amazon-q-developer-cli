import { describe, it, expect, mock } from 'bun:test';
import { dispatch } from '../dispatcher';
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

describe('dispatch - additional coverage', () => {
  describe('prompt type commands', () => {
    it('sends /<cmd> as message when no args', async () => {
      const ctx = createMockCommandContext();
      const cmd = makeCmd({
        name: '/fix',
        meta: { type: 'prompt' },
      });

      await dispatch(cmd, '', ctx);

      expect(ctx._spies.sendMessage!).toHaveBeenCalledWith('/fix');
    });

    it('sends /<cmd> <args> as message when args provided', async () => {
      const ctx = createMockCommandContext();
      const cmd = makeCmd({
        name: '/fix',
        meta: { type: 'prompt' },
      });

      await dispatch(cmd, 'the bug in auth', ctx);

      expect(ctx._spies.sendMessage!).toHaveBeenCalledWith(
        '/fix the bug in auth'
      );
    });
  });

  describe('skill type commands', () => {
    it('sends message for skill commands', async () => {
      const ctx = createMockCommandContext();
      const cmd = makeCmd({
        name: '/review',
        meta: { type: 'skill' },
      });

      await dispatch(cmd, '', ctx);

      expect(ctx._spies.sendMessage!).toHaveBeenCalledWith('/review');
    });
  });

  describe('panel inputType with no args', () => {
    it('sets activeCommand with empty options', async () => {
      const ctx = createMockCommandContext();
      const cmd = makeCmd({
        name: '/tools',
        meta: { inputType: 'panel' },
      });

      await dispatch(cmd, '', ctx);

      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      expect(call[0].command.name).toBe('/tools');
      expect(call[0].options).toEqual([]);
    });
  });

  describe('backend command execution', () => {
    it('calls kiro.executeCommand with command and args', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'done',
        data: undefined,
      });

      const cmd = makeCmd({
        name: '/compact',
        source: 'backend',
      });
      await dispatch(cmd, 'aggressive', ctx);

      expect(ctx.kiro.executeCommand).toHaveBeenCalled();
      const call = (ctx.kiro.executeCommand as any).mock.calls[0]!;
      expect(call[0].command).toBe('compact');
      expect(call[0].args).toEqual({ value: 'aggressive' });
    });

    it('shows error alert when kiro.executeCommand fails', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockRejectedValue(
        new Error('Connection failed')
      );

      const cmd = makeCmd({
        name: '/compact',
        source: 'backend',
      });
      await dispatch(cmd, '', ctx);

      expect(ctx._spies.showAlert!).toHaveBeenCalled();
      const call = ctx._spies.showAlert!.mock.calls[0]!;
      expect(call[0]).toBe('Connection failed');
      expect(call[1]).toBe('error');
    });
  });

  describe('result.message display', () => {
    it('shows result.message when no effect handled messaging', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Operation completed',
        data: undefined,
      });

      // Use a command name that has no effect handler
      const cmd = makeCmd({
        name: '/unknown-backend',
        source: 'backend',
      });
      await dispatch(cmd, 'args', ctx);

      expect(ctx._spies.showAlert!).toHaveBeenCalled();
      const call = ctx._spies.showAlert!.mock.calls[0]!;
      expect(call[0]).toBe('Operation completed');
      expect(call[1]).toBe('success');
    });

    it('shows error status for unsuccessful result', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: false,
        message: 'Something went wrong',
        data: undefined,
      });

      const cmd = makeCmd({
        name: '/unknown-backend',
        source: 'backend',
      });
      await dispatch(cmd, 'args', ctx);

      expect(ctx._spies.showAlert!).toHaveBeenCalled();
      const call = ctx._spies.showAlert!.mock.calls[0]!;
      expect(call[0]).toBe('Something went wrong');
      expect(call[1]).toBe('error');
    });
  });

  describe('local commands skip backend', () => {
    it('does not call executeCommand for local commands', async () => {
      const ctx = createMockCommandContext();

      const cmd = makeCmd({
        name: '/editor',
        source: 'local' as const,
        meta: { local: true },
      });
      // runEffect for 'editor' calls openEditorSync which we can't test here,
      // but we verify executeCommand was NOT called
      await dispatch(cmd, '', ctx);

      expect(ctx.kiro.executeCommand).not.toHaveBeenCalled();
    });
  });

  describe('/agent swap loading message', () => {
    it('shows loading message when swapping agent', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Agent switched',
        data: { agent: { name: 'coder' } },
      });

      const cmd = makeCmd({
        name: '/agent',
        source: 'backend',
      });
      await dispatch(cmd, 'swap coder', ctx);

      // setLoadingMessage should have been called with the agent name
      const loadingCalls = ctx._spies.setLoadingMessage!.mock.calls;
      const hasAgentMessage = loadingCalls.some(
        (c: any[]) => typeof c[0] === 'string' && c[0].includes('coder')
      );
      expect(hasAgentMessage).toBe(true);
    });
  });

  describe('/chat options fetched via kiro.listSessions', () => {
    it('formats sessions with relative time', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro as any).listSessions = mock(() =>
        Promise.resolve({
          sessions: [
            {
              sessionId: 'abc12345-def',
              title: 'My Chat',
              updatedAt: new Date().toISOString(),
            },
          ],
        })
      );
      (ctx.kiro as any).sessionId = 'current-session';

      const cmd = makeCmd({
        name: '/chat',
        meta: { inputType: 'selection', local: true },
      });
      await dispatch(cmd, '', ctx);

      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      expect(call[0].options).toHaveLength(1);
      expect(call[0].options[0].label).toContain('My Chat');
      expect(call[0].options[0].value).toBe('abc12345-def');
    });
  });

  describe('selection with no options', () => {
    it('shows alert for non-chat commands with no options', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.getCommandOptions as any).mockResolvedValue({
        options: [],
      });

      const cmd = makeCmd({
        name: '/model',
        meta: { inputType: 'selection' },
      });
      await dispatch(cmd, '', ctx);

      expect(ctx._spies.showAlert!).toHaveBeenCalled();
      const call = ctx._spies.showAlert!.mock.calls[0]!;
      expect(call[0]).toContain('No options available');
      expect(call[1]).toBe('error');
    });
  });
});

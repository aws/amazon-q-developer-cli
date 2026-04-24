import { describe, it, expect, mock } from 'bun:test';
import { dispatch } from './dispatcher';
import type { SlashCommand } from '../stores/app-store';
import { createMockCommandContext } from './__tests__/test-helpers.js';

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
      const ctx = createMockCommandContext();
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
      const ctx = createMockCommandContext();
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
      const ctx = createMockCommandContext();
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
      const ctx = createMockCommandContext();
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
      const ctx = createMockCommandContext();
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

    it('passes initialExpanded through for /context show', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Context breakdown - 42% used',
        data: {
          contextUsagePercentage: 42,
          initialExpanded: true,
          breakdown: { contextFiles: { tokens: 100, percent: 10 } },
        },
      });

      const cmd = makeCmd({
        name: '/context',
        meta: { inputType: 'panel' },
      });
      await dispatch(cmd, 'show', ctx);

      const calls = ctx._spies.setShowContextBreakdown!.mock.calls;
      expect(calls[0]?.[1]?.initialExpanded).toBe(true);
    });

    it('does not open panel for /context add result without breakdown', async () => {
      const ctx = createMockCommandContext();
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
      const ctx = createMockCommandContext();
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

    it('/chat new skips selection menu, resets messages and UI state', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro as any).newSession = mock(() =>
        Promise.resolve({
          sessionId: 'new-session-123',
          currentModel: { id: 'model-1', name: 'Model One' },
          currentAgent: { name: 'default' },
        })
      );

      const cmd = makeCmd({
        name: '/chat',
        meta: { inputType: 'selection', local: true },
      });
      await dispatch(cmd, 'new', ctx);

      expect((ctx.kiro.listSessions as any)?.mock?.calls?.length ?? 0).toBe(0);
      expect(ctx._spies.resetMessages!).toHaveBeenCalled();
      expect(ctx._spies.clearUIState!).toHaveBeenCalled();
    });

    it('/chat new with prompt sends message after session creation', async () => {
      const ctx = createMockCommandContext();
      let resolveNewSession: (v: any) => void;
      const newSessionPromise = new Promise((r) => {
        resolveNewSession = r;
      });
      (ctx.kiro as any).newSession = mock(() => newSessionPromise);

      const cmd = makeCmd({
        name: '/chat',
        meta: { inputType: 'selection', local: true },
      });
      await dispatch(cmd, 'new hello world', ctx);

      // Resolve the newSession promise
      resolveNewSession!({
        sessionId: 'new-session-456',
        currentModel: { id: 'model-1', name: 'Model One' },
        currentAgent: { name: 'default' },
      });
      // Wait for microtasks
      await new Promise((r) => setTimeout(r, 10));

      expect(ctx._spies.setSessionId!.mock.calls[0]?.[0]).toBe(
        'new-session-456'
      );
      expect(ctx._spies.sendMessage!.mock.calls[0]?.[0]).toBe('hello world');
    });

    it('/chat <sessionId> loads existing session without calling newSession', async () => {
      const ctx = createMockCommandContext();
      const newSessionSpy = mock(() => Promise.resolve({}));
      (ctx.kiro as any).newSession = newSessionSpy;
      let resolveLoad: (v: any) => void;
      const loadPromise = new Promise((r) => {
        resolveLoad = r;
      });
      (ctx.kiro as any).loadSession = mock(() => loadPromise);

      const cmd = makeCmd({
        name: '/chat',
        meta: { inputType: 'selection', local: true },
      });
      await dispatch(cmd, 'abc-123', ctx);

      resolveLoad!({
        sessionId: 'abc-123',
        currentModel: { id: 'model-1', name: 'Model One' },
      });
      await new Promise((r) => setTimeout(r, 10));

      // newSession should NOT have been called
      expect(newSessionSpy.mock.calls.length).toBe(0);
      // loadSession should have been called with the session ID
      expect((ctx.kiro as any).loadSession.mock.calls[0]?.[0]).toBe('abc-123');
      expect(ctx._spies.setSessionId!.mock.calls[0]?.[0]).toBe('abc-123');
    });
  });

  describe('prompt type commands', () => {
    it('sends message with /{cmdName} {args} and returns without calling executeCommand', async () => {
      const ctx = createMockCommandContext();
      const cmd = makeCmd({
        name: '/myPrompt',
        source: 'backend',
        meta: { type: 'prompt' },
      });

      await dispatch(cmd, 'some arguments', ctx);

      expect(ctx._spies.sendMessage!).toHaveBeenCalledTimes(1);
      expect(ctx._spies.sendMessage!.mock.calls[0]![0]).toBe(
        '/myPrompt some arguments'
      );
      // executeCommand should NOT have been called
      expect((ctx.kiro.executeCommand as any).mock.calls.length).toBe(0);
    });

    it('sends message without args when args is empty', async () => {
      const ctx = createMockCommandContext();
      const cmd = makeCmd({
        name: '/myPrompt',
        source: 'backend',
        meta: { type: 'prompt' },
      });

      await dispatch(cmd, '', ctx);

      expect(ctx._spies.sendMessage!).toHaveBeenCalledTimes(1);
      expect(ctx._spies.sendMessage!.mock.calls[0]![0]).toBe('/myPrompt');
      expect((ctx.kiro.executeCommand as any).mock.calls.length).toBe(0);
    });
  });

  describe('skill type commands', () => {
    it('sends message with /{cmdName} {args} and returns without calling executeCommand', async () => {
      const ctx = createMockCommandContext();
      const cmd = makeCmd({
        name: '/mySkill',
        source: 'backend',
        meta: { type: 'skill' },
      });

      await dispatch(cmd, 'skill args', ctx);

      expect(ctx._spies.sendMessage!).toHaveBeenCalledTimes(1);
      expect(ctx._spies.sendMessage!.mock.calls[0]![0]).toBe(
        '/mySkill skill args'
      );
      expect((ctx.kiro.executeCommand as any).mock.calls.length).toBe(0);
    });

    it('sends message without args when args is empty', async () => {
      const ctx = createMockCommandContext();
      const cmd = makeCmd({
        name: '/mySkill',
        source: 'backend',
        meta: { type: 'skill' },
      });

      await dispatch(cmd, '', ctx);

      expect(ctx._spies.sendMessage!).toHaveBeenCalledTimes(1);
      expect(ctx._spies.sendMessage!.mock.calls[0]![0]).toBe('/mySkill');
    });
  });

  describe('backend command error handling', () => {
    it('shows alert with error.message when executeCommand throws an Error', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockRejectedValue(
        new Error('Connection timeout')
      );

      const cmd = makeCmd({ name: '/deploy', source: 'backend' });
      await dispatch(cmd, 'prod', ctx);

      expect(ctx._spies.setLoadingMessage!).toHaveBeenCalledWith(null);
      expect(ctx._spies.showAlert!).toHaveBeenCalledTimes(1);
      expect(ctx._spies.showAlert!.mock.calls[0]![0]).toBe(
        'Connection timeout'
      );
      expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('error');
    });

    it('shows "Command failed" when executeCommand throws a non-Error', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockRejectedValue('string error');

      const cmd = makeCmd({ name: '/deploy', source: 'backend' });
      await dispatch(cmd, 'prod', ctx);

      expect(ctx._spies.showAlert!.mock.calls[0]![0]).toBe('Command failed');
      expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('error');
    });
  });

  describe('agent swap loading message', () => {
    it('shows loading message when cmdName is "agent" with non-create/edit args', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Agent switched',
        data: { agent: { name: 'coder' } },
      });

      const cmd = makeCmd({ name: '/agent', source: 'backend' });
      await dispatch(cmd, 'coder', ctx);

      // Should have called setLoadingMessage with agent name
      const loadingCalls = ctx._spies.setLoadingMessage!.mock.calls;
      expect(loadingCalls[0]![0]).toBe('Agent changing to coder');
      // Should clear loading after execution
      expect(loadingCalls[1]![0]).toBe(null);
    });

    it('strips "swap " prefix for display name', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Agent switched',
        data: { agent: { name: 'coder' } },
      });

      const cmd = makeCmd({ name: '/agent', source: 'backend' });
      await dispatch(cmd, 'swap coder', ctx);

      const loadingCalls = ctx._spies.setLoadingMessage!.mock.calls;
      expect(loadingCalls[0]![0]).toBe('Agent changing to coder');
    });

    it('does NOT show loading for "agent create" subcommand', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: '',
        data: undefined,
      });

      const cmd = makeCmd({ name: '/agent', source: 'backend' });
      await dispatch(cmd, 'create myAgent', ctx);

      const loadingCalls = ctx._spies.setLoadingMessage!.mock.calls;
      // Should not have the "Agent changing to" loading message
      const agentLoadingCalls = loadingCalls.filter(
        (c: unknown[]) =>
          typeof c[0] === 'string' && c[0].startsWith('Agent changing')
      );
      expect(agentLoadingCalls.length).toBe(0);
    });

    it('does NOT show loading for "agent edit" subcommand', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: '',
        data: undefined,
      });

      const cmd = makeCmd({ name: '/agent', source: 'backend' });
      await dispatch(cmd, 'edit myAgent', ctx);

      const loadingCalls = ctx._spies.setLoadingMessage!.mock.calls;
      const agentLoadingCalls = loadingCalls.filter(
        (c: unknown[]) =>
          typeof c[0] === 'string' && c[0].startsWith('Agent changing')
      );
      expect(agentLoadingCalls.length).toBe(0);
    });
  });

  describe('guide loading message', () => {
    it('shows "Switching agent..." loading message for /guide command', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Guide activated',
        data: undefined,
      });

      const cmd = makeCmd({ name: '/guide', source: 'backend' });
      await dispatch(cmd, 'some-guide', ctx);

      const loadingCalls = ctx._spies.setLoadingMessage!.mock.calls;
      expect(loadingCalls[0]![0]).toBe('Switching agent...');
      // Should clear loading after execution
      expect(loadingCalls[1]![0]).toBe(null);
    });
  });

  describe('result message display', () => {
    it('shows alert when result has a message and effect did not handle it', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Operation completed successfully',
        data: undefined,
      });

      // Use a command that has no special effect handler
      const cmd = makeCmd({ name: '/someCmd', source: 'backend' });
      await dispatch(cmd, 'arg', ctx);

      expect(ctx._spies.showAlert!).toHaveBeenCalledTimes(1);
      expect(ctx._spies.showAlert!.mock.calls[0]![0]).toBe(
        'Operation completed successfully'
      );
      expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('success');
      expect(ctx._spies.showAlert!.mock.calls[0]![2]).toBe(5000);
    });

    it('shows error alert when result is not successful', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: false,
        message: 'Something went wrong',
        data: undefined,
      });

      const cmd = makeCmd({ name: '/someCmd', source: 'backend' });
      await dispatch(cmd, 'arg', ctx);

      expect(ctx._spies.showAlert!).toHaveBeenCalledTimes(1);
      expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('error');
    });

    it('does not show alert when result has no message', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: '',
        data: undefined,
      });

      const cmd = makeCmd({ name: '/someCmd', source: 'backend' });
      await dispatch(cmd, 'arg', ctx);

      // showAlert should not be called (empty message is falsy)
      expect(ctx._spies.showAlert!.mock.calls.length).toBe(0);
    });
  });
});

import { describe, it, expect } from 'bun:test';
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
      expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('warning');
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
      expect(ctx._spies.showAlert!.mock.calls[0]?.[1]).toBe('warning');
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
    // /chat is intercepted by `handleV2Chat` (V2 engine) or
    // `handleChat` from kas-handlers (KAS engine) before the
    // dispatcher's selection / backend pipeline. Behavior coverage
    // lives in:
    //   - src/commands/v2-handlers/__tests__/chat.test.ts
    //   - src/commands/kas-handlers/__tests__/chat.test.ts
    //   - src/commands/__tests__/kas-intercept.test.ts (dispatcher
    //     wiring for both intercepts).
    it.skip('see v2-handlers/chat.test.ts and kas-intercept.test.ts', () => {});
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
      expect(ctx.kiro.sendChatSlashCommandTelemetry).toHaveBeenCalledWith({
        command: '/prompt',
        success: true,
      });
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

    it('surfaces string errors thrown by executeCommand', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockRejectedValue('string error');

      const cmd = makeCmd({ name: '/deploy', source: 'backend' });
      await dispatch(cmd, 'prod', ctx);

      // Raw string errors are surfaced as-is (useful info > generic fallback).
      expect(ctx._spies.showAlert!.mock.calls[0]![0]).toBe('string error');
      expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('error');
    });

    it('falls back to "Command failed" for null/undefined errors', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockRejectedValue(null);

      const cmd = makeCmd({ name: '/deploy', source: 'backend' });
      await dispatch(cmd, 'prod', ctx);

      expect(ctx._spies.showAlert!.mock.calls[0]![0]).toBe('Command failed');
      expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('error');
    });

    it('extracts data.details from ACP RequestError-shaped rejection', async () => {
      const ctx = createMockCommandContext();
      // Simulate ACP RequestError with structured data (e.g. KAS auth errors).
      const err = Object.assign(new Error('Internal error'), {
        code: -32603,
        data: { details: 'No auth token found.' },
      });
      (ctx.kiro.executeCommand as any).mockRejectedValue(err);

      const cmd = makeCmd({ name: '/deploy', source: 'backend' });
      await dispatch(cmd, 'prod', ctx);

      expect(ctx._spies.showAlert!.mock.calls[0]![0]).toBe(
        'No auth token found.'
      );
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

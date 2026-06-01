import { describe, it, expect, mock } from 'bun:test';
import { runEffect } from '../effects.js';
import type { SlashCommand } from '../../stores/app-store.js';
import { createMockCommandContext } from './test-helpers.js';

const clearCmd: SlashCommand = {
  name: '/clear',
  description: 'Clear conversation',
  source: 'local' as const,
  meta: { local: true },
};

describe('/clear effect — clearMessages', () => {
  it('preserves current agent when KAS returns a new session', () => {
    const setModeMock = mock(() => Promise.resolve());
    const ctx = createMockCommandContext({
      currentAgent: { name: 'kiro-dev-v2' },
      kiro: { setMode: setModeMock } as any,
    });

    const result = {
      success: true,
      message: '',
      data: {
        sessionId: 'new-session-123',
        currentModel: { id: 'auto', name: 'Auto' },
        currentAgent: { name: 'vibe' },
      },
    };

    runEffect(clearCmd, result, ctx, '');

    expect(ctx._spies.resetMessages!).toHaveBeenCalled();
    expect(ctx._spies.setSessionId!).toHaveBeenCalledWith('new-session-123');
    // Should re-apply previous agent, not the backend default
    expect(setModeMock).toHaveBeenCalledWith('kiro-dev-v2');
    expect(ctx._spies.setCurrentAgent!).toHaveBeenCalledWith({
      name: 'kiro-dev-v2',
    });
  });

  it('uses backend agent when no previous agent is set', () => {
    const ctx = createMockCommandContext({
      currentAgent: null,
      kiro: { setMode: mock(() => Promise.resolve()) } as any,
    });

    const result = {
      success: true,
      message: '',
      data: {
        sessionId: 'new-session-456',
        currentAgent: { name: 'vibe', welcomeMessage: 'Hello' },
      },
    };

    runEffect(clearCmd, result, ctx, '');

    expect(ctx._spies.setCurrentAgent!).toHaveBeenCalledWith({
      name: 'vibe',
      welcomeMessage: 'Hello',
    });
  });

  it('falls back to backend agent with alert when setMode fails', async () => {
    const setModeMock = mock(() => Promise.reject(new Error('network error')));
    const ctx = createMockCommandContext({
      currentAgent: { name: 'kiro-dev-v2' },
      kiro: { setMode: setModeMock } as any,
    });

    const result = {
      success: true,
      message: '',
      data: {
        sessionId: 'new-session-789',
        currentAgent: { name: 'vibe', welcomeMessage: 'Welcome' },
      },
    };

    runEffect(clearCmd, result, ctx, '');

    // Wait for the rejected promise to settle
    await new Promise((r) => setTimeout(r, 10));

    // Should have fallen back to backend agent and shown alert
    expect(ctx._spies.setCurrentAgent!).toHaveBeenNthCalledWith(2, {
      name: 'vibe',
      welcomeMessage: 'Welcome',
    });
    expect(ctx._spies.showAlert!).toHaveBeenCalledWith(
      'Failed to restore agent "kiro-dev-v2", reverted to default',
      'error',
      5000
    );
  });

  it('falls through to clearMessages when no sessionId (Rust mode)', () => {
    const ctx = createMockCommandContext();

    const result = { success: true, message: '', data: {} };

    runEffect(clearCmd, result, ctx, '');

    expect(ctx._spies.clearMessages!).toHaveBeenCalled();
    expect(ctx._spies.resetMessages!).not.toHaveBeenCalled();
  });
});

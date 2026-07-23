import { describe, it, expect, mock } from 'bun:test';
import { runEffect } from '../effects.js';
import type { SlashCommand } from '../../stores/app-store.js';
import {
  KAS_DEFAULT_AGENT_ID,
  KAS_DEFAULT_AGENT_NAME,
} from '../../constants/agents.js';
import { createMockCommandContext } from './test-helpers.js';

const clearCmd: SlashCommand = {
  name: '/clear',
  description: 'Clear conversation',
  source: 'local' as const,
  meta: { local: true },
};

describe('/clear effect — clearMessages', () => {
  it('preserves current agent when KAS returns a new session', () => {
    const setConfigOptionMock = mock(() => Promise.resolve());
    const ctx = createMockCommandContext({
      currentAgent: { name: 'kiro-dev-v2' },
      kiro: { setConfigOption: setConfigOptionMock } as any,
    });

    const result = {
      success: true,
      message: '',
      data: {
        sessionId: 'new-session-123',
        currentModel: { id: 'auto', name: 'Auto' },
        currentAgent: { name: KAS_DEFAULT_AGENT_ID },
      },
    };

    runEffect(clearCmd, result, ctx, '');

    expect(ctx._spies.resetMessages!).toHaveBeenCalled();
    expect(ctx._spies.setSessionId!).toHaveBeenCalledWith('new-session-123');
    // Should re-apply previous agent, not the backend default
    expect(setConfigOptionMock).toHaveBeenCalledWith('mode', 'kiro-dev-v2');
    expect(ctx._spies.setCurrentAgent!).toHaveBeenCalledWith({
      name: 'kiro-dev-v2',
    });
  });

  it('uses backend agent when no previous agent is set', () => {
    const ctx = createMockCommandContext({
      currentAgent: null,
      kiro: { setConfigOption: mock(() => Promise.resolve()) } as any,
    });

    const result = {
      success: true,
      message: '',
      data: {
        sessionId: 'new-session-456',
        currentAgent: { name: KAS_DEFAULT_AGENT_ID, welcomeMessage: 'Hello' },
      },
    };

    runEffect(clearCmd, result, ctx, '');

    expect(ctx._spies.setCurrentAgent!).toHaveBeenCalledWith({
      name: KAS_DEFAULT_AGENT_ID,
      welcomeMessage: 'Hello',
    });
  });

  it('falls back to backend agent with alert when setConfigOption fails', async () => {
    const setConfigOptionMock = mock(() =>
      Promise.reject(new Error('network error'))
    );
    const ctx = createMockCommandContext({
      currentAgent: { name: 'kiro-dev-v2' },
      kiro: { setConfigOption: setConfigOptionMock } as any,
    });

    const result = {
      success: true,
      message: '',
      data: {
        sessionId: 'new-session-789',
        currentAgent: {
          name: KAS_DEFAULT_AGENT_ID,
          welcomeMessage: 'Welcome',
        },
      },
    };

    runEffect(clearCmd, result, ctx, '');

    // Wait for the rejected promise to settle
    await new Promise((r) => setTimeout(r, 10));

    // Should have fallen back to backend agent and shown alert
    expect(ctx._spies.setCurrentAgent!).toHaveBeenNthCalledWith(2, {
      name: KAS_DEFAULT_AGENT_ID,
      welcomeMessage: 'Welcome',
    });
    expect(ctx._spies.showAlert!).toHaveBeenCalledWith(
      `Failed to restore agent "kiro-dev-v2", reverted to ${KAS_DEFAULT_AGENT_NAME}`,
      'error',
      5000
    );
  });

  it("resets the session origin to 'new' when clearing a cloud session", () => {
    const ctx = createMockCommandContext({
      currentAgent: null,
      cloudSessionActive: true,
      kiro: { setConfigOption: mock(() => Promise.resolve()) } as any,
    });

    const result = {
      success: true,
      message: '',
      data: { sessionId: 'new-session-after-clear' },
    };

    runEffect(clearCmd, result, ctx, '');

    expect(ctx._spies.beginKasSession!).toHaveBeenCalledWith('new');
  });

  it('does not reset the session origin when clearing a local session', () => {
    const ctx = createMockCommandContext({
      currentAgent: null,
      cloudSessionActive: false,
      kiro: { setConfigOption: mock(() => Promise.resolve()) } as any,
    });

    const result = {
      success: true,
      message: '',
      data: { sessionId: 'new-local-session-after-clear' },
    };

    runEffect(clearCmd, result, ctx, '');

    expect(ctx._spies.beginKasSession!).not.toHaveBeenCalled();
  });

  it('falls through to clearMessages when no sessionId (Rust mode)', () => {
    const ctx = createMockCommandContext();

    const result = { success: true, message: '', data: {} };

    runEffect(clearCmd, result, ctx, '');

    expect(ctx._spies.clearMessages!).toHaveBeenCalled();
    expect(ctx._spies.resetMessages!).not.toHaveBeenCalled();
  });
});

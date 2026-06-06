import { describe, it, expect } from 'bun:test';
import { runEffect } from '../effects.js';
import type { SlashCommand } from '../../stores/app-store.js';
import { createMockCommandContext } from './test-helpers.js';

const sessionIdCmd: SlashCommand = {
  name: '/session-id',
  description: 'Print the current session ID',
  source: 'local' as const,
  meta: { local: true },
};

describe('/session-id', () => {
  it('shows session ID as alert', () => {
    const ctx = createMockCommandContext({
      kiro: { sessionId: 'test-uuid-1234' } as any,
    });
    (ctx as any).getUiMode = () => 'lite';

    runEffect(sessionIdCmd, null, ctx, '');

    expect(ctx._spies.announceSystem!).toHaveBeenCalledWith(
      'Session ID: test-uuid-1234'
    );
  });

  it('shows "none" when no session', () => {
    const ctx = createMockCommandContext({
      kiro: { sessionId: undefined } as any,
    });
    (ctx as any).getUiMode = () => 'lite';

    runEffect(sessionIdCmd, null, ctx, '');

    expect(ctx._spies.announceSystem!).toHaveBeenCalledWith(
      'Session ID: none'
    );
  });
});

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
  // Single announceSystem call renders per-surface (lite scrollback / TUI toast);
  // the 10s hint keeps the TUI toast up long enough to read/copy the ID.
  it('announces the ID and resume hint', () => {
    const ctx = createMockCommandContext({
      kiro: { sessionId: 'test-uuid-1234' } as any,
    });

    runEffect(sessionIdCmd, null, ctx, '');

    expect(ctx._spies.announceSystem!).toHaveBeenCalledWith(
      'Session ID: test-uuid-1234\nResume with: kiro-cli --resume-id test-uuid-1234',
      true,
      10000
    );
    expect(ctx._spies.showAlert!).not.toHaveBeenCalled();
  });

  it('announces "none" when no session', () => {
    const ctx = createMockCommandContext({
      kiro: { sessionId: undefined } as any,
    });

    runEffect(sessionIdCmd, null, ctx, '');

    expect(ctx._spies.announceSystem!).toHaveBeenCalledWith(
      'Session ID: none',
      true,
      10000
    );
  });
});

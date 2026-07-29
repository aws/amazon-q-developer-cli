import { afterAll, describe, expect, it, mock } from 'bun:test';
import type { Kiro } from '../kiro.js';
import { createAppStore } from './app-store.js';

// Session transitions must blank the footer's session-scoped readings: a cloud
// session pushes no context_usage until its next turn completes, so stale
// values from the previous session would otherwise linger a whole turn.

mock.module('../kiro', () => ({
  Kiro: mock(() => ({})),
}));

afterAll(() => mock.restore());

function createStoreForResume() {
  const kiro = {
    sessionId: 'old-session',
    isCloudSessionActive: () => true,
    loadSession: mock(
      async (
        sessionId: string,
        _onHistory?: (e: unknown) => void,
        _options?: unknown
      ) => ({ sessionId })
    ),
    replayHistory: mock((_events: unknown[]) => true),
    getSessionRepositories: () => null,
  } as unknown as Kiro;
  const store = createAppStore({ kiro });
  store.setState({ isInitialized: true });
  return store;
}

describe('footer reset on session transition', () => {
  it('blanks context %, last-turn tokens, and goal status when a session loads', async () => {
    const store = createStoreForResume();

    store.setState({
      contextUsagePercent: 73,
      lastTurnTokens: { input: 1000, output: 500 } as never,
      goalStatus: {
        state: 'active',
        iteration: 2,
        maxIterations: 5,
      } as never,
    });
    expect(store.getState().contextUsagePercent).toBe(73);

    await store.getState().resumeSession('new-session', 'cloud');

    expect(store.getState().contextUsagePercent).toBeNull();
    expect(store.getState().lastTurnTokens).toBeNull();
    expect(store.getState().goalStatus).toBeNull();
    expect(store.getState().sessionId).toBe('new-session');
  });
});

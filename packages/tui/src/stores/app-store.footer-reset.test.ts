import { describe, expect, it, mock } from 'bun:test';
import type { Kiro } from '../kiro.js';
import { createAppStore } from './app-store.js';

// Loading a cloud session must clear stale footer metrics until the next turn reports replacements.

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

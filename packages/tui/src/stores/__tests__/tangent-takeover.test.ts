import { describe, it, expect, mock, afterAll } from 'bun:test';
import { AgentEventType, ContentType } from '../../types/agent-events';
import type { AgentStreamEvent } from '../../types/agent-events';

// Keep createAppStore's transitive kiro import cheap (mirrors the perf test).
mock.module('../../kiro', () => ({
  Kiro: mock(() => ({ cancel: mock(), close: mock() })),
}));

afterAll(() => {
  mock.restore();
});

import { createAppStore } from '../app-store';
import { Kiro } from '../../kiro';
import { switchToKasSession } from '../../commands/kas-handlers/session-switch';

/**
 * Guards the STORE-level takeover in switchToKasSession: creating several empty
 * tangents (each forks the current session carrying one inherited line, then
 * loads + replays it) must leave the message store with that line exactly once
 * — i.e. each switch RESETS the store rather than appending. If the
 * `resetMessages()` call were dropped, messages would accumulate and this fails.
 *
 * NOTE: this does NOT exercise the terminal <Static> scrollback wipe
 * (ctx.bumpLiteScrollbackClear()). That is a render-layer effect — Ink <Static>
 * painting to the terminal — not observable from the store, so reverting the
 * bump would NOT fail this test. Guarding the visible-duplication fix requires a
 * PTY render-harness test (TestCase + getSnapshotFormatted).
 */
describe('switchToKasSession — store-level takeover (no message accumulation)', () => {
  it('creating N empty tangents leaves the inherited line once in the store', async () => {
    const store = createAppStore({ kiro: new Kiro() });
    store.setState({ isInitialized: true });

    const INHERITED = 'the one and only line';

    // Simulate KAS session/load for a forked (empty) tangent: it replays the
    // inherited history (one user line) via the onHistory buffer callback.
    const kiro = {
      loadSession: (id: string, onHistory?: (e: AgentStreamEvent) => void) => {
        onHistory?.({
          type: AgentEventType.UserMessage,
          id: `user-${id}`,
          content: { type: ContentType.Text, text: INHERITED },
        } as unknown as AgentStreamEvent);
        return Promise.resolve({ sessionId: id });
      },
    };

    // Minimal ctx: only the message-affecting methods hit the real store;
    // everything else is a no-op. This is exactly what determines whether a
    // switch replaces or accumulates.
    const ctx = {
      kiro,
      setLoadingMessage: () => {},
      clearUIState: () => {},
      resetMessages: () => store.getState().resetMessages(),
      bumpLiteScrollbackClear: () => store.getState().bumpLiteScrollbackClear(),
      setSessionId: () => {},
      setCurrentModel: () => {},
      setCurrentAgent: () => {},
      addSystemMessage: () => {},
      createStreamEventHandler: () =>
        store.getState().createStreamEventHandler(),
      setTangentName: () => {},
      showAlert: () => {},
    } as unknown as Parameters<typeof switchToKasSession>[0];

    for (let i = 0; i < 5; i++) {
      await switchToKasSession(ctx, `tangent-${i}`, {
        loadingLabel: `Created tangent ${i}`,
        resolveTangentName: () => `tangent-${i}`,
        logTag: 'tangent',
      });
    }

    const copies = store
      .getState()
      .messages.filter((m) => String(m.content).includes(INHERITED));

    expect(copies.length).toBe(1);
  });
});

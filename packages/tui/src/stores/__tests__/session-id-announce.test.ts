// Drives the real announceSystem routing that command-level tests mock away.

import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore, MessageRole } from '../app-store';
import { Kiro } from '../../kiro';

mock.module('../../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(() => Promise.resolve()),
    close: mock(),
    sendChatSlashCommandTelemetry: mock(),
    sessionId: 'sess-abc',
  })),
}));

afterAll(() => {
  mock.restore();
});

const EXPECTED =
  'Session ID: sess-abc\nResume with: kiro-cli --resume-id sess-abc';

describe('/session-id announceSystem routing', () => {
  it('lite: adds a System scrollback message with the resume line', async () => {
    const store = createAppStore({ kiro: new Kiro(), uiMode: 'lite' });
    store.setState({ isInitialized: true });

    await store.getState().handleUserInput('/session-id');

    const sys = store
      .getState()
      .messages.find((m) => m.role === MessageRole.System);
    expect(sys).toBeDefined();
    expect((sys as { content: string }).content).toBe(EXPECTED);
  });

  it('tui: shows a transient toast with the resume line and a 10s read time', async () => {
    const store = createAppStore({ kiro: new Kiro(), uiMode: 'tui' });
    store.setState({ isInitialized: true });

    await store.getState().handleUserInput('/session-id');

    const alert = store.getState().transientAlert;
    expect(alert).not.toBeNull();
    expect(alert!.message).toBe(EXPECTED);
    expect(alert!.autoHideMs).toBe(10000);
  });
});

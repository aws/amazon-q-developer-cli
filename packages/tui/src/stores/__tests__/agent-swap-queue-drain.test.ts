/** Panel commands wait for an in-flight agent swap before opening. */

import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore } from '../app-store';
import { Kiro } from '../../kiro';

mock.module('../../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(() => Promise.resolve()),
    close: mock(),
    executeCommand: mock(
      () =>
        new Promise((resolve) =>
          setTimeout(
            () =>
              resolve({
                success: true,
                message: 'Agent switched',
                data: { agent: { name: 'coder' } },
              }),
            20
          )
        )
    ),
    sendModeChanged: mock(),
    recordSlashCommandInvocation: mock(),
    sendUiModeSessionStart: mock(),
    sendUiModeChanged: mock(),
    sendUiModeDefaultChanged: mock(),
    sessionId: 'test-session',
  })),
}));

afterAll(() => {
  mock.restore();
});

describe('agent swap with queued slash command (lite)', () => {
  it('opens a queued panel after the swap settles', async () => {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro, uiMode: 'lite' });
    store.setState({
      isInitialized: true,
      currentAgent: { name: 'kiro' },
      // Inject /agent into slashCommands the way CommandsUpdate would.
      slashCommands: [
        ...store.getState().slashCommands,
        {
          name: '/agent',
          description: 'Switch agent',
          source: 'backend' as const,
          meta: {},
        },
      ],
    });

    // Open the picker so executeCommandWithArg has an activeCommand to consume.
    // ActiveCommand.command is AvailableCommand, not SlashCommand — no `source`.
    store.setState({
      activeCommand: {
        command: {
          name: '/agent',
          description: 'Switch agent',
          meta: {},
        },
        options: [],
      },
    });

    // User picks 'coder' from the picker — store dispatches the agent swap
    // (this kicks off the dispatcher, which sets loadingMessage and awaits
    // the RPC, returning a Promise that resolves once the swap settles).
    const swapPromise = store.getState().executeCommandWithArg('coder');

    // Give the dispatcher a microtask to set loadingMessage and start awaiting.
    await new Promise((r) => setTimeout(r, 0));
    expect(store.getState().loadingMessage).toBe('Agent changing to coder');

    // While swap is in flight, queue a panel command for the settled session.
    await store.getState().handleUserInput('/settings');
    expect(store.getState().queuedMessages).toEqual(['/settings']);

    // The panel opens only after the swap's loading window closes.
    await swapPromise;
    await new Promise((r) => setTimeout(r, 30));

    expect(store.getState().queuedMessages).toEqual([]);
    expect(store.getState().showSettingsPanel).toBe(true);
  });
});

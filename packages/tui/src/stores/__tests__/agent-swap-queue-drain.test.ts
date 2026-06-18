/**
 * Regression test for the agent-swap + queued-slash-command bug.
 *
 * Repro: in lite mode, while a `/agent swap <name>` is in flight (the
 * dispatcher has set `loadingMessage` and is awaiting the RPC), the user
 * queues a picker command. When the swap RPC resolves, `processQueue` should
 * drain that command and open its menu — but the OUTER dispatcher's
 * "show result message" step then fires `ctx.showAlert(result.message,
 * 'success', 5000)` for the agent swap. Lite's `showAlert` override in
 * `executeCommandWithArg` used to call `set({ activeCommand: null })`
 * unconditionally, clobbering the just-opened menu.
 *
 * The fix narrows the override: `activeCommand` is only cleared on
 * warning/error status. Success alerts are silently dropped already
 * (lite has no NotificationBar), so there's no menu-display invariant
 * for them to enforce.
 */

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
    sendChatSlashCommandTelemetry: mock(),
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
  it('opens the queued picker menu after the swap settles', async () => {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro, uiMode: 'lite' });
    store.setState({
      isInitialized: true,
      currentAgent: { name: 'kiro' },
      // Inject /agent into slashCommands the way CommandsUpdate would, plus
      // a local panel command to stand in for picker-opening commands from
      // later slices.
      slashCommands: [
        ...store.getState().slashCommands,
        {
          name: '/agent',
          description: 'Switch agent',
          source: 'backend' as const,
          meta: {},
        },
        {
          name: '/pick',
          description: 'Open a picker',
          source: 'local' as const,
          meta: { local: true, inputType: 'panel' as const },
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

    // While swap is in flight, queue a picker command.
    await store.getState().handleUserInput('/pick');
    expect(store.getState().queuedMessages).toEqual(['/pick']);

    // Let the swap complete: the RPC resolves, setLoadingMessage(null) fires
    // processQueue inline, processQueue drains /pick and opens its menu,
    // then the dispatcher's "show result message" step runs the success alert.
    await swapPromise;
    await new Promise((r) => setTimeout(r, 30));

    expect(store.getState().queuedMessages).toEqual([]);
    // The bug: success alert clobbered activeCommand. With the fix, the
    // queued picker menu survives.
    expect(store.getState().activeCommand?.command.name).toBe('/pick');
  });
});

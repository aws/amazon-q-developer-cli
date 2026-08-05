import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore, MessageRole } from './app-store';
import { Kiro } from '../kiro';

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

afterAll(() => {
  mock.restore();
});

function storeWithSendSpy() {
  const store = createAppStore({ kiro: new Kiro() });
  const sendMessage = mock(async (_text: string) => {});
  // The picker only opens on an initialized, idle session; reflect that so the
  // rollback heuristic (which is skipped when a turn couldn't have fired) runs.
  store.setState({ sendMessage, isInitialized: true } as never);
  return { store, sendMessage };
}

describe('submitRepoPicker', () => {
  it('resumes a blocked queue after submitting an unchanged selection', async () => {
    const { store } = storeWithSendSpy();
    const processQueue = mock(async () => {});
    store.setState({
      queuedMessages: ['/tui'],
      processQueue,
      showRepoPicker: true,
    } as never);

    await store.getState().submitRepoPicker([]);
    await new Promise((resolve) => queueMicrotask(resolve));

    expect(processQueue).toHaveBeenCalledTimes(1);
  });

  it('closes the picker and clears its resources without sending when nothing is selected', async () => {
    const { store, sendMessage } = storeWithSendSpy();
    store
      .getState()
      .setShowRepoPicker(true, [
        { providerType: 'GITHUB', name: 'owner/app' } as never,
      ]);
    await store.getState().submitRepoPicker([]);
    expect(store.getState().showRepoPicker).toBe(false);
    expect(store.getState().repoPickerResources).toEqual([]);
    expect(sendMessage).not.toHaveBeenCalled();
  });

  it('sends the single-repo clone instruction for one selection', async () => {
    const { store, sendMessage } = storeWithSendSpy();
    store.getState().setShowRepoPicker(true, []);
    await store.getState().submitRepoPicker(['owner/app']);
    expect(sendMessage).toHaveBeenCalledWith(
      'Clone the repository owner/app into the workspace.'
    );
    expect(store.getState().showRepoPicker).toBe(false);
  });

  it('sends one multi-repo clone instruction for several selections', async () => {
    const { store, sendMessage } = storeWithSendSpy();
    await store.getState().submitRepoPicker(['owner/app', 'owner/lib']);
    expect(sendMessage).toHaveBeenCalledWith(
      'Clone the following repositories into the workspace: owner/app, owner/lib.'
    );
    expect(sendMessage).toHaveBeenCalledTimes(1);
  });

  it('treats a blank-only selection like an empty one (no send)', async () => {
    const { store, sendMessage } = storeWithSendSpy();
    await store.getState().submitRepoPicker(['  ', '']);
    expect(sendMessage).not.toHaveBeenCalled();
    expect(store.getState().showRepoPicker).toBe(false);
  });

  it('does not re-send the clone instruction when the selection is unchanged', async () => {
    const { store, sendMessage } = storeWithSendSpy();
    await store.getState().submitRepoPicker(['owner/app']);
    expect(sendMessage).toHaveBeenCalledTimes(1);
    // Reopen and submit the same pre-checked set (e.g. open /repo, press esc).
    await store.getState().submitRepoPicker(['owner/app']);
    expect(sendMessage).toHaveBeenCalledTimes(1);
  });

  it('sends the clone instruction only for repos added since the last submission', async () => {
    const { store, sendMessage } = storeWithSendSpy();
    await store.getState().submitRepoPicker(['owner/app']);
    await store.getState().submitRepoPicker(['owner/app', 'owner/lib']);
    expect(sendMessage).toHaveBeenLastCalledWith(
      'Clone the repository owner/lib into the workspace.'
    );
    expect(store.getState().attachedRepos).toEqual(['owner/app', 'owner/lib']);
  });

  it('deselecting a repo sends a removal instruction for it', async () => {
    const { store, sendMessage } = storeWithSendSpy();
    await store.getState().submitRepoPicker(['owner/app', 'owner/lib']);
    expect(sendMessage).toHaveBeenCalledTimes(1);
    await store.getState().submitRepoPicker(['owner/app']);
    expect(sendMessage).toHaveBeenCalledTimes(2);
    const removalMsg = (sendMessage.mock.calls[1] as string[])[0]!;
    expect(removalMsg).toContain('Remove the repository owner/lib');
    expect(store.getState().attachedRepos).toEqual(['owner/app']);
  });

  it('a mixed re-submission clones the added and removes the dropped in one turn', async () => {
    const { store, sendMessage } = storeWithSendSpy();
    await store.getState().submitRepoPicker(['owner/app']);
    await store.getState().submitRepoPicker(['owner/lib']);
    const msg = (sendMessage.mock.calls[1] as string[])[0]!;
    expect(msg).toContain('Clone the repository owner/lib');
    expect(msg).toContain('Remove the repository owner/app');
  });
});

describe('submitRepoPicker — failed clone turn rolls the footer back', () => {
  it('reverts footer + attached set when the clone turn errors', async () => {
    const { store } = storeWithSendSpy();
    // sendMessage that "fails": the turn machinery marks lastTurnErrored.
    const sendMessage = mock(async (_text: string) => {
      store.setState({ lastTurnErrored: true } as never);
    });
    store.setState({ sendMessage } as never);
    store
      .getState()
      .setShowRepoPicker(true, [
        { providerType: 'GITHUB', name: 'owner/app', defaultBranch: 'main' },
      ] as never);
    await store.getState().submitRepoPicker(['owner/app']);
    expect(sendMessage).toHaveBeenCalled();
    // Optimistic footer reverted: nothing actually attached.
    expect(store.getState().cloudRepo).toBeNull();
    expect(store.getState().attachedRepos).toEqual([]);
  });

  it('keeps the footer when the clone turn succeeds', async () => {
    const { store, sendMessage } = storeWithSendSpy();
    store
      .getState()
      .setShowRepoPicker(true, [
        { providerType: 'GITHUB', name: 'owner/app', defaultBranch: 'main' },
      ] as never);
    await store.getState().submitRepoPicker(['owner/app']);
    expect(sendMessage).toHaveBeenCalled();
    expect(store.getState().cloudRepo).toBe('owner/app');
    expect(store.getState().attachedRepos).toEqual(['owner/app']);
  });

  it('drops only the failed repo on a mixed success/failure turn', async () => {
    const { store } = storeWithSendSpy();
    // The turn clones both repos: one succeeds, one errors. The failed one
    // must not stay attached behind the unrelated success.
    const sendMessage = mock(async (_text: string) => {
      store.setState(
        (s) =>
          ({
            messages: [
              ...s.messages,
              {
                id: 't-ok',
                role: MessageRole.ToolUse,
                name: 'execute_bash',
                content:
                  '{"command":"git clone https://github.com/owner/app.git"}',
                result: { status: 'success' },
              },
              {
                id: 't-bad',
                role: MessageRole.ToolUse,
                name: 'execute_bash',
                content:
                  '{"command":"git clone https://github.com/owner/bad.git"}',
                result: { status: 'error' },
              },
            ],
          }) as never
      );
    });
    store.setState({ sendMessage } as never);
    store.getState().setShowRepoPicker(true, [
      { providerType: 'GITHUB', name: 'owner/app', defaultBranch: 'main' },
      { providerType: 'GITHUB', name: 'owner/bad', defaultBranch: 'main' },
    ] as never);
    await store.getState().submitRepoPicker(['owner/app', 'owner/bad']);
    expect(store.getState().attachedRepos).toEqual(['owner/app']);
    expect(store.getState().cloudRepo).toBe('owner/app');
  });

  it('promotes the surviving repo to primary (with its branch) when the first clone failed', async () => {
    const { store } = storeWithSendSpy();
    const sendMessage = mock(async (_text: string) => {
      store.setState(
        (s) =>
          ({
            messages: [
              ...s.messages,
              {
                id: 't-bad',
                role: MessageRole.ToolUse,
                name: 'execute_bash',
                content:
                  '{"command":"git clone https://github.com/owner/bad.git"}',
                result: { status: 'error' },
              },
              {
                id: 't-ok',
                role: MessageRole.ToolUse,
                name: 'execute_bash',
                content:
                  '{"command":"git clone https://github.com/owner/lib.git"}',
                result: { status: 'success' },
              },
            ],
          }) as never
      );
    });
    store.setState({ sendMessage } as never);
    store.getState().setShowRepoPicker(true, [
      { providerType: 'GITHUB', name: 'owner/bad', defaultBranch: 'main' },
      { providerType: 'GITHUB', name: 'owner/lib', defaultBranch: 'dev' },
    ] as never);
    await store.getState().submitRepoPicker(['owner/bad', 'owner/lib']);
    expect(store.getState().attachedRepos).toEqual(['owner/lib']);
    expect(store.getState().cloudRepo).toBe('owner/lib');
    expect(store.getState().cloudBranch).toBe('dev');
  });
});

describe('setShowRepoPicker — duplicate catalog rows', () => {
  it('dedupes rows by name (first sighting wins) so one checkbox maps to one row', () => {
    const { store } = storeWithSendSpy();
    store.getState().setShowRepoPicker(true, [
      { providerType: 'MIDWAY', name: 'KiroCliKasMigration' },
      { providerType: 'MIDWAY', name: 'KiroCliKasMigration' },
      { providerType: 'MIDWAY', name: 'KiroClientConfigSchemas' },
      { providerType: 'MIDWAY', name: 'KiroCliKasMigration' },
    ] as never);
    const names = store
      .getState()
      .repoPickerResources.map((r: { name: string }) => r.name);
    expect(names).toEqual(['KiroCliKasMigration', 'KiroClientConfigSchemas']);
  });
});

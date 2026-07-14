import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore } from './app-store';
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
  store.setState({ sendMessage } as never);
  return { store, sendMessage };
}

describe('submitRepoPicker', () => {
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

  it('deselecting a repo updates the attached set without sending', async () => {
    const { store, sendMessage } = storeWithSendSpy();
    await store.getState().submitRepoPicker(['owner/app', 'owner/lib']);
    expect(sendMessage).toHaveBeenCalledTimes(1);
    await store.getState().submitRepoPicker(['owner/app']);
    expect(sendMessage).toHaveBeenCalledTimes(1);
    expect(store.getState().attachedRepos).toEqual(['owner/app']);
  });
});

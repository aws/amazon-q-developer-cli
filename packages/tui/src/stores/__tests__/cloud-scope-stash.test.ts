/* eslint-disable @typescript-eslint/ban-ts-comment */
// @ts-nocheck
import { describe, it, expect, mock, afterAll } from 'bun:test';

mock.module('../../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

afterAll(() => {
  mock.restore();
});

const { createAppStore } = await import('../app-store');
const { Kiro } = await import('../../kiro');
const { InterruptMode } = await import('../../constants/interrupt-mode');

function makeStore() {
  const store = createAppStore({ kiro: new Kiro() });
  store.setState({ isInitialized: true });
  return store;
}

// Regression for: "When I selected repo in session A, switched to another
// session, and switched back to A, the footer showed no repo." The scope is
// stashed per session id on switch-away and restored on switch-back.
describe('cloud scope stash/restore across session switches', () => {
  it('restores the footer scope when switching back to a session', () => {
    const store = makeStore();
    store.setState({
      cloudRepo: 'kiro-team/kiro-cli',
      cloudBranch: 'main',
      cloudExtraRepos: 2,
      attachedRepos: ['kiro-team/kiro-cli', 'a/b', 'c/d'],
    });

    // Switch away from session A: stash, then clear.
    store.getState().stashCloudSessionScope('session-a');
    store.getState().resetCloudSessionScope();
    expect(store.getState().cloudRepo).toBeNull();
    expect(store.getState().attachedRepos).toEqual([]);

    // Switch back: restore hydrates the footer without a re-fetch.
    const restored = store.getState().restoreCloudSessionScope('session-a');
    expect(restored).toBe(true);
    expect(store.getState().cloudRepo).toBe('kiro-team/kiro-cli');
    expect(store.getState().cloudBranch).toBe('main');
    expect(store.getState().cloudExtraRepos).toBe(2);
    expect(store.getState().attachedRepos).toEqual([
      'kiro-team/kiro-cli',
      'a/b',
      'c/d',
    ]);
  });

  it('returns false (and leaves state cleared) for a session with no stash', () => {
    const store = makeStore();
    store.getState().resetCloudSessionScope();
    expect(store.getState().restoreCloudSessionScope('never-seen')).toBe(false);
    expect(store.getState().cloudRepo).toBeNull();
  });

  it('drops the stash when the session has nothing bound', () => {
    // A session whose repos were all detached must not resurrect an older
    // snapshot on the next switch-back.
    const store = makeStore();
    store.setState({
      cloudRepo: 'kiro-team/kiro-cli',
      attachedRepos: ['kiro-team/kiro-cli'],
    });
    store.getState().stashCloudSessionScope('session-a');
    // Later, everything is detached and we switch away again.
    store.setState({
      cloudRepo: null,
      cloudBranch: null,
      cloudExtraRepos: 0,
      attachedRepos: [],
    });
    store.getState().stashCloudSessionScope('session-a');
    expect(store.getState().restoreCloudSessionScope('session-a')).toBe(false);
  });

  it('is a no-op for a null session id', () => {
    const store = makeStore();
    store.setState({ cloudRepo: 'x/y', attachedRepos: ['x/y'] });
    store.getState().stashCloudSessionScope(null);
    expect(store.getState().cloudScopeBySession.size).toBe(0);
  });
});

// Regression for: typing /chat opened the picker titled "/sessions". The
// typed command is threaded through as the panel title.
describe('session picker title echoes the typed command', () => {
  it('uses the invokedAs title when provided', () => {
    const store = makeStore();
    store.getState().setShowSessionPicker(true, [], '/chat');
    expect(store.getState().sessionPickerTitle).toBe('/chat');
    store.getState().setShowSessionPicker(false);
    expect(store.getState().sessionPickerTitle).toBe('/sessions');
  });

  it('defaults to /sessions when no command is given', () => {
    const store = makeStore();
    store.getState().setShowSessionPicker(true, []);
    expect(store.getState().sessionPickerTitle).toBe('/sessions');
  });
});

describe('applyRepoFooter', () => {
  it('projects the first repo + (+N others) + attached set', () => {
    const store = makeStore();
    store.getState().applyRepoFooter(['a/b', 'c/d', 'e/f']);
    expect(store.getState().cloudRepo).toBe('a/b');
    expect(store.getState().cloudExtraRepos).toBe(2);
    expect(store.getState().attachedRepos).toEqual(['a/b', 'c/d', 'e/f']);
  });

  it('clears the footer (and branch) for an empty set', () => {
    const store = makeStore();
    store.setState({ cloudBranch: 'main' });
    store.getState().applyRepoFooter([]);
    expect(store.getState().cloudRepo).toBeNull();
    expect(store.getState().cloudExtraRepos).toBe(0);
    expect(store.getState().attachedRepos).toEqual([]);
    expect(store.getState().cloudBranch).toBeNull();
  });
});

describe('steering stays available in cloud sessions', () => {
  it('keeps the active interrupt mode across cloud entry and exit', () => {
    const store = makeStore();
    store.setState({ activeInterruptMode: InterruptMode.STEER });
    store.getState().setCloudSessionActive(true);
    expect(store.getState().activeInterruptMode).toBe(InterruptMode.STEER);
    store.getState().setCloudSessionActive(false);
    expect(store.getState().activeInterruptMode).toBe(InterruptMode.STEER);
  });

  it('allows toggling to steer while a cloud session is active', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: InterruptMode.QUEUE,
      cloudSessionActive: true,
    });
    store.getState().toggleInterruptMode();
    expect(store.getState().activeInterruptMode).toBe(InterruptMode.STEER);
  });

  it('accepts an explicit steer mode set while cloud is active', () => {
    const store = makeStore();
    store.setState({
      activeInterruptMode: InterruptMode.QUEUE,
      cloudSessionActive: true,
    });
    store.getState().setActiveInterruptMode(InterruptMode.STEER);
    expect(store.getState().activeInterruptMode).toBe(InterruptMode.STEER);
  });
});

import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore } from './app-store';
import { Kiro } from '../kiro';

// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, ['../kiro']);

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    sendMessage: mock(),
    steerMessage: mock(),
    clearSteering: mock(),
    setWorkflowNotificationDelivery: mock(() => Promise.resolve()),
    cancel: mock(),
    close: mock(),
  })),
}));

afterAll(() => {
  mock.restore();
});

function createTestStore() {
  const mockKiro = new Kiro();
  const store = createAppStore({ kiro: mockKiro });
  store.setState({ isInitialized: true, sessionId: 'test-session' });
  return store;
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

async function flushAsyncWork() {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe('toggleInterruptMode', () => {
  describe('Steering → Queueing', () => {
    it('flips mode from steering to queuing', () => {
      const store = createTestStore();
      store.setState({ activeInterruptMode: 'steer' });

      store.getState().toggleInterruptMode();

      expect(store.getState().activeInterruptMode).toBe('queue');
    });

    it('synchronizes workflow notification delivery with KAS', async () => {
      const store = createTestStore();
      store.setState({ activeInterruptMode: 'steer' });

      store.getState().toggleInterruptMode();
      await flushAsyncWork();

      expect(
        store.getState().kiro.setWorkflowNotificationDelivery
      ).toHaveBeenCalledWith('queue');
    });

    it('rolls back when KAS rejects the policy update', async () => {
      const store = createTestStore();
      const syncPolicy = mock(() =>
        Promise.reject(new Error('extension unavailable'))
      );
      (
        store.getState().kiro as unknown as {
          setWorkflowNotificationDelivery: typeof syncPolicy;
        }
      ).setWorkflowNotificationDelivery = syncPolicy;
      store.setState({ activeInterruptMode: 'steer' });

      store.getState().toggleInterruptMode();
      await flushAsyncWork();

      expect(store.getState().activeInterruptMode).toBe('steer');
      expect(store.getState().transientAlert).toMatchObject({
        message: 'Failed to switch interrupt mode',
        status: 'error',
      });
    });

    it('ignores a stale rejection while a newer toggle is pending', async () => {
      const store = createTestStore();
      const first = deferred<void>();
      const second = deferred<void>();
      const syncPolicy = mock((mode: string) =>
        mode === 'queue' ? first.promise : second.promise
      );
      (
        store.getState().kiro as unknown as {
          setWorkflowNotificationDelivery: typeof syncPolicy;
        }
      ).setWorkflowNotificationDelivery = syncPolicy;
      store.setState({ activeInterruptMode: 'steer' });

      store.getState().toggleInterruptMode();
      store.getState().toggleInterruptMode();
      await flushAsyncWork();
      expect(syncPolicy).toHaveBeenCalledTimes(1);

      first.reject(new Error('stale update failed'));
      await flushAsyncWork();
      expect(syncPolicy).toHaveBeenCalledTimes(2);

      second.resolve();
      await flushAsyncWork();

      expect(store.getState().activeInterruptMode).toBe('steer');
      expect(store.getState().transientAlert?.status).toBe('info');
    });

    it('rolls back to the last confirmed mode when the latest toggle fails', async () => {
      const store = createTestStore();
      const first = deferred<void>();
      const second = deferred<void>();
      const syncPolicy = mock((mode: string) =>
        mode === 'queue' ? first.promise : second.promise
      );
      (
        store.getState().kiro as unknown as {
          setWorkflowNotificationDelivery: typeof syncPolicy;
        }
      ).setWorkflowNotificationDelivery = syncPolicy;
      store.setState({ activeInterruptMode: 'steer' });

      store.getState().toggleInterruptMode();
      store.getState().toggleInterruptMode();
      await flushAsyncWork();

      first.resolve();
      await flushAsyncWork();
      second.reject(new Error('latest update failed'));
      await flushAsyncWork();

      expect(store.getState().activeInterruptMode).toBe('queue');
      expect(store.getState().transientAlert).toMatchObject({
        message: 'Failed to switch interrupt mode',
        status: 'error',
      });
    });

    it('does NOT modify pendingSteerContent (no migration)', () => {
      const store = createTestStore();
      store.setState({
        activeInterruptMode: 'steer',
        pendingSteerContent: 'fix the bug\n\nadd tests',
      });

      store.getState().toggleInterruptMode();

      // Pending steer stays untouched
      expect(store.getState().pendingSteerContent).toBe(
        'fix the bug\n\nadd tests'
      );
    });

    it('does NOT modify queuedMessages (no migration)', () => {
      const store = createTestStore();
      store.setState({
        activeInterruptMode: 'steer',
        pendingSteerContent: 'some message',
        queuedMessages: ['existing'],
      });

      store.getState().toggleInterruptMode();

      // Queue stays untouched
      expect(store.getState().queuedMessages).toEqual(['existing']);
    });

    it('does NOT call clearSteering', () => {
      const store = createTestStore();
      const mockClearSteering = mock(() => Promise.resolve());
      (store.getState().kiro as any).clearSteering = mockClearSteering;

      store.setState({
        activeInterruptMode: 'steer',
        pendingSteerContent: 'some message',
      });

      store.getState().toggleInterruptMode();

      expect(mockClearSteering).not.toHaveBeenCalled();
    });

    it('shows transient alert indicating Queue mode', () => {
      const store = createTestStore();
      store.setState({ activeInterruptMode: 'steer' });

      store.getState().toggleInterruptMode();

      expect(store.getState().transientAlert).toMatchObject({
        message: 'Switched to Queue mode',
        status: 'info',
      });
    });
  });

  describe('Queueing → Steering', () => {
    it('flips mode from queuing to steering', () => {
      const store = createTestStore();
      store.setState({ activeInterruptMode: 'queue' });

      store.getState().toggleInterruptMode();

      expect(store.getState().activeInterruptMode).toBe('steer');
    });

    it('does NOT modify queuedMessages (no migration)', () => {
      const store = createTestStore();
      store.setState({
        activeInterruptMode: 'queue',
        queuedMessages: ['message one', 'message two'],
      });

      store.getState().toggleInterruptMode();

      // Queue stays untouched — messages will drain at end-of-turn
      expect(store.getState().queuedMessages).toEqual([
        'message one',
        'message two',
      ]);
    });

    it('does NOT call steerMessage', () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;

      store.setState({
        activeInterruptMode: 'queue',
        queuedMessages: ['message one', 'message two'],
      });

      store.getState().toggleInterruptMode();

      expect(mockSteerMessage).not.toHaveBeenCalled();
    });

    it('shows transient alert indicating Steer mode', () => {
      const store = createTestStore();
      store.setState({ activeInterruptMode: 'queue' });

      store.getState().toggleInterruptMode();

      expect(store.getState().transientAlert).toMatchObject({
        message: 'Switched to Steer mode',
        status: 'info',
      });
    });
  });

  describe('Coexistence', () => {
    it('both pendingSteerContent and queuedMessages can be non-empty after toggle', () => {
      const store = createTestStore();
      store.setState({
        activeInterruptMode: 'steer',
        pendingSteerContent: 'a steer message',
        queuedMessages: ['queued item 1', 'queued item 2'],
      });

      store.getState().toggleInterruptMode();

      expect(store.getState().activeInterruptMode).toBe('queue');
      expect(store.getState().pendingSteerContent).toBe('a steer message');
      expect(store.getState().queuedMessages).toEqual([
        'queued item 1',
        'queued item 2',
      ]);
    });
  });
});

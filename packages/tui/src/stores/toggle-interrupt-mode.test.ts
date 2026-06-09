import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore } from './app-store';
import { Kiro } from '../kiro';

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    sendMessage: mock(),
    steerMessage: mock(),
    clearSteering: mock(),
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

describe('toggleInterruptMode', () => {
  describe('Steering → Queueing', () => {
    it('flips mode from steering to queuing', () => {
      const store = createTestStore();
      store.setState({ activeInterruptMode: 'steer' });

      store.getState().toggleInterruptMode();

      expect(store.getState().activeInterruptMode).toBe('queue');
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

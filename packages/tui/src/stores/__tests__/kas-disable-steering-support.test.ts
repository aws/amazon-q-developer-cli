/**
 * Unit tests gating mid-turn steering to the v2 agent engine.
 *
 * KAS (surfaced as "v3") does not implement backend steering yet, so it is
 * pinned to QUEUE-only behavior. v2 keeps the full STEER/QUEUE dual mode.
 *
 * Verifies:
 * 1. Init pins activeInterruptMode to QUEUE for KAS even when the persisted
 *    setting is `steer`.
 * 2. v2 still honors the persisted setting.
 * 3. toggleInterruptMode is a no-op + alert on KAS; flips on v2.
 * 4. setActiveInterruptMode is a no-op (no alert) on KAS; sets on v2.
 * 5. queueMessage on KAS appends to the local queue and never calls steerMessage.
 * 6. clearSteerMessage on KAS never calls the backend clearSteering.
 */

import { describe, it, expect, mock, beforeEach, afterAll } from 'bun:test';

mock.module('../../utils/cli-settings', () => ({
  readStringSetting: (key: string, def: string) =>
    key === 'chat.defaultInterruptBehavior' ? 'steer' : def,
  readBoolSetting: (_key: string, def: boolean) => def,
}));

mock.module('../../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(() => Promise.resolve()),
    steerMessage: mock(() => Promise.resolve()),
    clearSteering: mock(() => Promise.resolve()),
    close: mock(),
  })),
}));

import { createAppStore } from '../app-store';
import { Kiro } from '../../kiro';

afterAll(() => {
  mock.restore();
});

describe('init pins interrupt mode for KAS', () => {
  let mockKiro: any;

  beforeEach(() => {
    mockKiro = new Kiro();
  });

  it('pins activeInterruptMode to queue for KAS despite persisted steer setting', () => {
    const store = createAppStore({ kiro: mockKiro, agentEngine: 'kas' });
    expect(store.getState().activeInterruptMode).toBe('queue');
  });

  it('honors the persisted setting for v2', () => {
    const store = createAppStore({ kiro: mockKiro, agentEngine: 'v2' });
    expect(store.getState().activeInterruptMode).toBe('steer');
  });
});

describe('toggleInterruptMode is gated for KAS', () => {
  let mockKiro: any;

  beforeEach(() => {
    mockKiro = new Kiro();
  });

  it('is a no-op on KAS and shows the unsupported alert', () => {
    const store = createAppStore({ kiro: mockKiro, agentEngine: 'kas' });

    store.getState().toggleInterruptMode();

    expect(store.getState().activeInterruptMode).toBe('queue');
    expect(store.getState().transientAlert?.message).toBe(
      'Steering is currently unsupported for v3'
    );
  });

  it('flips the mode on v2', () => {
    const store = createAppStore({ kiro: mockKiro, agentEngine: 'v2' });
    // v2 starts at 'steer' (persisted setting)
    store.getState().toggleInterruptMode();
    expect(store.getState().activeInterruptMode).toBe('queue');
  });
});

describe('setActiveInterruptMode is gated for KAS', () => {
  let mockKiro: any;

  beforeEach(() => {
    mockKiro = new Kiro();
  });

  it('is a no-op on KAS with no alert', () => {
    const store = createAppStore({ kiro: mockKiro, agentEngine: 'kas' });

    store.getState().setActiveInterruptMode('steer' as any);

    expect(store.getState().activeInterruptMode).toBe('queue');
    expect(store.getState().transientAlert).toBeFalsy();
  });

  it('sets the mode on v2', () => {
    const store = createAppStore({ kiro: mockKiro, agentEngine: 'v2' });

    store.getState().setActiveInterruptMode('queue' as any);

    expect(store.getState().activeInterruptMode).toBe('queue');
  });
});

describe('queueMessage never steers on KAS', () => {
  let mockKiro: any;

  beforeEach(() => {
    mockKiro = new Kiro();
  });

  it('appends to the local queue and never calls steerMessage', () => {
    const store = createAppStore({ kiro: mockKiro, agentEngine: 'kas' });
    store.setState({ isInitialized: true, sessionId: 'session-1' });

    store.getState().queueMessage('do the thing');

    expect(store.getState().queuedMessages).toEqual(['do the thing']);
    expect(mockKiro.steerMessage).not.toHaveBeenCalled();
  });
});

describe('clearSteerMessage never hits the backend on KAS', () => {
  let mockKiro: any;

  beforeEach(() => {
    mockKiro = new Kiro();
  });

  it('clears locally without calling clearSteering on KAS', () => {
    const store = createAppStore({ kiro: mockKiro, agentEngine: 'kas' });
    store.setState({
      isInitialized: true,
      sessionId: 'session-1',
      pendingSteerContent: 'buffered text',
    });

    store.getState().clearSteerMessage();

    expect(store.getState().pendingSteerContent).toBeNull();
    expect(mockKiro.clearSteering).not.toHaveBeenCalled();
  });
});

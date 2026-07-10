import { describe, it, expect, mock, afterEach } from 'bun:test';
import { mkdirSync, writeFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { createAppStore } from './app-store';
import { Kiro } from '../kiro';
import { AgentEventType } from '../types/agent-events';

function makeStore() {
  return createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
}

// Mirrors the session-start seeding done by the real callers (index.tsx,
// /chat, rewind): begin the session via the store action, then record the
// explicit --effort launch flag (boot-only state).
function beginSession(
  store: ReturnType<typeof makeStore>,
  origin: 'new' | 'resumed',
  effortExplicit = false
) {
  store.getState().beginKasSession(origin);
  store.setState((s) => ({ kas: { ...s.kas, effortExplicit } }));
}

describe('kas config-option caches', () => {
  it('defaults to empty lists', () => {
    const store = makeStore();
    expect(store.getState().kas.availableModels).toEqual([]);
    expect(store.getState().kas.availableAgents).toEqual([]);
    expect(store.getState().kas.availableEfforts).toEqual([]);
  });

  it('handleKasModelConfigEvent replaces the model and effort lists', () => {
    const store = makeStore();
    store.getState().handleKasModelConfigEvent({
      type: AgentEventType.KasModelConfigUpdate,
      models: [
        { id: 'a', name: 'A' },
        { id: 'b', name: 'B' },
      ],
      currentModelId: undefined,
      efforts: [{ value: 'high', name: 'High' }],
      currentLevel: 'high',
      origin: 'serverPush',
    });
    expect(store.getState().kas.availableModels.map((m) => m.id)).toEqual([
      'a',
      'b',
    ]);
    expect(store.getState().kas.availableEfforts).toHaveLength(1);

    store.getState().handleKasModelConfigEvent({
      type: AgentEventType.KasModelConfigUpdate,
      models: [{ id: 'c', name: 'C' }],
      currentModelId: undefined,
      efforts: [],
      currentLevel: null,
      origin: 'serverPush',
    });
    expect(store.getState().kas.availableModels.map((m) => m.id)).toEqual([
      'c',
    ]);
    expect(store.getState().kas.availableEfforts).toEqual([]);
  });

  it('setKasAvailableAgents replaces the agent list', () => {
    const store = makeStore();
    store
      .getState()
      .setKasAvailableAgents([
        { id: 'default', name: 'Default', welcomeMessage: 'hi' },
      ]);
    expect(store.getState().kas.availableAgents).toHaveLength(1);
  });
});

describe('setCurrentAgent welcome banner', () => {
  it('appends the provided welcomeMessage as a Model message', () => {
    const store = makeStore();
    store
      .getState()
      .setCurrentAgent({ name: 'greeter', welcomeMessage: 'what to build?' });
    const msgs = store.getState().messages;
    expect(msgs[msgs.length - 1]?.content).toBe('what to build?');
  });

  it('appends nothing when no welcomeMessage is provided', () => {
    const store = makeStore();
    const before = store.getState().messages.length;
    store.getState().setCurrentAgent({ name: 'greeter' });
    expect(store.getState().messages.length).toBe(before);
  });

  it('does not append a welcome when suppressed', () => {
    const store = makeStore();
    const before = store.getState().messages.length;
    store
      .getState()
      .setCurrentAgent(
        { name: 'greeter', welcomeMessage: 'hi' },
        { suppressWelcome: true }
      );
    expect(store.getState().messages.length).toBe(before);
  });
});

describe('session-origin seeding + effort auto-apply', () => {
  let originalHome: string | undefined;
  afterEach(() => {
    if (originalHome !== undefined) process.env.HOME = originalHome;
    else delete process.env.HOME;
  });

  // Write a saved per-model effort default for `modelId` to a temp cli.json.
  function seedSavedEffort(modelId: string, effort: string) {
    const home = join(
      tmpdir(),
      `kas-slice-${Date.now()}-${Math.random().toString(36).slice(2)}`
    );
    mkdirSync(join(home, '.kiro', 'settings'), { recursive: true });
    writeFileSync(
      join(home, '.kiro', 'settings', 'cli.json'),
      JSON.stringify({
        'chat.modelDefaults': { [modelId]: { output_config: { effort } } },
      }),
      'utf-8'
    );
    originalHome = process.env.HOME;
    process.env.HOME = home;
  }

  function makeSpiedStore() {
    const kiro = new Kiro();
    const setConfigOption = mock(() => Promise.resolve());
    (kiro as unknown as { setConfigOption: unknown }).setConfigOption =
      setConfigOption;
    return {
      store: createAppStore({ kiro, agentEngine: 'kas' }),
      setConfigOption,
    };
  }

  it('seeding a session sets the origin and resets the model baseline', () => {
    const store = makeStore();
    store.getState().handleKasModelConfigEvent({
      type: AgentEventType.KasModelConfigUpdate,
      models: [{ id: 'a', name: 'A' }],
      currentModelId: 'a',
      efforts: [],
      currentLevel: null,
      origin: 'serverPush',
    });
    expect(store.getState().kas.previousModelId).toBe('a');

    beginSession(store, 'resumed');
    expect(store.getState().kas.sessionOrigin).toBe('resumed');
    expect(store.getState().kas.previousModelId).toBeNull();
  });

  it('restore() reverts the prior tracking state when a session RPC fails', () => {
    const store = makeStore();
    // A prior session established its tracking.
    beginSession(store, 'new');
    store.getState().handleKasModelConfigEvent({
      type: AgentEventType.KasModelConfigUpdate,
      models: [{ id: 'opus', name: 'Opus' }],
      currentModelId: 'opus',
      efforts: [],
      currentLevel: null,
      origin: 'serverPush',
    });
    expect(store.getState().kas.sessionOrigin).toBe('new');
    expect(store.getState().kas.previousModelId).toBe('opus');

    // A resume is attempted (as /chat load would seed), then the RPC fails.
    const restore = store.getState().beginKasSession('resumed');
    expect(store.getState().kas.sessionOrigin).toBe('resumed');
    expect(store.getState().kas.previousModelId).toBeNull();
    restore();
    expect(store.getState().kas.sessionOrigin).toBe('new');
    expect(store.getState().kas.previousModelId).toBe('opus');
  });

  it('applies the saved effort default when the model resolves late in a new session', () => {
    seedSavedEffort('opus', 'high');
    const { store, setConfigOption } = makeSpiedStore();
    // Session starts new with no model yet — only the origin is seeded.
    beginSession(store, 'new');
    // The model resolves later via an autonomous serverPush.
    store.getState().handleKasModelConfigEvent({
      type: AgentEventType.KasModelConfigUpdate,
      models: [{ id: 'opus', name: 'Opus' }],
      currentModelId: 'opus',
      efforts: [
        { value: 'low', name: 'Low' },
        { value: 'high', name: 'High' },
      ],
      currentLevel: 'low',
      origin: 'serverPush',
    });
    expect(setConfigOption).toHaveBeenCalledWith('effortLevel', 'high');
  });

  it('does NOT apply the saved effort default when launched with --effort', () => {
    seedSavedEffort('opus', 'high');
    const { store, setConfigOption } = makeSpiedStore();
    beginSession(store, 'new', true);
    store.getState().handleKasModelConfigEvent({
      type: AgentEventType.KasModelConfigUpdate,
      models: [{ id: 'opus', name: 'Opus' }],
      currentModelId: 'opus',
      efforts: [
        { value: 'low', name: 'Low' },
        { value: 'high', name: 'High' },
      ],
      currentLevel: 'low',
      origin: 'serverPush',
    });
    expect(setConfigOption).not.toHaveBeenCalled();
  });

  it('does not re-apply when a subsequent update echoes the same model (no loop)', () => {
    seedSavedEffort('opus', 'high');
    const { store, setConfigOption } = makeSpiedStore();
    beginSession(store, 'new');
    // First update resolves the model and applies its saved default.
    store.getState().handleKasModelConfigEvent({
      type: AgentEventType.KasModelConfigUpdate,
      models: [{ id: 'opus', name: 'Opus' }],
      currentModelId: 'opus',
      efforts: [
        { value: 'low', name: 'Low' },
        { value: 'high', name: 'High' },
      ],
      currentLevel: 'low',
      origin: 'serverPush',
    });
    expect(setConfigOption).toHaveBeenCalledTimes(1);

    // The applied effort echoes back for the same model — must not re-apply.
    store.getState().handleKasModelConfigEvent({
      type: AgentEventType.KasModelConfigUpdate,
      models: [{ id: 'opus', name: 'Opus' }],
      currentModelId: 'opus',
      efforts: [
        { value: 'low', name: 'Low' },
        { value: 'high', name: 'High' },
      ],
      currentLevel: 'high',
      origin: 'serverPush',
    });
    expect(setConfigOption).toHaveBeenCalledTimes(1);
  });

  it('does NOT apply the saved effort default on a resumed session', () => {
    seedSavedEffort('opus', 'high');
    const { store, setConfigOption } = makeSpiedStore();
    beginSession(store, 'resumed');
    store.getState().handleKasModelConfigEvent({
      type: AgentEventType.KasModelConfigUpdate,
      models: [{ id: 'opus', name: 'Opus' }],
      currentModelId: 'opus',
      efforts: [
        { value: 'low', name: 'Low' },
        { value: 'high', name: 'High' },
      ],
      currentLevel: 'low',
      origin: 'serverPush',
    });
    expect(setConfigOption).not.toHaveBeenCalled();
  });
});

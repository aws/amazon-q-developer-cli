import { describe, it, expect } from 'bun:test';
import { createAppStore } from './app-store';
import { Kiro } from '../kiro';
import { KAS_COMMANDS, getKasCommands } from '../kas-commands';

describe('AppState.agentEngine', () => {
  it("defaults to 'v2' when KIRO_AGENT_ENGINE is unset", () => {
    const prev = process.env.KIRO_AGENT_ENGINE;
    delete process.env.KIRO_AGENT_ENGINE;
    try {
      const store = createAppStore({ kiro: new Kiro() });
      expect(store.getState().agentEngine).toBe('v2');
    } finally {
      if (prev !== undefined) process.env.KIRO_AGENT_ENGINE = prev;
    }
  });

  it("reads 'kas' from process.env.KIRO_AGENT_ENGINE", () => {
    const prev = process.env.KIRO_AGENT_ENGINE;
    process.env.KIRO_AGENT_ENGINE = 'kas';
    try {
      const store = createAppStore({ kiro: new Kiro() });
      expect(store.getState().agentEngine).toBe('kas');
    } finally {
      if (prev === undefined) delete process.env.KIRO_AGENT_ENGINE;
      else process.env.KIRO_AGENT_ENGINE = prev;
    }
  });

  it('explicit prop overrides env', () => {
    const prev = process.env.KIRO_AGENT_ENGINE;
    process.env.KIRO_AGENT_ENGINE = 'kas';
    try {
      const store = createAppStore({
        kiro: new Kiro(),
        agentEngine: 'v2',
      });
      expect(store.getState().agentEngine).toBe('v2');
    } finally {
      if (prev === undefined) delete process.env.KIRO_AGENT_ENGINE;
      else process.env.KIRO_AGENT_ENGINE = prev;
    }
  });
});

describe('AppState.kasCommands', () => {
  it("initializes to the feature-resolved KAS command set when engine is 'kas'", () => {
    const store = createAppStore({
      kiro: new Kiro(),
      agentEngine: 'kas',
    });
    // The store seeds from getKasCommands(), which drops feature-gated
    // entries (e.g. /autonomous behind remote_sandbox) not in the cohort.
    expect(store.getState().kasCommands).toEqual([...getKasCommands()]);
  });

  it("initializes to [] when engine is 'v2'", () => {
    const store = createAppStore({
      kiro: new Kiro(),
      agentEngine: 'v2',
    });
    expect(store.getState().kasCommands).toEqual([]);
  });

  it('setKasCommands replaces (does not merge) the slice', () => {
    const store = createAppStore({
      kiro: new Kiro(),
      agentEngine: 'kas',
    });
    const initial = store.getState().kasCommands;
    expect(initial.length).toBeGreaterThan(0);

    const replacement = KAS_COMMANDS.slice(0, 2);
    store.getState().setKasCommands(replacement);
    expect(store.getState().kasCommands).toEqual(replacement);
    expect(store.getState().kasCommands.length).toBe(2);
  });

  it('setKasCommands accepts empty array', () => {
    const store = createAppStore({
      kiro: new Kiro(),
      agentEngine: 'kas',
    });
    store.getState().setKasCommands([]);
    expect(store.getState().kasCommands).toEqual([]);
  });
});

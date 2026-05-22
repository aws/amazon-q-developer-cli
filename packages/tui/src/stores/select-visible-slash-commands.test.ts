import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore } from './app-store';
import { selectVisibleSlashCommands } from './selectors';
import { Kiro } from '../kiro';
import { KAS_COMMANDS } from '../kas-commands';

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

describe('selectVisibleSlashCommands', () => {
  it("returns KAS commands plus slashCommands when agentEngine === 'kas'", () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const visible = selectVisibleSlashCommands(store.getState());
    for (const cmd of KAS_COMMANDS) {
      expect(visible.find((c) => c.name === cmd.name)).toBeDefined();
    }
    for (const cmd of store.getState().slashCommands) {
      expect(visible.find((c) => c.name === cmd.name)).toBeDefined();
    }
  });

  it('exposes KAS-broadcast commands in autocomplete (regression: filter hid them)', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    // KAS broadcasts via setSlashCommands with source='backend' (see
    // index.tsx onCommandsUpdate). These must remain visible.
    store
      .getState()
      .setSlashCommands([
        { name: '/kas-prompt', description: 'Prompt', source: 'backend' },
      ]);
    const visible = selectVisibleSlashCommands(store.getState());
    expect(visible.find((c) => c.name === '/kas-prompt')).toBeDefined();
  });

  it("returns slashCommands directly when agentEngine === 'rust'", () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'rust' });
    const result = selectVisibleSlashCommands(store.getState());
    expect(result).toBe(store.getState().slashCommands);
  });

  it('keeps host-side commands like /exit and /settings reachable in KAS mode', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const visible = selectVisibleSlashCommands(store.getState());
    expect(visible.find((c) => c.name === '/exit')).toBeDefined();
    expect(visible.find((c) => c.name === '/settings')).toBeDefined();
  });
});

/**
 * Darkship gating for the /config command (cloud config UX).
 *
 * /config must be COMPLETELY unreachable outside the cloud_config rollout:
 * not registered in the local slash-command list, so it neither autocompletes
 * nor dispatches. This is the local-control test the darkship rule requires.
 */

import { describe, it, expect, mock, afterAll, afterEach } from 'bun:test';
import { createAppStore } from './app-store';
import { Kiro } from '../kiro';
import { features } from '../features';

// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, ['../kiro']);

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

const ORIGINAL_FEATURES = process.env.KIRO_ENABLED_FEATURES;

function setFeatures(json: string | undefined) {
  if (json === undefined) delete process.env.KIRO_ENABLED_FEATURES;
  else process.env.KIRO_ENABLED_FEATURES = json;
  features._resetForTests();
}

afterEach(() => {
  setFeatures(ORIGINAL_FEATURES);
});

afterAll(() => {
  mock.restore();
});

describe('/config darkship gating', () => {
  it('is not registered when cloud_config is off (default)', () => {
    setFeatures(undefined);
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    expect(
      store.getState().slashCommands.find((c) => c.name === '/config')
    ).toBeUndefined();
  });

  it('is not registered when other features are on but cloud_config is not', () => {
    setFeatures('["remote_sandbox","tui"]');
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    expect(
      store.getState().slashCommands.find((c) => c.name === '/config')
    ).toBeUndefined();
  });

  it('is registered inside the cloud_config cohort', () => {
    setFeatures('["cloud_config"]');
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    expect(
      store.getState().slashCommands.find((c) => c.name === '/config')
    ).toBeDefined();
  });

  it('is not registered on V2 even inside the cohort (KAS-only)', () => {
    // Every cache the panel reads (descriptors, powers/steering/hooks
    // pushes) is KAS-fed — V2 has no KAS, so /config never registers there.
    setFeatures('["cloud_config"]');
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    expect(
      store.getState().slashCommands.find((c) => c.name === '/config')
    ).toBeUndefined();
  });
});

/**
 * Telemetry wiring for the cloud-config UX: the store emits the
 * /config category-view counter when the panel opens, and the sync-health
 * diagnostics counter when a cloudConfig diagnostics push lands.
 */

import { afterAll, beforeEach, describe, expect, it, mock } from 'bun:test';
import { AgentEventType } from '../types/agent-events';

const actualObserver = await import('../utils/tui-telemetry-observer');
// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, [
  '../utils/tui-telemetry-observer',
  '../kiro',
]);

const mockRecordTuiConfigPanel = mock((_a: unknown) => {});
const mockRecordTuiCloudConfigDiagnostics = mock((_a: unknown) => {});
mock.module('../utils/tui-telemetry-observer', () => ({
  ...actualObserver,
  recordTuiConfigPanel: mockRecordTuiConfigPanel,
  recordTuiCloudConfigDiagnostics: mockRecordTuiCloudConfigDiagnostics,
}));

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

const { createAppStore } = await import('./app-store');
const { Kiro } = await import('../kiro');
const { features } = await import('../features');

const ORIGINAL_FEATURES = process.env.KIRO_ENABLED_FEATURES;

function setFeatures(json: string | undefined) {
  if (json === undefined) delete process.env.KIRO_ENABLED_FEATURES;
  else process.env.KIRO_ENABLED_FEATURES = json;
  features._resetForTests();
}

beforeEach(() => {
  mockRecordTuiConfigPanel.mockClear();
  mockRecordTuiCloudConfigDiagnostics.mockClear();
  // The diagnostics counter is cohort-gated (all three cloud-config metrics
  // describe the flagged population) — tests emit as an on-cohort client.
  setFeatures('["cloud_config"]');
});

afterAll(() => {
  setFeatures(ORIGINAL_FEATURES);
  mock.restore();
});

describe('/config panel telemetry', () => {
  it('counts a menu view on bare open and the category on routed open', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });

    store.getState().setShowConfigPanel(true);
    store.getState().setShowConfigPanel(true, 'steering');
    store.getState().setShowConfigPanel(false);

    // engine is omitted at the call site: /config is KAS-only, so the
    // recorder's v3 default is always correct.
    expect(mockRecordTuiConfigPanel.mock.calls.map((c) => c[0])).toMatchObject([
      { category: 'menu' },
      { category: 'steering' },
    ]);
    expect(mockRecordTuiConfigPanel.mock.calls[0]?.[0]).not.toHaveProperty(
      'engine'
    );
  });

  it('opening the panel resets a stale handoff token (never opens inert)', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    store.getState().beginConfigHandoff();
    expect(store.getState().configHandoffToken).not.toBe(0);
    store.getState().setShowConfigPanel(true);
    expect(store.getState().configHandoffToken).toBe(0);
  });

  it('beginConfigHandoff returns a fresh monotonic token each call', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const a = store.getState().beginConfigHandoff();
    const b = store.getState().beginConfigHandoff();
    expect(b).toBeGreaterThan(a);
    expect(store.getState().configHandoffToken).toBe(b);
    store.getState().endConfigHandoff();
    expect(store.getState().configHandoffToken).toBe(0);
  });

  it('ESC-back re-entry via reopenConfigMenu counts a menu view', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });

    // /config → route to /mcp (panel closes) → ESC walks back to the table.
    store.getState().setShowConfigPanel(true);
    store.getState().setShowConfigPanel(false);
    store.getState().reopenConfigMenu();

    expect(store.getState().showConfigPanel).toBe(true);
    expect(store.getState().configPanelCategory).toBeNull();
    expect(mockRecordTuiConfigPanel.mock.calls.map((c) => c[0])).toMatchObject([
      { category: 'menu' },
      { category: 'menu' },
    ]);
  });
});

describe('cloud config diagnostics telemetry', () => {
  it('counts severities from a cloudConfig push and ignores other domains', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.DiagnosticsUpdate,
      domain: 'cloudConfig',
      diagnostics: [
        { severity: 'warning', code: 'staleReplica', message: 'stale' },
        { severity: 'error', code: 'syncFailed', message: 'broken' },
      ],
    } as never);
    handler({
      type: AgentEventType.DiagnosticsUpdate,
      domain: 'otherDomain',
      diagnostics: [{ severity: 'error', code: 'x', message: 'y' }],
    } as never);

    expect(mockRecordTuiCloudConfigDiagnostics).toHaveBeenCalledTimes(1);
    expect(
      mockRecordTuiCloudConfigDiagnostics.mock.calls[0]?.[0]
    ).toMatchObject({ severities: ['warning', 'error'], engine: 'v3' });
  });

  it('does not count off-cohort (diagnostics stored, metric silent)', () => {
    setFeatures('[]');
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.DiagnosticsUpdate,
      domain: 'cloudConfig',
      diagnostics: [{ severity: 'error', code: 'syncFailed', message: 'x' }],
    } as never);

    // Off-cohort clients can never render a diagnostic, so counting them
    // would change the metric's meaning — but the store still holds the set.
    expect(mockRecordTuiCloudConfigDiagnostics).not.toHaveBeenCalled();
    expect(store.getState().cloudConfigDiagnostics).toHaveLength(1);
  });

  it('does not count an empty (all-clear) push', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.DiagnosticsUpdate,
      domain: 'cloudConfig',
      diagnostics: [],
    } as never);

    expect(mockRecordTuiCloudConfigDiagnostics).not.toHaveBeenCalled();
  });
});

import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { vi } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { createAppStore } from '../app-store';

/**
 * Import Kiro via a cache-busting query suffix so that mock.module pollution
 * from earlier test files sharing the bun process does not replace the class.
 */
// @ts-expect-error — query suffix bypasses bun's mock registry
const { Kiro } = await import('../../kiro?session-resume-test');

describe('resumeSession cwd transaction', () => {
  let root: string;
  let previousCwd: string;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'session-resume-'));
    previousCwd = process.cwd();
  });

  afterEach(() => {
    process.chdir(previousCwd);
    rmSync(root, { recursive: true, force: true });
    vi.restoreAllMocks();
  });

  it('restores the previous cwd when a cross-workspace load fails', async () => {
    const startingDirectory = join(root, 'starting');
    const targetDirectory = join(root, 'target');
    mkdirSync(startingDirectory);
    mkdirSync(targetDirectory);
    process.chdir(startingDirectory);
    const canonicalStartingDirectory = process.cwd();

    const kiro = new Kiro();
    vi.spyOn(kiro, 'loadSession').mockRejectedValue(new Error('load failed'));
    const store = createAppStore({ kiro, agentEngine: 'kas' });
    const dashboardSessions = [
      {
        sessionId: 'remote-session',
        cwd: targetDirectory,
        title: 'Resume target',
        updatedAt: '2026-08-10T00:00:00.000Z',
      },
    ];
    store.setState({
      showSessionDashboard: true,
      sessionDashboardSessions: dashboardSessions,
    });

    const resumed = await store
      .getState()
      .resumeSession('remote-session', 'cloud', targetDirectory);

    expect(resumed).toBe(false);
    expect(process.cwd()).toBe(canonicalStartingDirectory);
    expect(store.getState().showSessionDashboard).toBe(true);
    expect(store.getState().sessionDashboardSessions).toEqual(
      dashboardSessions
    );
  });

  it('supports dashboard retry from failed resume through successful load', async () => {
    const kiro = new Kiro();
    const loadSession = vi
      .spyOn(kiro, 'loadSession')
      .mockRejectedValueOnce(new Error('transient load failure'))
      .mockResolvedValue({ sessionId: 'retry-session' });
    const store = createAppStore({ kiro, agentEngine: 'kas' });
    const dashboardSessions = [
      {
        sessionId: 'retry-session',
        cwd: '/remote/workspace',
        title: 'Retry target',
        updatedAt: '2026-08-10T00:00:00.000Z',
        source: 'remote' as const,
        engine: 'v3' as const,
      },
    ];
    store.setState({
      showSessionDashboard: true,
      sessionDashboardSessions: dashboardSessions,
    });

    const failed = await store
      .getState()
      .resumeSession('retry-session', 'cloud', undefined, 'v3');

    expect(failed).toBe(false);
    expect(store.getState().showSessionDashboard).toBe(true);
    expect(store.getState().sessionDashboardSessions).toEqual(
      dashboardSessions
    );

    const resumed = await store
      .getState()
      .resumeSession('retry-session', 'cloud', undefined, 'v3');

    expect(resumed).toBe(true);
    expect(store.getState().showSessionDashboard).toBe(false);
    expect(store.getState().sessionDashboardSessions).toEqual([]);
    expect(loadSession).toHaveBeenCalledTimes(2);
    expect(loadSession).toHaveBeenLastCalledWith(
      'retry-session',
      expect.any(Function),
      { source: 'remote' }
    );
  });

  it('rejects a second resume while the first load is pending', async () => {
    const kiro = new Kiro();
    let resolveLoad!: (value: { sessionId: string }) => void;
    const load = new Promise<{ sessionId: string }>((resolve) => {
      resolveLoad = resolve;
    });
    const loadSession = vi.spyOn(kiro, 'loadSession').mockReturnValue(load);
    const store = createAppStore({ kiro, agentEngine: 'kas' });
    store.setState({ mode: 'session-dashboard', showSessionDashboard: true });

    const first = store
      .getState()
      .resumeSession('first-session', 'cloud', undefined, 'v3');
    const second = await store
      .getState()
      .resumeSession('second-session', 'cloud', undefined, 'v3');

    expect(second).toBe(false);
    expect(store.getState().mode).toBe('inline');
    expect(loadSession).toHaveBeenCalledTimes(1);

    resolveLoad({ sessionId: 'first-session' });
    await expect(first).resolves.toBe(true);
  });

  it('returns true only after the selected session loads', async () => {
    const kiro = new Kiro();
    vi.spyOn(kiro, 'loadSession').mockResolvedValue({
      sessionId: 'remote-session',
    });
    const store = createAppStore({ kiro, agentEngine: 'kas' });

    const resumed = await store
      .getState()
      .resumeSession('remote-session', 'cloud');

    expect(resumed).toBe(true);
  });
});

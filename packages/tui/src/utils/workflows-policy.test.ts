/**
 * Opt-out contract: one preference has to govern every workflow boundary.
 * The failure this guards is a half-disabled surface — KAS told the feature
 * is off while the TUI still advertises `/goal` and starts the extension.
 */

import { describe, test, expect, beforeEach, afterEach } from 'bun:test';
import { mkdtempSync, writeFileSync, rmSync, mkdirSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { Feature, features } from '../features';

describe('workflows opt-in policy', () => {
  let tmpDir: string;
  let originalKiroHome: string | undefined;
  let originalEnabledFeatures: string | undefined;

  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'workflows-policy-test-'));
    originalKiroHome = process.env.KIRO_HOME;
    process.env.KIRO_HOME = tmpDir;
    originalEnabledFeatures = process.env.KIRO_ENABLED_FEATURES;
    process.env.KIRO_ENABLED_FEATURES = '[]';
    features._resetForTests();
  });

  afterEach(() => {
    if (originalKiroHome === undefined) delete process.env.KIRO_HOME;
    else process.env.KIRO_HOME = originalKiroHome;
    if (originalEnabledFeatures === undefined)
      delete process.env.KIRO_ENABLED_FEATURES;
    else process.env.KIRO_ENABLED_FEATURES = originalEnabledFeatures;
    features._resetForTests();
    rmSync(tmpDir, { recursive: true, force: true });
  });

  function writeSettings(settings: Record<string, unknown>) {
    const dir = join(tmpDir, 'settings');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'cli.json'), JSON.stringify(settings));
  }

  function onRollout(): void {
    process.env.KIRO_ENABLED_FEATURES = JSON.stringify([Feature.Workflows]);
    features._resetForTests();
  }

  /** Re-import so the modules pick up the current env + settings file. */
  async function freshModules() {
    for (const m of [
      './workflows-policy.js',
      './cli-settings.js',
      './kas-settings.js',
      '../kas-commands.js',
    ]) {
      delete require.cache[require.resolve(m)];
    }
    const policy = await import('./workflows-policy.js');
    const kasSettings = await import('./kas-settings.js');
    const commands = await import('../kas-commands.js');
    return {
      resolveWorkflowsPolicy: policy.resolveWorkflowsPolicy,
      buildKasSettings: kasSettings.buildKasSettings,
      getKasCommands: commands.getKasCommands,
    };
  }

  const workflowCommandCount = (
    commands: readonly { name: string }[]
  ): number => commands.filter((c) => c.name.startsWith('/goal')).length;

  test('off the rollout: unavailable, disabled, no control, no commands', async () => {
    const { resolveWorkflowsPolicy, buildKasSettings, getKasCommands } =
      await freshModules();

    expect(resolveWorkflowsPolicy()).toEqual({
      available: false,
      enabled: false,
    });
    expect(buildKasSettings()?.workflows).toBeUndefined();
    expect(workflowCommandCount(getKasCommands())).toBe(0);
  });

  test('on the rollout, not opted in: control reachable, feature off', async () => {
    onRollout();
    const { resolveWorkflowsPolicy, buildKasSettings, getKasCommands } =
      await freshModules();

    // `available` keeps the /settings toggle reachable so the user can opt in.
    expect(resolveWorkflowsPolicy()).toEqual({
      available: true,
      enabled: false,
    });
    expect(buildKasSettings()?.workflows).toEqual({ enabled: false });
    expect(buildKasSettings()?.goal).toEqual({ enabled: false });
    expect(workflowCommandCount(getKasCommands())).toBe(0);
  });

  test('on the rollout, opted in: every boundary agrees the feature is live', async () => {
    onRollout();
    writeSettings({ 'chat.enableWorkflows': true });
    const { resolveWorkflowsPolicy, buildKasSettings, getKasCommands } =
      await freshModules();

    expect(resolveWorkflowsPolicy()).toEqual({
      available: true,
      enabled: true,
    });
    expect(buildKasSettings()?.workflows).toEqual({ enabled: true });
    expect(buildKasSettings()?.goal).toEqual({ enabled: true });
    expect(workflowCommandCount(getKasCommands())).toBeGreaterThan(0);
  });

  test('explicit opt-out matches the never-toggled state at every boundary', async () => {
    onRollout();
    writeSettings({ 'chat.enableWorkflows': false });
    const { resolveWorkflowsPolicy, buildKasSettings, getKasCommands } =
      await freshModules();

    expect(resolveWorkflowsPolicy()).toEqual({
      available: true,
      enabled: false,
    });
    expect(buildKasSettings()?.workflows).toEqual({ enabled: false });
    expect(workflowCommandCount(getKasCommands())).toBe(0);
  });

  test('the opt-in cannot turn the feature on off the rollout', async () => {
    writeSettings({ 'chat.enableWorkflows': true });
    const { resolveWorkflowsPolicy, buildKasSettings, getKasCommands } =
      await freshModules();

    expect(resolveWorkflowsPolicy()).toEqual({
      available: false,
      enabled: false,
    });
    expect(buildKasSettings()?.workflows).toBeUndefined();
    expect(workflowCommandCount(getKasCommands())).toBe(0);
  });
});

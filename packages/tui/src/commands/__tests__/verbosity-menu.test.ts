/**
 * Direct unit tests for the extracted `handleVerbosity` entry point. The full
 * routing tree is covered via runEffect in verbose-command.test.ts; this file
 * pins the module's public contract: the lite gate, config write-through, and
 * active-preset detection (the disambiguation invariant).
 */

import {
  describe,
  it,
  expect,
  beforeEach,
  beforeAll,
  afterAll,
} from 'bun:test';
import { mkdtempSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

let tmpHome: string;
let originalKiroHome: string | undefined;

// Redirect KIRO_HOME before importing so config writes hit a throwaway dir.
beforeAll(() => {
  originalKiroHome = process.env.KIRO_HOME;
  tmpHome = mkdtempSync(join(tmpdir(), 'kiro-verbosity-menu-test-'));
  process.env.KIRO_HOME = tmpHome;
});

afterAll(() => {
  if (originalKiroHome === undefined) delete process.env.KIRO_HOME;
  else process.env.KIRO_HOME = originalKiroHome;
  if (tmpHome) {
    try {
      rmSync(tmpHome, { recursive: true, force: true });
    } catch {
      // best-effort
    }
  }
});

import { handleVerbosity } from '../verbosity-menu.js';
import {
  DENSITY_PRESETS,
  DENSITY_DISPLAY,
  DENSITY_FILTERS,
  getVerboseConfig,
  resetVerboseCache,
  setVerboseConfig,
  type DensityPreset,
} from '../../lite/verbose.js';
import type { SlashCommand } from '../../stores/app-store.js';
import { createMockCommandContext } from './test-helpers.js';

const cmd: SlashCommand = {
  name: '/verbosity',
  description: '',
  source: 'local',
  meta: { local: true, liteOnly: true },
};

const liteCtx = () => {
  const ctx = createMockCommandContext({ slashCommands: [cmd] });
  (ctx as any).getUiMode = () => 'lite';
  return ctx;
};

const firstArg = (ctx: ReturnType<typeof liteCtx>, spy: string) =>
  (ctx._spies[spy]!.mock.calls[0] as unknown[])?.[0];

beforeEach(() => {
  resetVerboseCache();
  setVerboseConfig({ filters: ['all'] });
});

describe('handleVerbosity', () => {
  it('rejects outside lite mode without mutating config', () => {
    const ctx = createMockCommandContext({ slashCommands: [cmd] }); // getUiMode → tui
    const before = JSON.stringify(getVerboseConfig());
    expect(handleVerbosity(null, ctx, cmd, '')).toBe(true);
    const call = ctx._spies.showAlert!.mock.calls[0] as unknown[];
    expect(call[0]).toContain('only available in lite mode');
    expect(call[1]).toBe('error');
    expect(JSON.stringify(getVerboseConfig())).toBe(before);
  });

  it('persists on/off CLI forms through the filter list', () => {
    const ctx = liteCtx();
    handleVerbosity(null, ctx, cmd, 'off');
    expect(getVerboseConfig().filters).toEqual([]);
    handleVerbosity(null, ctx, cmd, 'on');
    expect(getVerboseConfig().filters).toEqual(['all']);
  });

  // detectActivePreset must round-trip every preset — minimal/lean share an
  // empty filter list (disambiguated by display), default/full share most of
  // their display (disambiguated by filters).
  it.each(DENSITY_PRESETS.map((p) => [p] as [DensityPreset]))(
    'status reports %s when its exact shape is saved',
    (preset) => {
      setVerboseConfig({
        display: { ...DENSITY_DISPLAY[preset] },
        filters: [...DENSITY_FILTERS[preset]],
      });
      const ctx = liteCtx();
      handleVerbosity(null, ctx, cmd, 'status');
      expect(firstArg(ctx, 'announceSystem')).toContain(`density: ${preset}`);
    }
  );
});

/**
 * Direct contract tests for handleVerbosity. Full runEffect routing lives in
 * verbose-command.test.ts.
 */

import {
  afterAll,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
} from 'bun:test';
import { mkdtempSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

let tmpHome: string;
let originalKiroHome: string | undefined;
let originalRollout: string | undefined;

beforeAll(() => {
  originalKiroHome = process.env.KIRO_HOME;
  originalRollout = process.env.KIRO_LITE_ROLLOUT_ENABLED;
  tmpHome = mkdtempSync(join(tmpdir(), 'kiro-verbosity-menu-test-'));
  process.env.KIRO_HOME = tmpHome;
  process.env.KIRO_LITE_ROLLOUT_ENABLED = '1';
});

afterAll(() => {
  if (originalKiroHome === undefined) delete process.env.KIRO_HOME;
  else process.env.KIRO_HOME = originalKiroHome;
  if (originalRollout === undefined)
    delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
  else process.env.KIRO_LITE_ROLLOUT_ENABLED = originalRollout;
  rmSync(tmpHome, { recursive: true, force: true });
});

import { handleVerbosity } from '../verbosity-menu.js';
import {
  DENSITY_DISPLAY,
  DENSITY_FILTERS,
  DENSITY_PRESETS,
  getVerboseConfig,
  getTuiVerboseDisplay,
  getTuiVerboseFilters,
  resetVerboseCache,
  setVerboseConfig,
  type DensityPreset,
  type ToolArgsMode,
} from '../../lite/verbose.js';
import type { SlashCommand } from '../../stores/app-store.js';
import {
  createMockCommandContext,
  type MockCommandContext,
} from './test-helpers.js';

type UiMode = 'lite' | 'tui';
type Engine = 'v2' | 'kas';
type Menu = { options: Array<{ value: string }> };

const cmd: SlashCommand = {
  name: '/verbosity',
  description: '',
  source: 'local',
  meta: { local: true },
};

const context = (
  uiMode: UiMode = 'lite',
  agentEngine: Engine = 'v2'
): MockCommandContext => {
  const ctx = createMockCommandContext({ slashCommands: [cmd] });
  ctx.agentEngine = agentEngine;
  ctx.getUiMode = () => uiMode;
  return ctx;
};

const calls = (ctx: MockCommandContext, spy: string) =>
  ctx._spies[spy]!.mock.calls as unknown[][];

const firstArg = <T>(ctx: MockCommandContext, spy: string): T =>
  calls(ctx, spy)[0]![0] as T;

const run = (
  args: string,
  uiMode: UiMode = 'lite',
  agentEngine: Engine = 'v2'
) => {
  const ctx = context(uiMode, agentEngine);
  expect(handleVerbosity(null, ctx, cmd, args)).toBe(true);
  return ctx;
};

const menuValues = (ctx: MockCommandContext) =>
  firstArg<Menu>(ctx, 'setActiveCommand').options.map(({ value }) => value);

beforeEach(() => {
  rmSync(join(tmpHome, 'settings'), { recursive: true, force: true });
  resetVerboseCache();
  setVerboseConfig({ filters: ['all'] });
});

describe('handleVerbosity', () => {
  it('opens the TUI menu in-cohort without a lite-only error', () => {
    const ctx = run('', 'tui');
    expect(ctx._spies.setActiveCommand).toHaveBeenCalled();
    expect(calls(ctx, 'showAlert').flat().join(' ')).not.toContain(
      'only available in lite mode'
    );
    expect(
      firstArg<{ command: SlashCommand }>(ctx, 'setActiveCommand').command.name
    ).toBe('/verbosity');
  });

  it('rejects TUI mode off the Lite rollout cohort', () => {
    delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
    try {
      const ctx = run('', 'tui');
      expect(ctx._spies.setActiveCommand).not.toHaveBeenCalled();
      expect(firstArg<string>(ctx, 'showAlert')).toContain('not available');
    } finally {
      process.env.KIRO_LITE_ROLLOUT_ENABLED = '1';
    }
  });

  it.each([
    ['kas', false, true],
    ['v2', true, false],
  ] satisfies Array<[Engine, boolean, boolean]>)(
    '%s gates subagent responses (changes=%s)',
    (agentEngine, changes, warns) => {
      const before = getTuiVerboseDisplay().subagent.responses;
      const ctx = run('set:subagent:responses', 'tui', agentEngine);
      expect(getTuiVerboseDisplay().subagent.responses).toBe(
        changes ? !before : before
      );
      expect(
        calls(ctx, 'showAlert').some((call) =>
          String(call[0]).includes('KAS subagents provide responses')
        )
      ).toBe(warns);
    }
  );

  it.each(DENSITY_PRESETS.map((preset) => [preset] as [DensityPreset]))(
    'status identifies the exact %s preset shape',
    (preset: DensityPreset) => {
      setVerboseConfig({
        display: { ...DENSITY_DISPLAY[preset] },
        filters: [...DENSITY_FILTERS[preset]],
      });
      expect(firstArg<string>(run('status'), 'announceSystem')).toContain(
        `density: ${preset}`
      );
    }
  );

  it('applies and recognizes the TUI default without subagent detail', () => {
    run('density default', 'tui');
    expect(getTuiVerboseDisplay()).toMatchObject({
      persistOutput: false,
      subagent: {
        pipeline: false,
        prompts: false,
        roles: false,
        deps: false,
        responses: false,
      },
    });
    expect(getTuiVerboseFilters()).toEqual(['all', '-subagent']);
    expect(firstArg<string>(run('status', 'tui'), 'announceSystem')).toContain(
      'density: default'
    );
  });

  it('cycles thinking expanded -> collapsed -> off -> expanded', () => {
    setVerboseConfig({ display: { thinkingDisplay: 'expanded' } }, 'tui');
    const ctx = context('tui');
    for (const [thinkingDisplay, showThinkingContent] of [
      ['collapsed', true],
      ['off', false],
      ['expanded', true],
    ] as const) {
      expect(handleVerbosity(null, ctx, cmd, 'set:thinkingDisplay:cycle')).toBe(
        true
      );
      expect(getVerboseConfig('tui').display).toMatchObject({
        thinkingDisplay,
        showThinkingContent,
      });
    }
  });

  it('sets an explicit thinking mode', () => {
    setVerboseConfig({ display: { thinkingDisplay: 'expanded' } }, 'tui');
    run('set:thinkingDisplay:off', 'tui');
    expect(getVerboseConfig('tui').display).toMatchObject({
      thinkingDisplay: 'off',
      showThinkingContent: false,
    });
  });

  it.each([
    [
      'tui',
      ['set:toolArgsMode:toggle'],
      [
        'set:toolArgsMode:off',
        'set:toolArgsMode:inline',
        'set:toolArgsMode:block',
      ],
    ],
    [
      'lite',
      [
        'set:toolArgsMode:off',
        'set:toolArgsMode:inline',
        'set:toolArgsMode:block',
      ],
      ['set:toolArgsMode:toggle'],
    ],
  ] satisfies Array<[UiMode, string[], string[]]>)(
    '%s exposes its args controls',
    (uiMode, includes, excludes) => {
      const values = menuValues(run('menu:tool', uiMode));
      for (const value of includes) expect(values).toContain(value);
      for (const value of excludes) expect(values).not.toContain(value);
    }
  );

  it.each([
    ['off', ['inline', 'off']],
    ['block', ['off']],
  ] satisfies Array<[ToolArgsMode, ToolArgsMode[]]>)(
    'TUI args toggle maps %s through %p',
    (initial, expected) => {
      setVerboseConfig({ display: { toolArgsMode: initial } }, 'tui');
      for (const toolArgsMode of expected) {
        run('set:toolArgsMode:toggle', 'tui');
        expect(getVerboseConfig('tui').display?.toolArgsMode).toBe(
          toolArgsMode
        );
      }
    }
  );

  it('keeps preset changes on the active UI', () => {
    run('density lean', 'lite');
    run('density full', 'tui');
    expect(getVerboseConfig('lite').filters).toEqual([]);
    expect(getVerboseConfig('tui').filters).toEqual(['all']);
    expect(getVerboseConfig('lite').display?.outputMaxLines).toBe(10);
    expect(getVerboseConfig('tui').display?.outputMaxLines).toBeNull();
  });
});

/**
 * Tests for the /verbosity (formerly /verbose) effect handler.
 *
 * Strategy: redirect KIRO_HOME to a tmp dir before importing the effects
 * module so verbose config writes don't clobber the developer's real
 * ~/.kiro/settings/lite_verbose.json. Tests reset the cache + clear the
 * temp file between cases so each starts from defaults.
 */

import {
  describe,
  it,
  expect,
  beforeEach,
  beforeAll,
  afterEach,
  afterAll,
} from 'bun:test';
import { mkdtempSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

let tmpHome: string;
let originalKiroHome: string | undefined;

// Set KIRO_HOME *before* importing the modules under test so kiroHomePath()
// resolves into our throwaway dir for the entire suite.
beforeAll(() => {
  originalKiroHome = process.env.KIRO_HOME;
  tmpHome = mkdtempSync(join(tmpdir(), 'kiro-verbose-cmd-test-'));
  process.env.KIRO_HOME = tmpHome;
});

afterAll(() => {
  if (originalKiroHome === undefined) {
    delete process.env.KIRO_HOME;
  } else {
    process.env.KIRO_HOME = originalKiroHome;
  }
  if (tmpHome) {
    try {
      rmSync(tmpHome, { recursive: true, force: true });
    } catch {
      // best-effort
    }
  }
});

import { runEffect } from '../effects.js';
import {
  DEFAULT_DISPLAY,
  getVerboseConfig,
  resetVerboseCache,
  setVerboseConfig,
} from '../../lite/verbose.js';
import type { VerboseDisplayConfig } from '../../lite/verbose.js';
import type { SlashCommand } from '../../stores/app-store.js';
import { createMockCommandContext } from './test-helpers.js';

// Most display-config fixtures only vary one or two fields off the default
// shape; this deep-merges overrides (subagent included) so call sites declare
// just the field under test instead of repeating the full 16-key literal.
type DisplayOverrides = Partial<Omit<VerboseDisplayConfig, 'subagent'>> & {
  subagent?: Partial<VerboseDisplayConfig['subagent']>;
};
const display = (o: DisplayOverrides = {}): VerboseDisplayConfig => ({
  ...DEFAULT_DISPLAY,
  ...o,
  subagent: { ...DEFAULT_DISPLAY.subagent, ...(o.subagent ?? {}) },
});

const verbosityCmd: SlashCommand = {
  name: '/verbosity',
  description: '',
  source: 'local' as const,
  meta: { local: true, liteOnly: true },
};

function liteCtx() {
  const ctx = createMockCommandContext({ slashCommands: [verbosityCmd] });
  // Override the default 'tui' to 'lite' so the verbosity handler's lite-only
  // gate accepts the call.
  (ctx as any).getUiMode = () => 'lite';
  return ctx;
}

// The last `setActiveCommand({ options, previewKey })` arg — the menu the
// handler opened on this run.
function lastMenu(ctx: ReturnType<typeof liteCtx>): {
  options: any[];
  previewKey?: string;
} {
  const calls = ctx._spies.setActiveCommand!.mock
    .calls as unknown as unknown[][];
  return calls[calls.length - 1]![0] as { options: any[]; previewKey?: string };
}

const rowDesc = (ctx: ReturnType<typeof liteCtx>, label: string) =>
  lastMenu(ctx).options.find((o) => o.label === label)?.description;

// The option `value`s of the menu the handler just opened.
const menuValues = (ctx: ReturnType<typeof liteCtx>): string[] =>
  lastMenu(ctx).options.map((o) => String(o.value));

describe('/verbosity lite-mode gate', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('rejects with an error alert when called outside lite mode', () => {
    const before = JSON.stringify(getVerboseConfig());
    const ctx = createMockCommandContext({ slashCommands: [verbosityCmd] });
    // ctx.getUiMode defaults to 'tui'
    const handled = runEffect(verbosityCmd, null, ctx, '');
    expect(handled).toBe(true);
    const calls = ctx._spies.showAlert!.mock.calls as unknown as unknown[][];
    expect(calls[0]![0]).toContain('only available in lite mode');
    expect(calls[0]![1]).toBe('error');
    // No mutation to config.
    expect(JSON.stringify(getVerboseConfig())).toEqual(before);
  });
});

describe('/verbosity top menu and status', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('bare /verbosity opens the density menu when an active preset is detected', () => {
    // Setup: filters=['all'] in beforeEach combined with the `full` preset
    // display matches DENSITY_FILTERS.full + DENSITY_DISPLAY.full, so the
    // entry router should land in the density menu (the simple, common
    // case for preset users). detectActivePreset() requires BOTH display
    // and filters to match a preset, so we explicitly set display here —
    // the beforeEach's filters=['all'] alone doesn't match because
    // DEFAULT_DISPLAY.outputMaxLines (5) ≠ full preset's null.
    setVerboseConfig({
      filters: ['all'],
      display: { ...require('../../lite/verbose.js').DENSITY_DISPLAY.full },
    });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, '');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    expect(calls.length).toBe(1);
    const arg = calls[0]![0] as {
      command: SlashCommand;
      options: any[];
      previewKey?: string;
    };
    expect(arg.command.name).toBe('/verbosity');
    expect(arg.previewKey).toBe('density');
    // Density rows (one per preset) plus a Custom row.
    const labels = arg.options.map((o) => o.label);
    expect(labels).toContain('default');
    expect(labels).toContain('full');
    expect(labels).toContain('custom');
    // No per-knob labels — those live in the config menu.
    expect(labels).not.toContain('Tool calls');
    expect(labels).not.toContain('Show output');
  });

  it('bare /verbosity opens the config menu when no preset is active (custom)', () => {
    // Hand-toggle into a shape that matches no preset: filters=[] AND a
    // partial display patch.
    setVerboseConfig({
      filters: [],
      display: display({ showElapsed: false }),
    });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, '');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[0]![0] as { options: any[] };
    const labels = arg.options.map((o) => o.label);
    // Config menu rows surface — Tool calls, Subagent, etc.
    expect(labels).toContain('Tool calls');
    expect(labels).toContain('Subagent');
    expect(labels).toContain('Show output');
    // The standalone Reset row is gone — the `default` preset replaces it.
    expect(labels).not.toContain('Reset to defaults');
  });

  it('/verbosity config explicitly opens the config menu regardless of preset', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'config');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[0]![0] as { options: any[]; previewKey?: string };
    expect(arg.previewKey).toBe('top');
    const labels = arg.options.map((o) => o.label);
    expect(labels).toContain('Tool calls');
  });

  it('/verbosity on sets filters to ["all"]', () => {
    setVerboseConfig({ filters: [] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'on');
    expect(getVerboseConfig().filters).toEqual(['all']);
  });

  it('/verbosity off sets filters to []', () => {
    setVerboseConfig({ filters: ['all'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'off');
    expect(getVerboseConfig().filters).toEqual([]);
  });

  it('/verbosity status announces filters without an ON/OFF prefix', () => {
    const ctx = liteCtx();
    setVerboseConfig({ filters: ['shell', 'mcp'] });
    runEffect(verbosityCmd, null, ctx, 'status');
    const calls = ctx._spies.announceSystem!.mock
      .calls as unknown as unknown[][];
    const msg = calls[0]![0] as string;
    expect(msg).not.toMatch(/\bON\b/);
    expect(msg).not.toMatch(/\bOFF\b/);
    expect(msg).toContain('shell, mcp');
    // Unchanged
    expect(getVerboseConfig().filters).toEqual(['shell', 'mcp']);
  });
});

describe('/verbosity filter mutations', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('/verbosity only <list> replaces filters', () => {
    const ctx = liteCtx();
    runEffect(
      verbosityCmd,
      null,
      ctx,
      'only shell mcp__nova-memory-mcp__recall'
    );
    const f = getVerboseConfig().filters;
    expect(f).toContain('shell');
    expect(f).toContain('mcp__nova-memory-mcp__recall');
    expect(f).not.toContain('all');
  });

  it('/verbosity all resets filters', () => {
    setVerboseConfig({ filters: ['shell'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'all');
    expect(getVerboseConfig().filters).toEqual(['all']);
  });

  it('/verbosity add appends without dropping existing', () => {
    setVerboseConfig({ filters: ['shell'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'add mcp');
    const f = getVerboseConfig().filters;
    expect(f).toContain('shell');
    expect(f).toContain('mcp');
  });

  it('/verbosity add from "all" baseline collapses to the new tokens', () => {
    setVerboseConfig({ filters: ['all'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'add shell');
    // From "all" baseline we treat the union as the new explicit list.
    // Expect shell present and "all" gone (otherwise normalization would
    // collapse back to all and adding a category would be a no-op).
    const f = getVerboseConfig().filters;
    expect(f).toContain('shell');
    expect(f).not.toContain('all');
  });

  it('/verbosity remove drops listed tokens', () => {
    setVerboseConfig({ filters: ['shell', 'mcp', 'read'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'remove mcp');
    const f = getVerboseConfig().filters;
    expect(f).toContain('shell');
    expect(f).toContain('read');
    expect(f).not.toContain('mcp');
  });

  it('removing the last filter leaves filters empty (off)', () => {
    setVerboseConfig({ filters: ['shell'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'remove shell');
    expect(getVerboseConfig().filters).toEqual([]);
  });

  it('only with no tokens shows an error alert and does not mutate', () => {
    setVerboseConfig({ filters: ['shell'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'only');
    const calls = ctx._spies.showAlert!.mock.calls as unknown as unknown[][];
    expect(calls[0]![0]).toContain('needs at least one filter token');
    expect(calls[0]![1]).toBe('error');
    expect(getVerboseConfig().filters).toEqual(['shell']);
  });

  it('rejects-only input surfaces a specific error', () => {
    setVerboseConfig({ filters: ['all'] });
    const ctx = liteCtx();
    // Every token contains shell metachars that validateTokens rejects.
    runEffect(verbosityCmd, null, ctx, 'only bad|token nope$arg what?');
    const calls = ctx._spies.showAlert!.mock.calls as unknown as unknown[][];
    expect(calls[0]![0]).toContain('No valid tokens');
    expect(calls[0]![1]).toBe('error');
  });

  it('mixed valid/invalid tokens: keeps the valid ones, lists ignored', () => {
    setVerboseConfig({ filters: ['all'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'only shell bad|token mcp');
    const f = getVerboseConfig().filters;
    expect(f).toContain('shell');
    expect(f).toContain('mcp');
    expect(f).not.toContain('bad|token');
    const calls = ctx._spies.announceSystem!.mock
      .calls as unknown as unknown[][];
    const msg = calls[0]![0] as string;
    expect(msg).toContain('ignored: bad|token');
  });
});

describe('/verbosity config interactive menu', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('output sub-menu replaces the legacy master toggle with a "filter:all" pseudo-row', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:output');
    const arg = lastMenu(ctx) as { command?: SlashCommand; options: any[] };
    expect((arg as any).command.name).toBe('/verbosity');
    const values = menuValues(ctx);
    expect(values).not.toContain('__verbose_toggle__');
    // First row is the "all" pseudo-row.
    expect(arg.options[0].value).toBe('filter:all');
  });

  // Every category row reflects the master filter state: [on] under "all", and
  // [off] when the filter list is empty.
  it.each([
    [['all'], '[on]'],
    [[], '[off]'],
  ])('category rows show %s as %s', (filters, expectedDesc) => {
    setVerboseConfig({ filters });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:output');
    const categoryRows = lastMenu(ctx).options.filter((o) =>
      String(o.value).startsWith('category:')
    );
    expect(categoryRows.length).toBeGreaterThan(0);
    for (const opt of categoryRows) {
      expect(opt.description).toBe(expectedDesc);
    }
  });

  it('selecting a category from "all" baseline drops just that one', () => {
    const ctx = liteCtx();
    // category:shell args lands here when the user picks the "shell" row.
    runEffect(verbosityCmd, null, ctx, 'category:shell');
    const f = getVerboseConfig().filters;
    expect(f).not.toContain('shell');
    // Other categories survive the implicit "all" expansion.
    expect(f).toContain('mcp');
    // Menu re-opens automatically (the output sub-menu).
    expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
  });

  it('selecting a missing category from a narrowed list adds it', () => {
    setVerboseConfig({ filters: ['shell'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'category:mcp');
    const f = getVerboseConfig().filters;
    expect(f).toContain('shell');
    expect(f).toContain('mcp');
  });

  it('rejects an unknown category', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'category:totally-fake');
    const calls = ctx._spies.showAlert!.mock.calls as unknown as unknown[][];
    expect(calls[0]![0]).toContain('Unknown category');
    expect(calls[0]![1]).toBe('error');
  });
});

describe('/verbosity density presets', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('density CLI applies preset display config', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'density minimal');
    const display = getVerboseConfig().display!;
    expect(display.toolArgsMode).toBe('off');
    expect(display.showToolReasoning).toBe(false);
    expect(display.showElapsed).toBe(false);
    expect(display.subagent.prompts).toBe(false);
  });

  it('density lean resets filters to []', () => {
    // Picking a preset is now a clean reset to that preset's full intent
    // (display + filters). Custom filter lists survive only via the
    // Custom flow.
    setVerboseConfig({ filters: ['shell', 'mcp'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'density lean');
    expect(getVerboseConfig().filters).toEqual([]);
    const display = getVerboseConfig().display!;
    expect(display.toolArgsMode).toBe('inline');
  });

  it('density:<preset> from menu (CLI shortcut) re-opens the density menu', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'density:lean');
    const display = getVerboseConfig().display!;
    expect(display.toolArgsMode).toBe('inline');
    expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
  });

  it('density:apply:<preset> commits the preset and closes the menu', () => {
    // The post-confirmation Yes row routes through density:apply:<preset>.
    // Picking a preset is a finish action — close the menu so the user
    // lands at the prompt (the announceSystem call confirms it took
    // effect). Re-opening the density menu after commit had been read as
    // "did the click actually do anything?" by users.
    setVerboseConfig({ filters: ['shell', 'mcp'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'density:apply:default');
    const cfg = getVerboseConfig();
    expect(cfg.display!.toolArgsMode).toBe('block');
    // `default` preset's filter shape is ['shell'] — the fresh-install
    // default that streams shell stdout in a tail-windowed strip. Picking
    // the preset rewrites the filter list to that exact shape, dropping
    // the prior 'mcp' override.
    expect(cfg.filters).toEqual(['shell']);
    // Last setActiveCommand call must be null (closes the overlay).
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    expect(calls[calls.length - 1]?.[0]).toBeNull();
    // ESC return route is cleared so a stale menu:density route can't drop
    // the user back into the menu after they exit.
    const escCalls = ctx._spies.setVerboseReturnOnEscape!.mock
      .calls as unknown as unknown[][];
    expect(escCalls[escCalls.length - 1]?.[0]).toBeNull();
  });

  it('density without a preset arg shows an error', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'density');
    const calls = ctx._spies.showAlert!.mock.calls as unknown as unknown[][];
    expect(calls[0]![0]).toContain('density needs a preset');
  });

  it('density:full also writes filters: ["all"] (1:1 with parent agent)', () => {
    setVerboseConfig({ filters: [] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'density:full');
    const cfg = getVerboseConfig();
    // Display matches normal/full (everything on, no caps).
    expect(cfg.display!.showToolReasoning).toBe(true);
    expect(cfg.display!.toolArgsMode).toBe('block');
    expect(cfg.display!.showElapsed).toBe(true);
    // The differentiator: filter list collapses to ['all'].
    expect(cfg.filters).toEqual(['all']);
  });

  // density:<preset> colon form (Bug D): accepted as a CLI shortcut. The
  // accepted forms (lean/minimal/full) are covered by the tests above; this
  // pins the unknown-preset error path that the colon form must still reject.
  it('density:<unknown> (e.g. density:custom) surfaces an error alert', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'density:custom');
    const calls = ctx._spies.showAlert!.mock.calls as unknown as unknown[][];
    expect(calls[0]![0]).toContain('Unknown density preset');
    expect(calls[0]![1]).toBe('error');
  });
});

describe('/verbosity display flag toggles (set:)', () => {
  beforeEach(() => {
    setVerboseConfig({
      filters: ['all'],
      display: display({ outputMaxLines: null, argsMaxChars: 80 }),
    });
    resetVerboseCache();
  });

  it.each([
    [
      'set:showToolReasoning',
      (d: VerboseDisplayConfig) => d.showToolReasoning as unknown,
      false as unknown,
    ],
    [
      'set:toolArgsMode:inline',
      (d: VerboseDisplayConfig) => d.toolArgsMode as unknown,
      'inline' as unknown,
    ],
    [
      'set:toolArgsMode:off',
      (d: VerboseDisplayConfig) => d.toolArgsMode as unknown,
      'off' as unknown,
    ],
  ])('%s sets the display field', (route, accessor, expected) => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, route as string);
    expect(accessor(getVerboseConfig().display!)).toBe(expected);
  });

  it('set:subagent:prompts toggles only that section', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'set:subagent:prompts');
    const sub = getVerboseConfig().display!.subagent;
    expect(sub.prompts).toBe(false);
    // Other sections survive the toggle.
    expect(sub.pipeline).toBe(true);
    expect(sub.responses).toBe(true);
  });

  it('reset puts display back to defaults and empties filters', () => {
    setVerboseConfig({
      filters: ['shell'],
      display: display({
        showToolReasoning: false,
        toolArgsMode: 'off',
        showElapsed: false,
        subagent: {
          pipeline: false,
          prompts: false,
          roles: false,
          deps: false,
          responses: false,
        },
        outputMaxLines: null,
        argsMaxChars: 80,
      }),
    });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'reset');
    const cfg = getVerboseConfig();
    // `reset` applies the `default` preset, whose filter shape is ['shell']
    // (the fresh-install canonical state). The override 'shell' was already
    // present in the prior shape, but more importantly the filter list is
    // now rewritten to the preset shape rather than cleared.
    expect(cfg.filters).toEqual(['shell']);
    expect(cfg.display!.toolArgsMode).toBe('block');
    expect(cfg.display!.showToolReasoning).toBe(true);
    expect(cfg.display!.subagent.prompts).toBe(true);
  });
});

describe('/verbosity preset confirmation gate (replaces standalone reset)', () => {
  beforeEach(() => {
    resetVerboseCache();
    // Custom shape so bare /verbosity opens the config menu — keeps the
    // legacy assertions about "config menu has no Reset row" relevant.
    setVerboseConfig({
      filters: [],
      display: display({
        showToolReasoning: false,
        toolArgsMode: 'off',
        showElapsed: false,
        outputMaxLines: null,
        argsMaxChars: 80,
      }),
    });
  });

  it('config menu no longer has a Reset row — the default preset replaces it', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'config');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const labels = arg.options.map((o) => o.label);
    expect(labels).not.toContain('Reset to defaults');
  });

  it('menu:density:confirm:default opens the confirmation submenu without mutating', () => {
    setVerboseConfig({ filters: ['shell'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:density:confirm:default');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    expect(arg.options[0].label).toBe('Cancel');
    expect(arg.options[0].value).toBe('menu:density');
    const yes = arg.options.find((o) => o.value === 'density:apply:default');
    expect(yes).toBeDefined();
    // Confirm did not commit — filters unchanged.
    expect(getVerboseConfig().filters).toEqual(['shell']);
  });

  it('Cancel from the confirmation returns to the density menu without resetting', () => {
    setVerboseConfig({ filters: ['shell'] });
    const ctx = liteCtx();
    // Cancel routes back through menu:density.
    runEffect(verbosityCmd, null, ctx, 'menu:density');
    expect(getVerboseConfig().filters).toEqual(['shell']);
    const announce = ctx._spies.announceSystem!.mock
      .calls as unknown as unknown[][];
    const announced = announce.map((c) => c[0] as string).join(' ');
    expect(announced).not.toContain('reset to defaults');
  });

  it('density:apply:default applies the default-preset reset and announces it', () => {
    setVerboseConfig({
      filters: ['shell'],
      display: display({
        showToolReasoning: false,
        toolArgsMode: 'off',
        showElapsed: false,
        subagent: {
          pipeline: false,
          prompts: false,
          roles: false,
          deps: false,
          responses: false,
        },
        outputMaxLines: null,
        argsMaxChars: 80,
      }),
    });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'density:apply:default');
    const cfg = getVerboseConfig();
    // `default` preset rewrites filters to its canonical ['shell'] shape.
    expect(cfg.filters).toEqual(['shell']);
    expect(cfg.display!.toolArgsMode).toBe('block');
    const announce = ctx._spies.announceSystem!.mock
      .calls as unknown as unknown[][];
    const announced = announce.map((c) => c[0] as string).join(' ');
    expect(announced).toContain('density set to default');
  });

  it('CLI /verbosity reset still works as the power-user shortcut for the default preset', () => {
    // The standalone Reset menu row is gone, but typing `/verbosity reset`
    // is preserved as a one-shot shortcut — the user opted in by typing.
    setVerboseConfig({ filters: ['shell', 'mcp'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'reset');
    const cfg = getVerboseConfig();
    // Reset is the default preset, whose filter shape is ['shell'].
    expect(cfg.filters).toEqual(['shell']);
    expect(cfg.display!.toolArgsMode).toBe('block');
  });
});

describe('/verbosity drilldown menus', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  // Each drilldown menu opens with a fixed set of routing rows plus a
  // section-encoded back-link. One table asserts the menu shape per route;
  // per-route behavior (toggling, mutation, [active] markers) is tested
  // separately below.
  it.each([
    {
      route: 'menu:tool',
      required: [
        'set:showToolReasoning',
        'set:showElapsed',
        'set:toolArgsMode:off',
        'set:toolArgsMode:inline',
        'set:toolArgsMode:block',
        'menu:top:tool',
      ],
      forbidden: [],
    },
    {
      // Master step toggle, its two nested rows (instructions + role labels),
      // responses, and the full-output toggle (mirrors the `subagent` filter).
      // The old "deps" row is gone — dependency arrows are always-on now.
      route: 'menu:subagent',
      required: [
        'set:subagent:pipeline',
        'set:subagent:prompts',
        'set:subagent:roles',
        'set:subagent:responses',
        'set:subagent:fullOutput',
        'menu:top:subagent',
      ],
      forbidden: ['set:subagent:deps'],
    },
  ])(
    '$route opens its drilldown rows + back-link',
    ({ route, required, forbidden }) => {
      const ctx = liteCtx();
      runEffect(verbosityCmd, null, ctx, route);
      const values = menuValues(ctx);
      for (const v of required) expect(values).toContain(v);
      for (const v of forbidden) expect(values).not.toContain(v);
    }
  );

  it('friendly section names jump straight into their sub-menu', () => {
    // Mirrors the breadcrumb so nested menus are reachable as typed
    // subcommands: `/verbosity truncation`, `/verbosity tool`, etc. open the
    // same menu as the internal `menu:<section>` route. `density` is
    // intentionally excluded (bare `density` is the CLI set-preset form).
    const cases: Array<[string, string]> = [
      ['tool', 'menu:top:tool'],
      ['tools', 'menu:top:tool'],
      ['tool calls', 'menu:top:tool'],
      ['subagent', 'menu:top:subagent'],
      ['output', 'menu:top:output'],
      ['truncation', 'menu:top:truncation'],
    ];
    for (const [arg, backLink] of cases) {
      const ctx = liteCtx();
      runEffect(verbosityCmd, null, ctx, arg);
      const calls = ctx._spies.setActiveCommand!.mock
        .calls as unknown as unknown[][];
      expect(calls.length).toBeGreaterThan(0);
      const opened = calls[calls.length - 1]![0] as { options: any[] };
      const values = opened.options.map((o) => o.value);
      // The back-link encodes the section, proving we opened that sub-menu.
      expect(values).toContain(backLink);
    }
  });

  it('bare "density" stays the CLI set-preset form, not a menu jump', () => {
    // Guard the deliberate exclusion: `density` without a preset errors
    // rather than opening the density menu.
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'density');
    const alerts = ctx._spies.showAlert!.mock.calls as unknown as unknown[][];
    expect(alerts[0]![0]).toContain('density needs a preset');
  });

  it('menu:subagent hides nested rows when pipeline is off', () => {
    // Dropping pipeline (the master "Show subagent steps" toggle) makes
    // prompts and roles dead toggles — the renderer wraps both in
    // `if (sub.pipeline && ...)`. Reflect that in the menu so the user
    // can't toggle a row that wouldn't change the output.
    setVerboseConfig({
      display: display({
        subagent: { pipeline: false },
        outputMaxLines: null,
        argsMaxChars: 80,
      }),
    });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:subagent');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const values = arg.options.map((o) => o.value);
    // Master + responses + fullOutput stay visible.
    expect(values).toContain('set:subagent:pipeline');
    expect(values).toContain('set:subagent:responses');
    expect(values).toContain('set:subagent:fullOutput');
    // Nested rows hidden.
    expect(values).not.toContain('set:subagent:prompts');
    expect(values).not.toContain('set:subagent:roles');
  });

  it('set:subagent:fullOutput toggles the subagent filter token', () => {
    setVerboseConfig({ filters: [] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'set:subagent:fullOutput');
    expect(getVerboseConfig().filters).toContain('subagent');
    runEffect(verbosityCmd, null, ctx, 'set:subagent:fullOutput');
    expect(getVerboseConfig().filters).not.toContain('subagent');
  });

  it('set:subagent:fullOutput from "all" baseline drops just subagent', () => {
    // Same expansion semantics as category:<name> toggles — turning off a
    // single category from "all" must explicitly enumerate the rest so the
    // user doesn't end up with a no-op (everything still allowed).
    setVerboseConfig({ filters: ['all'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'set:subagent:fullOutput');
    const f = getVerboseConfig().filters;
    expect(f).not.toContain('subagent');
    expect(f).not.toContain('all');
    // Other categories survive the implicit expansion.
    expect(f).toContain('shell');
    expect(f).toContain('mcp');
  });

  it('menu:density shows the four presets (routed to the confirm gate), marks the active one, and lists Custom', () => {
    setVerboseConfig({ display: undefined, filters: [] });
    resetVerboseCache();
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:density');
    const values = menuValues(ctx);
    // Each preset row routes to the confirmation gate (menu:density:confirm:*),
    // NOT a direct density:<preset> commit — the `menu:` prefix is required or
    // the route falls through to "Unknown subcommand" (a past regression).
    for (const p of ['minimal', 'lean', 'default', 'full']) {
      expect(values).toContain(`menu:density:confirm:${p}`);
    }
    // Legacy `normal`/`verbose` identifiers are gone (renamed to default/full).
    expect(values).not.toContain('menu:density:confirm:normal');
    expect(values).not.toContain('menu:density:confirm:verbose');
    // Custom row routes to the config (per-knob) menu.
    expect(lastMenu(ctx).options.find((o) => o.label === 'custom')?.value).toBe(
      'menu:config'
    );
    // 'default' = DEFAULT_DISPLAY + filters: ['shell'] (fresh-install shape);
    // re-open under that shape to verify the [active] marker lands on `default`.
    setVerboseConfig({ filters: ['shell'] });
    const ctx2 = liteCtx();
    runEffect(verbosityCmd, null, ctx2, 'menu:density');
    expect(
      lastMenu(ctx2).options.find(
        (o) => o.value === 'menu:density:confirm:default'
      )?.description
    ).toContain('[active]');
  });
});

describe('/verbosity ESC navigation flag', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  // ESC return-route contract (CommandMenu re-dispatches this route on ESC):
  //   - top + density (top-level entries) → null (fully exits)
  //   - each config submenu → menu:top:<key> (re-opens config on its row)
  //   - preset confirmation → menu:density (back to density menu)
  //   - toggling a per-knob setting re-arms the submenu's return route so ESC
  //     after a toggle goes back one level, not fully out.
  it.each([
    ['', null],
    ['menu:density', null],
    ['menu:tool', 'menu:top:tool'],
    ['menu:subagent', 'menu:top:subagent'],
    ['menu:output', 'menu:top:output'],
    ['menu:truncation', 'menu:top:truncation'],
    ['menu:density:confirm:lean', 'menu:density'],
    ['set:showToolReasoning', 'menu:top:tool'],
    // Editor submenu returns one level up to its parent section menu.
    ['menu:truncation:argsLines:edit', 'menu:truncation'],
  ] as const)('%p arms verboseReturnOnEscape to %p', (arg, route) => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, arg);
    const calls = ctx._spies.setVerboseReturnOnEscape!.mock
      .calls as unknown as unknown[][];
    const last = calls[calls.length - 1];
    expect(last?.[0]).toBe(route);
  });
});

describe('/verbosity Truncation submenu', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('config menu includes a Truncation row as the last section', () => {
    // Reset row is gone — Truncation is now the final row in the config
    // menu (the density preset replaces the standalone reset).
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'config');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const labels = arg.options.map((o) => o.label);
    const truncIdx = labels.indexOf('Truncation');
    expect(truncIdx).toBeGreaterThanOrEqual(0);
    // No Reset row anywhere.
    expect(labels).not.toContain('Reset to defaults');
    // Truncation is the last labeled row.
    expect(truncIdx).toBe(labels.length - 1);
  });

  it('Truncation row description summarizes both caps', () => {
    setVerboseConfig({
      display: display({ outputMaxLines: 20, argsMaxChars: 80 }),
    });
    const ctx = liteCtx();
    // Use the explicit `config` route — bare /verbosity may open density
    // when the saved shape happens to match a preset, and density doesn't
    // surface the per-section Truncation row summary.
    runEffect(verbosityCmd, null, ctx, 'config');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const truncRow = arg.options.find((o) => o.label === 'Truncation');
    expect(truncRow?.description).toContain('args unlimited/80 chars');
    expect(truncRow?.description).toContain('output 20 lines/unlimited');
  });

  it('menu:truncation lists all four knobs with current values', () => {
    setVerboseConfig({
      display: display({
        argsMaxLines: 7,
        outputMaxLines: null,
        argsMaxChars: 80,
      }),
    });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:truncation');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const argsLines = arg.options.find((o) => o.label === 'Args · lines');
    const argsChars = arg.options.find(
      (o) => o.label === 'Args · chars per value'
    );
    const outLines = arg.options.find((o) => o.label === 'Output · lines');
    const outChars = arg.options.find(
      (o) => o.label === 'Output · chars per line'
    );
    expect(argsLines?.description).toBe('7 lines');
    expect(argsChars?.description).toBe('80 chars');
    expect(outLines?.description).toBe('unlimited');
    expect(outChars?.description).toBe('unlimited');
    // Back-link present so the user can return to the top menu without ESC.
    const values = arg.options.map((o) => o.value);
    expect(values).toContain('menu:top:truncation');
  });

  // The preset menu was replaced with a numeric editor (rendered by
  // CommandMenu when previewKey ends with `:edit`). The effect handler marks
  // the route by setting previewKey on the active command — the option list is
  // a single placeholder; all keypresses are handled by the editor component.
  it.each([
    ['menu:truncation:argsLines:edit', 'truncation:argsLines:edit'],
    ['menu:truncation:outputLines:edit', 'truncation:outputLines:edit'],
  ])('%s opens the numeric editor (sets previewKey)', (route, expectedKey) => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, route);
    expect(lastMenu(ctx).previewKey).toBe(expectedKey);
  });

  it.each([
    // [route, accessor, expected, seed display]
    [
      'set:argsMaxLines:20',
      (d: VerboseDisplayConfig) => d.argsMaxLines as unknown,
      20 as unknown,
      undefined as VerboseDisplayConfig | undefined,
    ],
    [
      'set:outputMaxLines:null',
      (d: VerboseDisplayConfig) => d.outputMaxLines as unknown,
      null as unknown,
      display({ outputMaxLines: 50, argsMaxChars: 80 }),
    ],
    // Seeds toolArgsMode:'inline' so the cap clear applies to the inline path.
    [
      'set:argsMaxChars:null',
      (d: VerboseDisplayConfig) => d.argsMaxChars as unknown,
      null as unknown,
      display({
        toolArgsMode: 'inline',
        outputMaxLines: null,
        argsMaxChars: 80,
      }),
    ],
  ])(
    '%s writes the field (committed from editor)',
    (route, accessor, expected, seed) => {
      if (seed) setVerboseConfig({ display: seed });
      const ctx = liteCtx();
      runEffect(verbosityCmd, null, ctx, route as string);
      expect(accessor(getVerboseConfig().display!)).toBe(expected);
      // A set: route reopens its menu after committing.
      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
    }
  );

  // ESC return-route arming for menu:truncation and its edit submenu is
  // covered by the parameterized table in `/verbosity ESC navigation flag`.
});

describe('/verbosity top menu Output filters row summary', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  // Explicit `config` route — bare /verbosity would open the density menu
  // when the saved shape matches a preset, and density doesn't surface the
  // per-section filter summary.
  it.each([
    [[], 'none'],
    [['all'], 'all'],
    [['shell', 'mcp', 'read'], 'shell, mcp, read'],
  ])('Show output row summarizes filters %j as %p', (filters, expected) => {
    setVerboseConfig({ filters });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'config');
    expect(rowDesc(ctx, 'Show output')).toBe(expected);
  });
});

describe('/verbosity long filter announcement (count-based truncation)', () => {
  let originalColumns: number | undefined;

  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: [] });
    // Pin terminal width small so the truncation threshold kicks in
    // deterministically regardless of the runner's actual terminal.
    originalColumns = process.stdout.columns;
    Object.defineProperty(process.stdout, 'columns', {
      configurable: true,
      writable: true,
      value: 80,
    });
  });

  afterEach(() => {
    if (originalColumns !== undefined) {
      Object.defineProperty(process.stdout, 'columns', {
        configurable: true,
        writable: true,
        value: originalColumns,
      });
    }
  });

  it('truncates with a "+N more" suffix when the joined form is too long', () => {
    const ctx = liteCtx();
    runEffect(
      verbosityCmd,
      null,
      ctx,
      'only shell read write web grep glob code introspect task subagent mcp'
    );
    const calls = ctx._spies.announceSystem!.mock
      .calls as unknown as unknown[][];
    const msg = calls[calls.length - 1]![0] as string;
    expect(msg).toMatch(/filters: \d+ \(.*\.\.\. \+\d+ more\)/);
    expect(msg).toContain('shell');
    expect(msg).toContain('+');
    expect(msg).toContain('more');
  });

  it('keeps the joined form when it fits the budget', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'only shell mcp');
    const calls = ctx._spies.announceSystem!.mock
      .calls as unknown as unknown[][];
    const msg = calls[calls.length - 1]![0] as string;
    expect(msg).toContain('filters: shell, mcp');
    expect(msg).not.toContain('more)');
  });
});

describe('/verbosity top menu Subagent row summary', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  // `config` route — these probe the per-section row summaries in the config
  // menu, which bare /verbosity may bypass when the saved shape matches a
  // preset. Summary reads top-down: step list + nested labels, then summary,
  // then full-output (no "only" / "pipeline · prompts · ..." internal wording).
  it.each([
    [
      'every knob on',
      { filters: ['all'] as string[], subagent: {} },
      'steps + instructions + roles · summary · full output',
    ],
    [
      'only steps on collapses nested labels',
      {
        filters: [] as string[],
        subagent: {
          prompts: false,
          roles: false,
          deps: false,
          responses: false,
        },
      },
      'steps',
    ],
    [
      'all sections off and no full-output filter',
      {
        filters: [] as string[],
        subagent: {
          pipeline: false,
          prompts: false,
          roles: false,
          deps: false,
          responses: false,
        },
      },
      '(all hidden)',
    ],
  ])('Subagent row summary: %s', (_name, { filters, subagent }, expected) => {
    setVerboseConfig({
      filters,
      display: display({ subagent, outputMaxLines: null, argsMaxChars: 80 }),
    });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'config');
    expect(rowDesc(ctx, 'Subagent')).toBe(expected);
  });
});

describe('/verbosity unknown-token soft warnings', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: [] });
  });

  it('warns when typo tokens are saved', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'only foo bar');
    // Filters still applied — soft warning, not rejection.
    expect(getVerboseConfig().filters).toEqual(['foo', 'bar']);
    const calls = ctx._spies.announceSystem!.mock
      .calls as unknown as unknown[][];
    const msg = calls[calls.length - 1]![0] as string;
    expect(msg).toContain('warning');
    expect(msg).toContain('foo');
    expect(msg).toContain('bar');
  });

  it('does not warn for known categories', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'only shell mcp');
    const calls = ctx._spies.announceSystem!.mock
      .calls as unknown as unknown[][];
    const msg = calls[calls.length - 1]![0] as string;
    expect(msg).not.toContain('warning');
  });

  it('does not warn for mcp__-prefixed exact tool names', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'only mcp__nova-memory-mcp__remember');
    const calls = ctx._spies.announceSystem!.mock
      .calls as unknown as unknown[][];
    const msg = calls[calls.length - 1]![0] as string;
    expect(msg).not.toContain('warning');
  });
});

describe('/verbosity unknown subcommand', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('shows an error alert listing valid forms', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'wat-is-this');
    const calls = ctx._spies.showAlert!.mock.calls as unknown as unknown[][];
    expect(calls[0]![0]).toContain('Unknown /verbosity subcommand');
    expect(calls[0]![1]).toBe('error');
  });
});

describe('/verbosity case-insensitive command verbs', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  // The verb is folded to lowercase; uppercase/mixed-case forms behave like
  // the canonical lowercase verb. (Token-case preservation is a SEPARATE
  // regression — see the two `only ...` tests below.)
  it.each([
    ['ON', { filters: [] }, (c: any) => expect(c.filters).toEqual(['all'])],
    ['OFF', { filters: ['all'] }, (c: any) => expect(c.filters).toEqual([])],
    [
      'ALL',
      { filters: ['shell'] },
      (c: any) => expect(c.filters).toEqual(['all']),
    ],
    [
      'DENSITY lean',
      { filters: ['all'] },
      (c: any) => expect(c.display!.toolArgsMode).toBe('inline'),
    ],
    [
      'DENSITY:lean',
      { filters: ['all'] },
      (c: any) => expect(c.display!.toolArgsMode).toBe('inline'),
    ],
    // `Status` folds to `status`: a read-only verb that must not mutate.
    [
      'Status',
      { filters: ['shell'] },
      (c: any) => expect(c.filters).toEqual(['shell']),
    ],
  ] as const)('/verbosity %s folds the verb', (arg, initial, check) => {
    setVerboseConfig(initial as any);
    runEffect(verbosityCmd, null, liteCtx(), arg);
    check(getVerboseConfig());
  });

  // The verb may be any case, but the filter token after it must reach the
  // saved config exactly as typed — MCP tool names (and category names) are
  // case-sensitive. This is a distinct regression from verb-folding above.
  it.each([
    ['ONLY mcp__SomeCase__tool', 'mcp__SomeCase__tool'],
    ['only Shell', 'Shell'],
  ])('/verbosity %s preserves the filter token case', (arg, token) => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, arg);
    const f = getVerboseConfig().filters;
    expect(f).toContain(token);
    expect(f).not.toContain(token.toLowerCase());
  });
});

// Sanity test: the help-panel local-command merge skips liteOnly entries when
// the UI mode is 'tui'. Covers the gating change in effects.ts:showHelpPanel.
describe('help merge gates liteOnly commands', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('omits /verbosity from help in TUI mode', () => {
    const helpCmd: SlashCommand = {
      name: '/help',
      description: 'Show help',
      source: 'backend',
    };
    const ctx = createMockCommandContext({
      slashCommands: [helpCmd, verbosityCmd],
    });
    // Default getUiMode is 'tui'.
    const result = {
      success: true,
      message: 'Help',
      data: {
        commands: [{ name: '/help', description: 'Show help', usage: '/help' }],
      },
    };
    runEffect(helpCmd, result, ctx, '');
    const call = ctx._spies.setShowHelpPanel!.mock.calls[0]!;
    const merged = call[1] as Array<{ name: string }>;
    expect(merged.find((c) => c.name === '/verbosity')).toBeUndefined();
  });

  it('includes /verbosity in help in lite mode', () => {
    const helpCmd: SlashCommand = {
      name: '/help',
      description: 'Show help',
      source: 'backend',
    };
    const ctx = liteCtx();
    (ctx as any).slashCommands = [helpCmd, verbosityCmd];
    const result = {
      success: true,
      message: 'Help',
      data: {
        commands: [{ name: '/help', description: 'Show help', usage: '/help' }],
      },
    };
    runEffect(helpCmd, result, ctx, '');
    const call = ctx._spies.setShowHelpPanel!.mock.calls[0]!;
    const merged = call[1] as Array<{ name: string }>;
    expect(merged.find((c) => c.name === '/verbosity')).toBeDefined();
  });
});

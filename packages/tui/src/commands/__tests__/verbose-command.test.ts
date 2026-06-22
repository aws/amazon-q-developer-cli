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

  // on/off are CLI aliases onto the filter list (the master enabled toggle is
  // gone): on→['all'], off→[].
  it.each([
    ['on', [] as string[], ['all'] as string[]],
    ['off', ['all'] as string[], [] as string[]],
  ])('/verbosity %s sets filters to %j', (verb, seed, expected) => {
    setVerboseConfig({ filters: [...seed] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, verb);
    expect(getVerboseConfig().filters).toEqual(expected);
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

  // only/all/add/remove filter mutations. `contains`/`excludes` assert
  // membership; `equals` pins the exact list when order/clearing matters.
  const filterCases: Array<{
    name: string;
    seed?: string[];
    arg: string;
    contains?: string[];
    excludes?: string[];
    equals?: string[];
  }> = [
    {
      name: 'only <list> replaces filters (preserving mcp__ token)',
      arg: 'only shell mcp__nova-memory-mcp__recall',
      contains: ['shell', 'mcp__nova-memory-mcp__recall'],
      excludes: ['all'],
    },
    {
      name: 'all resets filters to ["all"]',
      seed: ['shell'],
      arg: 'all',
      equals: ['all'],
    },
    {
      name: 'add appends without dropping existing',
      seed: ['shell'],
      arg: 'add mcp',
      contains: ['shell', 'mcp'],
    },
    {
      // From "all" baseline we treat the union as the new explicit list, else
      // normalization would collapse back to all and adding a category is a no-op.
      name: 'add from "all" baseline collapses to explicit tokens',
      seed: ['all'],
      arg: 'add shell',
      contains: ['shell'],
      excludes: ['all'],
    },
    {
      name: 'remove drops listed tokens',
      seed: ['shell', 'mcp', 'read'],
      arg: 'remove mcp',
      contains: ['shell', 'read'],
      excludes: ['mcp'],
    },
    {
      name: 'removing the last filter leaves filters empty (off)',
      seed: ['shell'],
      arg: 'remove shell',
      equals: [],
    },
  ];
  it.each(filterCases)('$name', ({ seed, arg, contains, excludes, equals }) => {
    if (seed) setVerboseConfig({ filters: [...seed] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, arg);
    const f = getVerboseConfig().filters;
    if (equals) expect(f).toEqual(equals);
    for (const t of contains ?? []) expect(f).toContain(t);
    for (const t of excludes ?? []) expect(f).not.toContain(t);
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

  it('output sub-menu first row is the "filter:all" pseudo-row', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:output');
    const arg = lastMenu(ctx) as { command?: SlashCommand; options: any[] };
    expect((arg as any).command.name).toBe('/verbosity');
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

  // A category row toggles its token: present → dropped, absent → added.
  // From the implicit "all" baseline the toggle expands to the explicit
  // category set first so the others survive. Selecting always re-opens the
  // output sub-menu so the user can keep toggling.
  it.each<{
    name: string;
    seed?: string[];
    route: string;
    contains: string[];
    excludes: string[];
  }>([
    {
      name: 'from "all" baseline drops just that one',
      route: 'category:shell',
      contains: ['mcp'],
      excludes: ['shell'],
    },
    {
      name: 'from a narrowed list adds a missing one',
      seed: ['shell'],
      route: 'category:mcp',
      contains: ['shell', 'mcp'],
      excludes: [],
    },
  ])('selecting a category $name', ({ seed, route, contains, excludes }) => {
    if (seed) setVerboseConfig({ filters: [...seed] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, route);
    const f = getVerboseConfig().filters;
    for (const t of contains) expect(f).toContain(t);
    for (const t of excludes) expect(f).not.toContain(t);
    // Menu re-opens automatically (the output sub-menu).
    expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
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

  // Applying a preset (CLI `density <preset>` verb form, the `density:<preset>`
  // colon shortcut, or the post-confirmation `density:apply:<preset>` Yes row)
  // is a clean reset to that preset's full intent: display config AND filter
  // list. Each row pins one preset's distinctive display + filter shape.
  const presetCases: Array<{
    route: string;
    seedFilters?: string[];
    filters?: string[];
    display?: Record<string, unknown>;
    sub?: Record<string, unknown>;
  }> = [
    {
      route: 'density minimal',
      display: { toolArgsMode: 'off', showToolReasoning: false },
      sub: { prompts: false },
    },
    {
      route: 'density lean',
      seedFilters: ['shell', 'mcp'],
      // Custom filter lists survive only via the Custom flow; preset clears.
      filters: [],
      display: { toolArgsMode: 'inline' },
    },
    {
      route: 'density:lean',
      display: { toolArgsMode: 'inline' },
    },
    {
      // `full` is differentiated from `default` solely by its filter list
      // collapsing to ['all'] (1:1 with what the parent agent sees).
      route: 'density:full',
      seedFilters: [],
      filters: ['all'],
      display: {
        showToolReasoning: true,
        toolArgsMode: 'block',
        showElapsed: true,
      },
    },
    {
      // CLI `/verbosity reset` is an alias for the `default` preset: it
      // rewrites display to DEFAULT_DISPLAY and the filter list to the
      // fresh-install shape (['shell']), regardless of the prior shape.
      route: 'reset',
      seedFilters: ['shell', 'mcp', 'read'],
      filters: ['shell'],
      display: {
        toolArgsMode: 'block',
        showToolReasoning: true,
        showElapsed: true,
      },
      sub: { prompts: true },
    },
  ];
  it.each(presetCases)(
    '$route applies the preset display + filter shape',
    ({ route, seedFilters, filters, display: disp, sub }) => {
      if (seedFilters) setVerboseConfig({ filters: [...seedFilters] });
      const ctx = liteCtx();
      runEffect(verbosityCmd, null, ctx, route);
      const cfg = getVerboseConfig();
      for (const [k, v] of Object.entries(disp ?? {})) {
        expect((cfg.display as any)[k]).toBe(v);
      }
      if (sub) {
        for (const [k, v] of Object.entries(sub)) {
          expect((cfg.display!.subagent as any)[k]).toBe(v);
        }
      }
      if (filters) expect(cfg.filters).toEqual(filters);
    }
  );

  it('density:<preset> menu shortcut re-opens the density menu (does not close)', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'density:lean');
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
    // The switch is announced — the announcement is what tells the user it
    // took effect (the menu closes, so there's no visible menu confirmation).
    const announced = (
      ctx._spies.announceSystem!.mock.calls as unknown as unknown[][]
    )
      .map((c) => c[0] as string)
      .join(' ');
    expect(announced).toContain('density set to default');
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

  it.each([
    ['density', 'density needs a preset'],
    ['density:custom', 'Unknown density preset'],
  ])('%p surfaces an error alert', (route, expected) => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, route);
    const calls = ctx._spies.showAlert!.mock.calls as unknown as unknown[][];
    expect(calls[0]![0]).toContain(expected);
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
});

describe('/verbosity drilldown menus', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  // Each drilldown menu opens with a fixed set of routing rows plus a
  // section-encoded back-link. The internal `menu:<section>` route and the
  // friendly breadcrumb aliases users type (`/verbosity tool`, `tools`, `tool
  // calls`, `truncation`, ...) all open the same menu; the back-link encodes
  // the section, proving the right sub-menu opened. `density` is intentionally
  // excluded (bare `density` is the CLI set-preset form). Per-route behavior
  // (toggling, mutation, [active] markers) is tested separately below.
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
    // Friendly breadcrumb aliases — only the back-link is asserted (the row
    // shape is already covered by the canonical menu:* rows above).
    { route: 'tool', required: ['menu:top:tool'], forbidden: [] },
    { route: 'tools', required: ['menu:top:tool'], forbidden: [] },
    { route: 'tool calls', required: ['menu:top:tool'], forbidden: [] },
    { route: 'subagent', required: ['menu:top:subagent'], forbidden: [] },
    { route: 'output', required: ['menu:top:output'], forbidden: [] },
    { route: 'truncation', required: ['menu:top:truncation'], forbidden: [] },
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

  it('set:subagent:fullOutput from "all" baseline drops just subagent', () => {
    // fullOutput piggybacks on the `subagent` filter token via the same
    // toggleFilterToken path as category:<name> rows. The distinct regression
    // here is the expansion: turning off a single category from "all" must
    // explicitly enumerate the rest so the user doesn't end up with a no-op
    // (everything still allowed). The plain empty→subagent→empty round-trip is
    // covered by the category-toggle table above.
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

describe('/verbosity Truncation submenu', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('Truncation is the last config row and its description summarizes both caps', () => {
    // Reset row is gone — Truncation is now the final row in the config menu
    // (the density preset replaces the standalone reset). Use the explicit
    // `config` route — bare /verbosity may open density when the saved shape
    // matches a preset, and density doesn't surface the per-section summary.
    setVerboseConfig({
      display: display({ outputMaxLines: 20, argsMaxChars: 80 }),
    });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'config');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const labels = arg.options.map((o) => o.label);
    const truncIdx = labels.indexOf('Truncation');
    expect(truncIdx).toBe(labels.length - 1);
    const truncRow = arg.options[truncIdx];
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
});

// Per-section row summaries in the top/config menu. All use the explicit
// `config` route — bare /verbosity may open the density menu when the saved
// shape matches a preset, and density doesn't surface these per-section rows.
// The Subagent summary reads top-down: step list + nested labels, then summary,
// then full-output (no "only" / internal "pipeline · prompts · ..." wording).
describe('/verbosity config-menu row summaries', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it.each<{
    name: string;
    filters: string[];
    subagent?: Record<string, unknown>;
    rowLabel: string;
    expected: string;
  }>([
    {
      name: 'output none',
      filters: [],
      rowLabel: 'Show output',
      expected: 'none',
    },
    {
      name: 'output all',
      filters: ['all'],
      rowLabel: 'Show output',
      expected: 'all',
    },
    {
      name: 'output explicit list',
      filters: ['shell', 'mcp', 'read'],
      rowLabel: 'Show output',
      expected: 'shell, mcp, read',
    },
    {
      name: 'subagent every knob on',
      filters: ['all'],
      subagent: {},
      rowLabel: 'Subagent',
      expected: 'steps + instructions + roles · summary · full output',
    },
    {
      name: 'subagent only steps on collapses nested labels',
      filters: [],
      subagent: { prompts: false, roles: false, deps: false, responses: false },
      rowLabel: 'Subagent',
      expected: 'steps',
    },
    {
      name: 'subagent all sections off and no full-output filter',
      filters: [],
      subagent: {
        pipeline: false,
        prompts: false,
        roles: false,
        deps: false,
        responses: false,
      },
      rowLabel: 'Subagent',
      expected: '(all hidden)',
    },
  ])(
    '$name → $rowLabel summarizes as "$expected"',
    ({ filters, subagent, rowLabel, expected }) => {
      setVerboseConfig({
        filters,
        ...(subagent
          ? {
              display: display({
                subagent,
                outputMaxLines: null,
                argsMaxChars: 80,
              }),
            }
          : {}),
      });
      const ctx = liteCtx();
      runEffect(verbosityCmd, null, ctx, 'config');
      expect(rowDesc(ctx, rowLabel)).toBe(expected);
    }
  );
});

describe('/verbosity unknown-token soft warnings', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: [] });
  });

  // Unknown tokens are saved (soft warning, not rejection) but flagged; known
  // categories and mcp__-prefixed exact tool names must NOT warn.
  it.each([
    { arg: 'only foo bar', warns: true, mentions: ['foo', 'bar'] },
    { arg: 'only shell mcp', warns: false },
    { arg: 'only mcp__nova-memory-mcp__remember', warns: false },
  ])('$arg warns=$warns', ({ arg, warns, mentions }) => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, arg);
    const calls = ctx._spies.announceSystem!.mock
      .calls as unknown as unknown[][];
    const msg = calls[calls.length - 1]![0] as string;
    if (warns) {
      expect(msg).toContain('warning');
      for (const t of mentions ?? []) expect(msg).toContain(t);
    } else {
      expect(msg).not.toContain('warning');
    }
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

  // The liteOnly /verbosity entry is merged into help only in lite mode; TUI
  // mode omits it. ctx differs only by getUiMode (tui default vs lite).
  it.each([
    ['tui', false],
    ['lite', true],
  ] as const)('%s mode includesVerbosity=%s', (uiMode, shouldContain) => {
    const helpCmd: SlashCommand = {
      name: '/help',
      description: 'Show help',
      source: 'backend',
    };
    const ctx = uiMode === 'lite' ? liteCtx() : createMockCommandContext({});
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
    const found = merged.find((c) => c.name === '/verbosity');
    if (shouldContain) expect(found).toBeDefined();
    else expect(found).toBeUndefined();
  });
});

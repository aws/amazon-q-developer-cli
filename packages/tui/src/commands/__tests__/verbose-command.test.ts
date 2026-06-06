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
  getVerboseConfig,
  resetVerboseCache,
  setVerboseConfig,
} from '../../lite/verbose.js';
import type { SlashCommand } from '../../stores/app-store.js';
import { createMockCommandContext } from './test-helpers.js';

const verbosityCmd: SlashCommand = {
  name: '/verbosity',
  description: '',
  source: 'local' as const,
  meta: { local: true, liteOnly: true },
};

// Backward-compat alias used by alias-specific tests below.
const verboseAliasCmd: SlashCommand = {
  name: '/verbose',
  description: '',
  source: 'local' as const,
  meta: { local: true, liteOnly: true, hidden: true },
};

function liteCtx() {
  const ctx = createMockCommandContext({ slashCommands: [verbosityCmd] });
  // Override the default 'tui' to 'lite' so the verbosity handler's lite-only
  // gate accepts the call.
  (ctx as any).getUiMode = () => 'lite';
  return ctx;
}

describe('/verbose lite-mode gate', () => {
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

describe('/verbose top menu and status', () => {
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
      display: {
        ...require('../../lite/verbose.js').DEFAULT_DISPLAY,
        showElapsed: false,
      },
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

  it('/verbose on sets filters to ["all"]', () => {
    setVerboseConfig({ filters: [] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'on');
    expect(getVerboseConfig().filters).toEqual(['all']);
  });

  it('/verbose off sets filters to []', () => {
    setVerboseConfig({ filters: ['all'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'off');
    expect(getVerboseConfig().filters).toEqual([]);
  });

  it('/verbose status announces filters without an ON/OFF prefix', () => {
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

describe('/verbose filter mutations', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('/verbose only <list> replaces filters', () => {
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

  it('/verbose all resets filters', () => {
    setVerboseConfig({ filters: ['shell'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'all');
    expect(getVerboseConfig().filters).toEqual(['all']);
  });

  it('/verbose add appends without dropping existing', () => {
    setVerboseConfig({ filters: ['shell'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'add mcp');
    const f = getVerboseConfig().filters;
    expect(f).toContain('shell');
    expect(f).toContain('mcp');
  });

  it('/verbose add from "all" baseline collapses to the new tokens', () => {
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

  it('/verbose remove drops listed tokens', () => {
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

describe('/verbose config interactive menu', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('output sub-menu has no master toggle row and lists every category with [on] when "all" is active', () => {
    const ctx = liteCtx();
    // Drill into the output filter sub-menu.
    runEffect(verbosityCmd, null, ctx, 'menu:output');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    expect(calls.length).toBeGreaterThanOrEqual(1);
    const arg = calls[calls.length - 1]![0] as {
      command: SlashCommand;
      options: any[];
    };
    expect(arg.command.name).toBe('/verbosity');
    // No legacy master toggle row.
    const values = arg.options.map((o) => o.value);
    expect(values).not.toContain('__verbose_toggle__');
    // First row is now the "all" pseudo-row.
    expect(arg.options[0].value).toBe('filter:all');
    // Filter rows should all be [on] under "all". The "all" pseudo-row
    // shows [active] instead of [on], skip it.
    const categoryRows = arg.options.filter((o) =>
      String(o.value).startsWith('category:')
    );
    expect(categoryRows.length).toBeGreaterThan(0);
    for (const opt of categoryRows) {
      expect(opt.description).toBe('[on]');
    }
  });

  it('empty filters: every tool is gated off', () => {
    setVerboseConfig({ filters: [] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:output');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const categoryRows = arg.options.filter((o) =>
      String(o.value).startsWith('category:')
    );
    expect(categoryRows.length).toBeGreaterThan(0);
    for (const opt of categoryRows) {
      expect(opt.description).toBe('[off]');
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

describe('/verbose density presets', () => {
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
});

describe('/verbose display flag toggles (set:)', () => {
  beforeEach(() => {
    setVerboseConfig({
      filters: ['all'],
      display: {
        showToolReasoning: true,
        toolArgsMode: 'block',
        showElapsed: true,
        subagent: {
          pipeline: true,
          prompts: true,
          roles: true,
          deps: true,
          responses: true,
        },
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: null,
        outputMaxLines: null,
        argsMaxChars: 80,
        outputMaxChars: null,
      },
    });
    resetVerboseCache();
  });

  it('set:showToolReasoning flips the boolean', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'set:showToolReasoning');
    expect(getVerboseConfig().display!.showToolReasoning).toBe(false);
  });

  it('set:toolArgsMode:inline switches arg mode', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'set:toolArgsMode:inline');
    expect(getVerboseConfig().display!.toolArgsMode).toBe('inline');
    runEffect(verbosityCmd, null, ctx, 'set:toolArgsMode:off');
    expect(getVerboseConfig().display!.toolArgsMode).toBe('off');
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
      display: {
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
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: null,
        outputMaxLines: null,
        argsMaxChars: 80,
        outputMaxChars: null,
      },
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

describe('/verbose preset confirmation gate (replaces standalone reset)', () => {
  beforeEach(() => {
    resetVerboseCache();
    // Custom shape so bare /verbosity opens the config menu — keeps the
    // legacy assertions about "config menu has no Reset row" relevant.
    setVerboseConfig({
      filters: [],
      display: {
        showToolReasoning: false,
        toolArgsMode: 'off',
        showElapsed: false,
        subagent: {
          pipeline: true,
          prompts: true,
          roles: true,
          deps: true,
          responses: true,
        },
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: null,
        outputMaxLines: null,
        argsMaxChars: 80,
        outputMaxChars: null,
      },
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
      display: {
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
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: null,
        outputMaxLines: null,
        argsMaxChars: 80,
        outputMaxChars: null,
      },
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

describe('/verbose drilldown menus', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('menu:tool opens an Args mode triplet plus reasoning + elapsed toggles', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:tool');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const values = arg.options.map((o) => o.value);
    expect(values).toContain('set:showToolReasoning');
    expect(values).toContain('set:showElapsed');
    expect(values).toContain('set:toolArgsMode:off');
    expect(values).toContain('set:toolArgsMode:inline');
    expect(values).toContain('set:toolArgsMode:block');
    // back-link present so the user can return to the top menu without esc.
    // The back-link encodes the source key so the cursor lands on "Tool calls".
    expect(values).toContain('menu:top:tool');
  });

  it('menu:subagent lists steps + nested rows + responses + full output toggle', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:subagent');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const values = arg.options.map((o) => o.value);
    // Master step toggle, the two nested rows it gates (instructions +
    // role labels), and responses still emit through set:subagent:<key>.
    // The deps toggle was dropped — dependency arrows are now always-on
    // when steps render (the user found the chrome confusing).
    for (const k of ['pipeline', 'prompts', 'roles', 'responses']) {
      expect(values).toContain(`set:subagent:${k}`);
    }
    // The new full-output toggle mirrors the `subagent` filter category.
    expect(values).toContain('set:subagent:fullOutput');
    // Old "deps" row is gone.
    expect(values).not.toContain('set:subagent:deps');
  });

  it('menu:subagent hides nested rows when pipeline is off', () => {
    // Dropping pipeline (the master "Show subagent steps" toggle) makes
    // prompts and roles dead toggles — the renderer wraps both in
    // `if (sub.pipeline && ...)`. Reflect that in the menu so the user
    // can't toggle a row that wouldn't change the output.
    setVerboseConfig({
      display: {
        showToolReasoning: true,
        toolArgsMode: 'block',
        showElapsed: true,
        subagent: {
          pipeline: false,
          prompts: true,
          roles: true,
          deps: true,
          responses: true,
        },
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: null,
        outputMaxLines: null,
        argsMaxChars: 80,
        outputMaxChars: null,
      },
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

  it('menu:density shows the four presets, marks the active one, and lists Custom', () => {
    setVerboseConfig({ display: undefined, filters: [] });
    resetVerboseCache();
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:density');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const values = arg.options.map((o) => o.value);
    // Selecting a preset row routes to the confirmation gate, not direct
    // commit — the value is `menu:density:confirm:<preset>`, not
    // `density:<preset>`. The `menu:` prefix is required so the route
    // handler picks it up; without it (regression), every preset fell
    // through to the "Unknown subcommand" error.
    expect(values).toContain('menu:density:confirm:minimal');
    expect(values).toContain('menu:density:confirm:lean');
    expect(values).toContain('menu:density:confirm:default');
    // `full` is the only preset whose filter shape is `['all']`.
    expect(values).toContain('menu:density:confirm:full');
    // Legacy `normal` rename: the default-out-of-the-box preset is now
    // called `default`. Old `normal` identifier is gone.
    expect(values).not.toContain('menu:density:confirm:normal');
    expect(values).not.toContain('menu:density:confirm:verbose');
    // Custom row routes to the config (per-knob) menu.
    const custom = arg.options.find((o) => o.label === 'custom') as any;
    expect(custom?.value).toBe('menu:config');
    // 'default' equals DEFAULT_DISPLAY + filters: ['shell'] — the fresh-
    // install canonical shape. Override the describe block's beforeEach
    // (which set filters: ['all'], matching `full`) with the default-active
    // shape and re-open the menu so we can verify the [active] marker
    // lands on the `default` row.
    setVerboseConfig({ filters: ['shell'] });
    const ctx2 = liteCtx();
    runEffect(verbosityCmd, null, ctx2, 'menu:density');
    const arg2 = (ctx2._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][])[0]![0] as { options: any[] };
    const def = arg2.options.find(
      (o) => o.value === 'menu:density:confirm:default'
    ) as any;
    expect(def.description).toContain('[active]');
  });

  it('menu:density:confirm:<preset> opens the preset confirmation submenu', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:density:confirm:lean');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    // Cancel comes first so the safe row is the default.
    expect(arg.options[0].label).toBe('Cancel');
    expect(arg.options[0].value).toBe('menu:density');
    // Yes row commits via density:apply:<preset>.
    const yes = arg.options.find((o) => o.value === 'density:apply:lean');
    expect(yes).toBeDefined();
    expect(yes!.label).toContain('Yes');
    // Cancel does not mutate.
    expect(getVerboseConfig().filters).toEqual(['all']);
  });
});

describe('/verbose ESC navigation flag', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('opening the top menu clears verboseReturnOnEscape', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, '');
    const calls = ctx._spies.setVerboseReturnOnEscape!.mock
      .calls as unknown as unknown[][];
    // Most recent setReturn call should be null — top menu fully exits on ESC.
    const last = calls[calls.length - 1];
    expect(last?.[0]).toBe(null);
  });

  it('opening any config sub-menu sets verboseReturnOnEscape to "menu:top:<key>"', () => {
    // Each config submenu encodes its source key in the return route so the
    // top (config) menu re-opens with the cursor on the row the user
    // descended from. The density menu is its own top-level entry now and
    // uses null setReturn instead.
    const expected: Record<string, string> = {
      'menu:tool': 'menu:top:tool',
      'menu:subagent': 'menu:top:subagent',
      'menu:output': 'menu:top:output',
      'menu:truncation': 'menu:top:truncation',
    };
    for (const [sub, route] of Object.entries(expected)) {
      const ctx = liteCtx();
      runEffect(verbosityCmd, null, ctx, sub);
      const calls = ctx._spies.setVerboseReturnOnEscape!.mock
        .calls as unknown as unknown[][];
      const last = calls[calls.length - 1];
      expect(last?.[0]).toBe(route);
    }
  });

  it('opening menu:density clears the ESC return route (top-level entry)', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:density');
    const calls = ctx._spies.setVerboseReturnOnEscape!.mock
      .calls as unknown as unknown[][];
    const last = calls[calls.length - 1];
    // Density is a top-level entry, not a config-menu submenu — ESC fully
    // exits rather than navigating back one level.
    expect(last?.[0]).toBe(null);
  });

  it('preset confirmation arms ESC to return to the density menu', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:density:confirm:lean');
    const calls = ctx._spies.setVerboseReturnOnEscape!.mock
      .calls as unknown as unknown[][];
    const last = calls[calls.length - 1];
    expect(last?.[0]).toBe('menu:density');
  });

  it('toggling a setting from a config sub-menu re-arms the flag for the next ESC', () => {
    const ctx = liteCtx();
    // Toggle a per-knob setting — handler re-opens the relevant submenu.
    runEffect(verbosityCmd, null, ctx, 'set:showToolReasoning');
    const calls = ctx._spies.setVerboseReturnOnEscape!.mock
      .calls as unknown as unknown[][];
    // After re-open, the flag must again point at the tool submenu's
    // top-menu return route. Without re-arming, ESC after one toggle would
    // fully exit instead of going back one level.
    const last = calls[calls.length - 1];
    expect(last?.[0]).toBe('menu:top:tool');
  });
});

describe('/verbose cursor positioning (initialIndex)', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('opens the entry menu with cursor at row 0', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, '');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { initialIndex?: number };
    expect(arg.initialIndex).toBe(0);
  });

  it('opens each submenu with cursor at row 0 (so the chevron is visible)', () => {
    for (const sub of [
      'menu:density',
      'menu:density:confirm:default',
      'menu:density:confirm:lean',
      'menu:tool',
      'menu:subagent',
      'menu:output',
      'menu:truncation',
      'menu:truncation:argsLines:edit',
      'menu:truncation:outputLines:edit',
    ]) {
      const ctx = liteCtx();
      runEffect(verbosityCmd, null, ctx, sub);
      const calls = ctx._spies.setActiveCommand!.mock
        .calls as unknown as unknown[][];
      const arg = calls[calls.length - 1]![0] as { initialIndex?: number };
      expect(arg.initialIndex).toBe(0);
    }
  });

  it('config menu re-entered via menu:top:<key> lands on the matching row', () => {
    // Config menu order: density(0), tool(1), subagent(2), thinking(3),
    // tasks(4), output(5), truncation(6). The standalone reset-confirm row
    // is gone — density now serves the reset-to-defaults purpose via the
    // `default` preset. `thinking` is the persisted-thinking-content toggle
    // (distinct from per-tool reasoning under tool-calls). `tasks` is the
    // lite task tray toggle.
    const expected: Record<string, number> = {
      'menu:top:density': 0,
      'menu:top:tool': 1,
      'menu:top:subagent': 2,
      'menu:top:thinking': 3,
      'menu:top:tasks': 4,
      'menu:top:output': 5,
      'menu:top:truncation': 6,
    };
    for (const [route, idx] of Object.entries(expected)) {
      const ctx = liteCtx();
      runEffect(verbosityCmd, null, ctx, route);
      const calls = ctx._spies.setActiveCommand!.mock
        .calls as unknown as unknown[][];
      const arg = calls[calls.length - 1]![0] as { initialIndex?: number };
      expect(arg.initialIndex).toBe(idx);
    }
  });

  it('config menu via bare menu:top still opens with cursor at row 0', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:top');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { initialIndex?: number };
    expect(arg.initialIndex).toBe(0);
  });
});

describe('/verbose Truncation submenu', () => {
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
      display: {
        showToolReasoning: true,
        toolArgsMode: 'block',
        showElapsed: true,
        subagent: {
          pipeline: true,
          prompts: true,
          roles: true,
          deps: true,
          responses: true,
        },
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: null,
        outputMaxLines: 20,
        argsMaxChars: 80,
        outputMaxChars: null,
      },
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
      display: {
        showToolReasoning: true,
        toolArgsMode: 'block',
        showElapsed: true,
        subagent: {
          pipeline: true,
          prompts: true,
          roles: true,
          deps: true,
          responses: true,
        },
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: 7,
        outputMaxLines: null,
        argsMaxChars: 80,
        outputMaxChars: null,
      },
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

  it('menu:truncation:argsLines:edit opens the numeric editor (sets previewKey)', () => {
    // The preset menu was replaced with a numeric editor (rendered by
    // CommandMenu when previewKey ends with `:edit`). The effect handler
    // marks the route by setting previewKey on the active command — the
    // option list is a single placeholder, all keypresses are handled by
    // the editor component.
    setVerboseConfig({
      display: {
        showToolReasoning: true,
        toolArgsMode: 'block',
        showElapsed: true,
        subagent: {
          pipeline: true,
          prompts: true,
          roles: true,
          deps: true,
          responses: true,
        },
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: 10,
        outputMaxLines: null,
        argsMaxChars: 80,
        outputMaxChars: null,
      },
    });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:truncation:argsLines:edit');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as {
      options: any[];
      previewKey?: string;
    };
    expect(arg.previewKey).toBe('truncation:argsLines:edit');
  });

  it('menu:truncation:outputLines:edit opens the output editor', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:truncation:outputLines:edit');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { previewKey?: string };
    expect(arg.previewKey).toBe('truncation:outputLines:edit');
  });

  it('set:argsMaxLines:20 saves the value (committed from editor)', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'set:argsMaxLines:20');
    expect(getVerboseConfig().display!.argsMaxLines).toBe(20);
    expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
  });

  it('set:argsMaxChars:null clears the chip cap (unlimited inline args)', () => {
    setVerboseConfig({
      display: {
        showToolReasoning: true,
        toolArgsMode: 'inline',
        showElapsed: true,
        subagent: {
          pipeline: true,
          prompts: true,
          roles: true,
          deps: true,
          responses: true,
        },
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: null,
        outputMaxLines: null,
        argsMaxChars: 80,
        outputMaxChars: null,
      },
    });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'set:argsMaxChars:null');
    expect(getVerboseConfig().display!.argsMaxChars).toBeNull();
  });

  it('set:outputMaxLines:null clears the output cap (unlimited)', () => {
    setVerboseConfig({
      display: {
        showToolReasoning: true,
        toolArgsMode: 'block',
        showElapsed: true,
        subagent: {
          pipeline: true,
          prompts: true,
          roles: true,
          deps: true,
          responses: true,
        },
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: null,
        outputMaxLines: 50,
        argsMaxChars: 80,
        outputMaxChars: null,
      },
    });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'set:outputMaxLines:null');
    expect(getVerboseConfig().display!.outputMaxLines).toBeNull();
  });

  it('opening menu:truncation arms ESC to return to the top menu', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:truncation');
    const calls = ctx._spies.setVerboseReturnOnEscape!.mock
      .calls as unknown as unknown[][];
    const last = calls[calls.length - 1];
    expect(last?.[0]).toBe('menu:top:truncation');
  });

  it('opening menu:truncation:argsLines:edit arms ESC to return to the truncation menu', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'menu:truncation:argsLines:edit');
    const calls = ctx._spies.setVerboseReturnOnEscape!.mock
      .calls as unknown as unknown[][];
    const last = calls[calls.length - 1];
    expect(last?.[0]).toBe('menu:truncation');
  });
});

describe('/verbose top menu Output filters row summary', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('shows "none" when filters is empty', () => {
    setVerboseConfig({ filters: [] });
    const ctx = liteCtx();
    // Use the explicit `config` route — bare /verbosity would open the
    // density menu since [] + DEFAULT_DISPLAY matches the `default` preset,
    // and the density menu doesn't surface the per-section filter summary.
    runEffect(verbosityCmd, null, ctx, 'config');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const outputRow = arg.options.find((o) => o.label === 'Show output');
    expect(outputRow?.description).toBe('none');
  });

  it('shows "all" when filters is ["all"]', () => {
    setVerboseConfig({ filters: ['all'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'config');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const outputRow = arg.options.find((o) => o.label === 'Show output');
    expect(outputRow?.description).toBe('all');
  });

  it('shows the comma-joined filter list otherwise', () => {
    setVerboseConfig({ filters: ['shell', 'mcp', 'read'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'config');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const outputRow = arg.options.find((o) => o.label === 'Show output');
    expect(outputRow?.description).toBe('shell, mcp, read');
  });
});

describe('/verbose long filter announcement (count-based truncation)', () => {
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

describe('/verbose top menu Subagent row summary', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('summary lists steps with nested labels, summary, and full output when every knob is on', () => {
    setVerboseConfig({
      filters: ['all'],
      display: {
        showToolReasoning: true,
        toolArgsMode: 'block',
        showElapsed: true,
        subagent: {
          pipeline: true,
          prompts: true,
          roles: true,
          deps: true,
          responses: true,
        },
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: null,
        outputMaxLines: null,
        argsMaxChars: 80,
        outputMaxChars: null,
      },
    });
    const ctx = liteCtx();
    // `config` route — these tests probe the per-section row summaries in
    // the config menu, which bare /verbosity may bypass when the saved
    // shape matches a preset.
    runEffect(verbosityCmd, null, ctx, 'config');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const subRow = arg.options.find((o) => o.label === 'Subagent');
    // Summary now reads top-down: master step list with its nested
    // labels, then summary, then full-output. Old "pipeline · prompts ·
    // roles · deps · responses" is replaced with user-facing wording.
    expect(subRow?.description).toBe(
      'steps + instructions + roles · summary · full output'
    );
    expect(subRow?.description).not.toContain('only');
  });

  it('summary collapses nested labels when only steps is on', () => {
    setVerboseConfig({
      filters: [],
      display: {
        showToolReasoning: true,
        toolArgsMode: 'block',
        showElapsed: true,
        subagent: {
          pipeline: true,
          prompts: false,
          roles: false,
          deps: false,
          responses: false,
        },
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: null,
        outputMaxLines: null,
        argsMaxChars: 80,
        outputMaxChars: null,
      },
    });
    const ctx = liteCtx();
    // `config` route — these tests probe the per-section row summaries in
    // the config menu, which bare /verbosity may bypass when the saved
    // shape matches a preset.
    runEffect(verbosityCmd, null, ctx, 'config');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const subRow = arg.options.find((o) => o.label === 'Subagent');
    // Just the bare "steps" label — no nested suffix, no summary, no
    // full output.
    expect(subRow?.description).toBe('steps');
  });

  it('shows "(all hidden)" when no sections are on AND no full-output filter', () => {
    setVerboseConfig({
      filters: [],
      display: {
        showToolReasoning: true,
        toolArgsMode: 'block',
        showElapsed: true,
        subagent: {
          pipeline: false,
          prompts: false,
          roles: false,
          deps: false,
          responses: false,
        },
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: null,
        outputMaxLines: null,
        argsMaxChars: 80,
        outputMaxChars: null,
      },
    });
    const ctx = liteCtx();
    // `config` route — these tests probe the per-section row summaries in
    // the config menu, which bare /verbosity may bypass when the saved
    // shape matches a preset.
    runEffect(verbosityCmd, null, ctx, 'config');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    const arg = calls[calls.length - 1]![0] as { options: any[] };
    const subRow = arg.options.find((o) => o.label === 'Subagent');
    expect(subRow?.description).toBe('(all hidden)');
  });
});

describe('/verbose unknown-token soft warnings', () => {
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

describe('/verbose unknown subcommand', () => {
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

  it('uses canonical /verbosity in error messages even when invoked via /verbose alias', () => {
    const ctx = liteCtx();
    // Invoke via the alias command — error should still cite the canonical
    // /verbosity name, not the alias the user happened to type.
    runEffect(verboseAliasCmd, null, ctx, 'wat-is-this');
    const calls = ctx._spies.showAlert!.mock.calls as unknown as unknown[][];
    expect(calls[0]![0]).toContain('Unknown /verbosity subcommand');
  });
});

describe('/verbose case-insensitive command verbs', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('/verbose ON works the same as /verbose on', () => {
    setVerboseConfig({ filters: [] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'ON');
    expect(getVerboseConfig().filters).toEqual(['all']);
  });

  it('/verbose OFF works the same as /verbose off', () => {
    setVerboseConfig({ filters: ['all'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'OFF');
    expect(getVerboseConfig().filters).toEqual([]);
  });

  it('/verbose ALL works the same as /verbose all', () => {
    setVerboseConfig({ filters: ['shell'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'ALL');
    expect(getVerboseConfig().filters).toEqual(['all']);
  });

  it('/verbose Status works the same as /verbose status', () => {
    setVerboseConfig({ filters: ['shell'] });
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'Status');
    const calls = ctx._spies.announceSystem!.mock
      .calls as unknown as unknown[][];
    expect(calls.length).toBeGreaterThan(0);
    expect(getVerboseConfig().filters).toEqual(['shell']);
  });

  it('/verbose DENSITY lean applies the preset (verb is case-insensitive)', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'DENSITY lean');
    expect(getVerboseConfig().display!.toolArgsMode).toBe('inline');
  });

  it('/verbose ONLY shell preserves filter token case', () => {
    const ctx = liteCtx();
    // The verb is uppercase but the filter token must reach the saved
    // config exactly as typed — MCP tool names are case-sensitive.
    runEffect(verbosityCmd, null, ctx, 'ONLY mcp__SomeCase__tool');
    expect(getVerboseConfig().filters).toContain('mcp__SomeCase__tool');
  });

  it('/verbose only Shell keeps the original Shell capitalization in saved filters', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'only Shell');
    // Filter token should NOT be lowercased — preserves what the user typed.
    expect(getVerboseConfig().filters).toContain('Shell');
    expect(getVerboseConfig().filters).not.toContain('shell');
  });
});

describe('/verbose density colon form (Bug D)', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('/verbose density:lean is accepted as a CLI form', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'density:lean');
    expect(getVerboseConfig().display!.toolArgsMode).toBe('inline');
  });

  it('/verbose density:minimal is accepted', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'density:minimal');
    expect(getVerboseConfig().display!.toolArgsMode).toBe('off');
    expect(getVerboseConfig().display!.showToolReasoning).toBe(false);
  });

  it('/verbose density:custom (unknown preset) surfaces an error alert', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'density:custom');
    const calls = ctx._spies.showAlert!.mock.calls as unknown as unknown[][];
    expect(calls[0]![0]).toContain('Unknown density preset');
    expect(calls[0]![1]).toBe('error');
  });

  it('/verbose DENSITY:lean is accepted (case-insensitive verb)', () => {
    const ctx = liteCtx();
    runEffect(verbosityCmd, null, ctx, 'DENSITY:lean');
    expect(getVerboseConfig().display!.toolArgsMode).toBe('inline');
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

// Backward-compat: /verbose still routes through the same dispatcher branch
// as /verbosity. Functionality is identical regardless of which alias the
// user typed.
describe('/verbose alias for /verbosity', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  it('/verbose config opens the same config menu as /verbosity config', () => {
    const ctx = liteCtx();
    runEffect(verboseAliasCmd, null, ctx, 'config');
    const calls = ctx._spies.setActiveCommand!.mock
      .calls as unknown as unknown[][];
    expect(calls.length).toBe(1);
    const arg = calls[0]![0] as { command: SlashCommand; options: any[] };
    // Header chip always reflects the canonical /verbosity command, not
    // the alias the user typed. The verbosityConfig handler resolves the
    // canonical SlashCommand from the registry so CommandMenu's
    // `command.name === '/verbosity'` checks (Ctrl+P preview toggle,
    // density-row draft highlight) work identically across direct entry,
    // the legacy /verbose alias, and /settings → verbosity. Mirrors how
    // /theme keeps the chip as /theme regardless of how it was reached.
    expect(arg.command.name).toBe('/verbosity');
    const labels = arg.options.map((o) => o.label);
    expect(labels).toContain('Tool calls');
    expect(labels).toContain('Show output');
  });

  it('/verbose on toggles via the alias', () => {
    setVerboseConfig({ filters: [] });
    const ctx = liteCtx();
    runEffect(verboseAliasCmd, null, ctx, 'on');
    expect(getVerboseConfig().filters).toEqual(['all']);
  });

  it('/verbose status announces the same shape as /verbosity status', () => {
    setVerboseConfig({ filters: ['shell'] });
    const ctx = liteCtx();
    runEffect(verboseAliasCmd, null, ctx, 'status');
    const calls = ctx._spies.announceSystem!.mock
      .calls as unknown as unknown[][];
    expect(calls.length).toBe(1);
    const msg = calls[0]![0] as string;
    expect(msg).toContain('shell');
  });
});

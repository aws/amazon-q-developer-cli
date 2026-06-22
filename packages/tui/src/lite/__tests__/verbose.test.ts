/**
 * Tests for the verbose-mode config module.
 *
 * Strategy: redirect KIRO_HOME to a tmp dir before importing the module
 * under test so config writes don't clobber the developer's real
 * ~/.kiro/settings/lite_verbose.json. Each test resets the cache + clears
 * the temp file between cases so each starts from defaults.
 *
 * Why a real tmp dir instead of `mock.module('fs', ...)`: a process-global
 * fs mock leaks across test files (mock.restore() doesn't undo module
 * mocks), and the in-module mockFile would carry the last-written display
 * config into other test files like verbose-command.test.ts — breaking
 * detectActivePreset's display equality check there.
 */

import {
  describe,
  test,
  expect,
  beforeAll,
  beforeEach,
  afterEach,
  afterAll,
} from 'bun:test';
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'fs';
import { tmpdir } from 'os';
import { dirname, join } from 'path';

let tmpHome: string;
let originalKiroHome: string | undefined;
let configFile: string;

// Set KIRO_HOME *before* importing the module under test so kiroHomePath()
// resolves into our throwaway dir for the entire suite.
beforeAll(() => {
  originalKiroHome = process.env.KIRO_HOME;
  tmpHome = mkdtempSync(join(tmpdir(), 'kiro-verbose-test-'));
  process.env.KIRO_HOME = tmpHome;
  configFile = join(tmpHome, 'settings', 'lite_verbose.json');
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

/**
 * Replace `mockFile = X` from the prior fs-mock strategy:
 * - `null` → file does not exist (rm if present)
 * - string → write that exact content (creates parent dir if missing)
 *
 * Also clears the sibling `cli.json` so tests start with a clean slate
 * for the cli.json mirror behavior. setVerboseConfig writes to BOTH
 * files; without this clear, a later test would see stale mirror
 * values from a prior test's setVerboseConfig() call.
 */
function setConfigFile(content: string | null) {
  if (content == null) {
    rmSync(configFile, { force: true });
  } else {
    mkdirSync(dirname(configFile), { recursive: true });
    writeFileSync(configFile, content);
  }
  rmSync(join(tmpHome, 'settings', 'cli.json'), { force: true });
}

import {
  getVerboseConfig,
  getVerboseDisplay,
  getVerboseFilters,
  setVerboseConfig,
  resetVerboseCache,
  shouldShowToolOutput,
  validateTokens,
  categorize,
  VERBOSE_CATEGORIES,
  DEFAULT_DISPLAY,
  DENSITY_DISPLAY,
  applyDensityPreset,
} from '../verbose.js';
import { Settings } from '../../constants/settings.js';

const cliJsonFile = () => join(tmpHome, 'settings', 'cli.json');

/** Read cli.json from the tmp KIRO_HOME, returning {} if missing. */
function readCliJson(): Record<string, unknown> {
  try {
    return JSON.parse(readFileSync(cliJsonFile(), 'utf-8'));
  } catch {
    return {};
  }
}

/** Write a cli.json blob into the tmp KIRO_HOME. Used by mirror-read tests
 *  to seed a cli.json value that getVerboseDisplay should pick up. */
function writeCliJson(obj: Record<string, unknown>) {
  mkdirSync(dirname(cliJsonFile()), { recursive: true });
  writeFileSync(cliJsonFile(), JSON.stringify(obj));
}

describe('verbose config', () => {
  beforeEach(() => {
    setConfigFile(null);
    resetVerboseCache();
    delete process.env.KIRO_LITE_VERBOSE;
  });

  afterEach(() => {
    delete process.env.KIRO_LITE_VERBOSE;
  });

  test('default config when no file exists', () => {
    const cfg = getVerboseConfig();
    // Fresh-install default streams shell stdout in a tail-windowed strip.
    expect(cfg.filters).toEqual(['shell']);
  });

  test('save then load roundtrip', () => {
    expect(setVerboseConfig({ filters: ['shell', 'mcp'] })).toBe(true);
    resetVerboseCache();
    const cfg = getVerboseConfig();
    expect(cfg.filters).toEqual(['shell', 'mcp']);
  });

  // Each row applies its filter sets in sequence (last one wins) so the
  // empty-list case proves a prior non-empty set is cleared, not merged.
  test.each([
    ['["all"] collapses mixed lists', [['all', 'shell', 'mcp']], ['all']],
    ['empty list is preserved as off', [['shell'], []], []],
    ['dedupes and trims', [['shell', '  shell  ', 'mcp']], ['shell', 'mcp']],
  ] as const)('filter normalization: %s', (_name, sets, expected) => {
    for (const filters of sets) setVerboseConfig({ filters: [...filters] });
    expect(getVerboseConfig().filters).toEqual([...expected]);
  });

  test('malformed file falls back to defaults silently', () => {
    setConfigFile('{ this is not json');
    const cfg = getVerboseConfig();
    expect(cfg.filters).toEqual(['shell']);
  });

  test('migration: legacy enabled:false overrides filters to []', () => {
    setConfigFile(JSON.stringify({ enabled: false, filters: ['all'] }));
    const cfg = getVerboseConfig();
    expect(cfg.filters).toEqual([]);
    // The shape no longer carries enabled.
    expect((cfg as unknown as Record<string, unknown>).enabled).toBeUndefined();
  });

  test('migration: legacy enabled:true keeps saved filter list', () => {
    setConfigFile(JSON.stringify({ enabled: true, filters: ['shell'] }));
    const cfg = getVerboseConfig();
    expect(cfg.filters).toEqual(['shell']);
  });

  test('migration: enabled field is dropped on next save', () => {
    setConfigFile(JSON.stringify({ enabled: true, filters: ['shell'] }));
    // Force a load+migration, then save anything.
    getVerboseConfig();
    setVerboseConfig({ filters: ['mcp'] });
    const written = JSON.parse(readFileSync(configFile, 'utf-8'));
    expect(written.enabled).toBeUndefined();
    expect(written.filters).toEqual(['mcp']);
  });

  test('KIRO_LITE_VERBOSE=1 seeds filters when no file exists', () => {
    process.env.KIRO_LITE_VERBOSE = '1';
    const cfg = getVerboseConfig();
    expect(cfg.filters).toEqual(['all']);
  });

  test('KIRO_LITE_VERBOSE=1 is a no-op when a saved config exists', () => {
    setConfigFile(JSON.stringify({ filters: ['shell'] }));
    process.env.KIRO_LITE_VERBOSE = '1';
    const cfg = getVerboseConfig();
    expect(cfg.filters).toEqual(['shell']);
  });
});

describe('shouldShowToolOutput', () => {
  beforeEach(() => {
    setConfigFile(null);
    resetVerboseCache();
    delete process.env.KIRO_LITE_VERBOSE;
  });

  test('empty filters: no tool ever surfaces output', () => {
    setVerboseConfig({ filters: [] });
    expect(shouldShowToolOutput('execute_bash')).toBe(false);
    expect(shouldShowToolOutput('mcp__nova-memory-mcp__recall')).toBe(false);
    expect(shouldShowToolOutput('fs_read')).toBe(false);
  });

  test('"all" lets every tool through', () => {
    setVerboseConfig({ filters: ['all'] });
    expect(shouldShowToolOutput('execute_bash')).toBe(true);
    expect(shouldShowToolOutput('mcp__nova-memory-mcp__recall')).toBe(true);
    expect(shouldShowToolOutput('some_unknown_tool')).toBe(true);
  });

  test('exact tool name match', () => {
    setVerboseConfig({
      filters: ['mcp__nova-memory-mcp__recall'],
    });
    expect(shouldShowToolOutput('mcp__nova-memory-mcp__recall')).toBe(true);
    expect(shouldShowToolOutput('mcp__nova-memory-mcp__remember')).toBe(false);
    // mcp category alone wouldn't match either since filters doesn't include it
    expect(shouldShowToolOutput('execute_bash')).toBe(false);
  });

  test('category match: shell allows execute_bash but not read', () => {
    setVerboseConfig({ filters: ['shell'] });
    expect(shouldShowToolOutput('execute_bash')).toBe(true);
    expect(shouldShowToolOutput('fs_read')).toBe(false);
  });

  test('mixed category + exact name: only shell + recall', () => {
    setVerboseConfig({
      filters: ['shell', 'mcp__nova-memory-mcp__recall'],
    });
    expect(shouldShowToolOutput('execute_bash')).toBe(true);
    expect(shouldShowToolOutput('mcp__nova-memory-mcp__recall')).toBe(true);
    // Other MCP tools blocked despite recall being allowed.
    expect(shouldShowToolOutput('mcp__builder-mcp__InternalSearch')).toBe(
      false
    );
    expect(shouldShowToolOutput('fs_read')).toBe(false);
  });

  test('mcp category covers any mcp__-prefixed tool', () => {
    setVerboseConfig({ filters: ['mcp'] });
    expect(shouldShowToolOutput('mcp__nova-memory-mcp__recall')).toBe(true);
    expect(shouldShowToolOutput('mcp__builder-mcp__InternalSearch')).toBe(true);
    expect(shouldShowToolOutput('execute_bash')).toBe(false);
  });
});

describe('categorize', () => {
  // fs_write → null: write is deliberately uncategorized (see verbose.ts) so
  // shouldShowToolOutput('fs_write') is false; the write call site instead
  // gates renderVerboseOutput on result.status === 'error'.
  test.each([
    ['mcp__nova-memory-mcp__recall', 'mcp'],
    ['execute_bash', 'shell'],
    ['fs_write', null],
    ['fs_read', 'read'],
    ['subagent', 'subagent'],
    ['totally_made_up_tool', null],
  ] as const)('categorize(%p) → %p', (tool, expected) => {
    expect(categorize(tool)).toBe(expected);
  });
});

describe('validateTokens', () => {
  test('accepts categories and tool-name shapes', () => {
    const { accepted, rejected } = validateTokens([
      'shell',
      'mcp',
      'all',
      'mcp__nova-memory-mcp__recall',
      'fs_write',
    ]);
    expect(accepted).toEqual([
      'shell',
      'mcp',
      'all',
      'mcp__nova-memory-mcp__recall',
      'fs_write',
    ]);
    expect(rejected).toEqual([]);
  });

  test('rejects tokens with whitespace or shell metachars', () => {
    const { accepted, rejected } = validateTokens([
      'has space',
      'pipe|bad',
      'good',
    ]);
    expect(accepted).toEqual(['good']);
    expect(rejected).toEqual(['has space', 'pipe|bad']);
  });

  test('drops empty tokens silently', () => {
    const { accepted, rejected } = validateTokens(['', '   ', 'shell']);
    expect(accepted).toEqual(['shell']);
    expect(rejected).toEqual([]);
  });

  test('flags unknown non-mcp tokens via the `unknown` partition', () => {
    const { accepted, rejected, unknown } = validateTokens([
      'shell',
      'foo',
      'bar',
    ]);
    // All accepted (we don't reject typos — MCP tools load lazily) but
    // foo/bar surface as unknown so the caller can soft-warn.
    expect(accepted).toEqual(['shell', 'foo', 'bar']);
    expect(rejected).toEqual([]);
    expect(unknown).toEqual(['foo', 'bar']);
  });

  test('mcp__-prefixed tokens are not flagged unknown', () => {
    const { accepted, unknown } = validateTokens([
      'mcp__nova-memory-mcp__recall',
      'mcp__some-server__tool',
    ]);
    expect(accepted).toEqual([
      'mcp__nova-memory-mcp__recall',
      'mcp__some-server__tool',
    ]);
    expect(unknown).toEqual([]);
  });

  test('known categories and `all` are not flagged unknown', () => {
    const { accepted, unknown } = validateTokens([
      'all',
      'shell',
      'mcp',
      'read',
      'subagent',
    ]);
    expect(accepted).toEqual(['all', 'shell', 'mcp', 'read', 'subagent']);
    expect(unknown).toEqual([]);
  });

  test('legacy `write` token still accepted, but flagged as unknown', () => {
    // `write` was a category token in earlier versions. After collapsing
    // write visibility into outputMaxLines / outputMaxChars on the diff
    // body, it's no longer a recognized category — but a saved config or
    // hand-typed `/verbose only write` shouldn't error out. validateTokens
    // accepts the token (so it survives a round-trip) and marks it unknown
    // so the effect handler can surface a soft warning.
    const { accepted, unknown } = validateTokens(['write']);
    expect(accepted).toEqual(['write']);
    expect(unknown).toEqual(['write']);
  });
});

describe('truncation cap config (argsMaxLines / outputMaxLines)', () => {
  beforeEach(() => {
    setConfigFile(null);
    resetVerboseCache();
    delete process.env.KIRO_LITE_VERBOSE;
  });

  test('defaults: argsMaxLines unbounded, outputMaxLines capped at 5', () => {
    expect(DEFAULT_DISPLAY.argsMaxLines).toBeNull();
    // Tail-window cap on streamed shell output — 5 visible lines + the
    // "+N more lines above" marker. Paired with filters: ['shell'] so a
    // fresh install actually streams something.
    expect(DEFAULT_DISPLAY.outputMaxLines).toBe(5);
  });

  test.each([
    ['minimal', 5],
    ['lean', 10],
    ['default', 5],
  ] as const)('density "%s" caps output at %i lines', (preset, cap) => {
    expect(DENSITY_DISPLAY[preset].outputMaxLines).toBe(cap);
  });

  test('density "default" leaves argsMaxLines unbounded', () => {
    expect(DENSITY_DISPLAY.default.argsMaxLines).toBeNull();
  });

  // Picking a preset is a clean reset of filters to the preset's shape, not a
  // partial patch — so a prior custom filter set ('shell','mcp') is replaced.
  test.each([
    ['minimal', [], 5],
    ['full', ['all'], DENSITY_DISPLAY.full.outputMaxLines],
    ['default', ['shell'], 5],
  ] as const)(
    'applyDensityPreset(%s) resets filters and output cap',
    (preset, expectedFilters, expectedCap) => {
      setVerboseConfig({ filters: ['shell', 'mcp'] });
      applyDensityPreset(preset);
      expect(getVerboseConfig().filters).toEqual([...expectedFilters]);
      expect(getVerboseConfig().display!.outputMaxLines).toBe(expectedCap);
    }
  );

  test('mergeDisplay accepts positive integers and persists them', () => {
    setVerboseConfig({
      display: {
        ...DEFAULT_DISPLAY,
        argsMaxLines: 7,
        outputMaxLines: 25,
      },
    });
    resetVerboseCache();
    const cfg = getVerboseConfig();
    expect(cfg.display!.argsMaxLines).toBe(7);
    expect(cfg.display!.outputMaxLines).toBe(25);
  });

  test('mergeDisplay coerces 0 / negative / non-numeric to null', () => {
    // Simulate a stale config file with bad cap values.
    setConfigFile(
      JSON.stringify({
        filters: [],
        display: {
          ...DEFAULT_DISPLAY,
          argsMaxLines: 0,
          outputMaxLines: -3,
        },
      })
    );
    const cfg = getVerboseConfig();
    expect(cfg.display!.argsMaxLines).toBeNull();
    expect(cfg.display!.outputMaxLines).toBeNull();
  });

  test('mergeDisplay floors fractional caps to integers', () => {
    setConfigFile(
      JSON.stringify({
        filters: [],
        display: { ...DEFAULT_DISPLAY, outputMaxLines: 12.7 },
      })
    );
    expect(getVerboseConfig().display!.outputMaxLines).toBe(12);
  });

  test('configs lacking the new fields default to null', () => {
    // Older saved configs (pre-truncation) only carried the legacy display
    // keys. mergeDisplay must default the new caps to null so we don't
    // surface "0 lines" or undefined behavior on first run after upgrade.
    setConfigFile(
      JSON.stringify({
        filters: ['shell'],
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
        },
      })
    );
    const cfg = getVerboseConfig();
    expect(cfg.display!.argsMaxLines).toBeNull();
    expect(cfg.display!.outputMaxLines).toBeNull();
  });
});

describe('VERBOSE_CATEGORIES', () => {
  test('covers the documented buckets', () => {
    // If we add a new bucket later the test will catch it; keeping the
    // list explicit also doubles as documentation.
    //
    // `write` is intentionally omitted — see verbose.ts for the
    // rationale. Write tools always render in scrollback; the diff body
    // is capped by outputMaxLines / outputMaxChars, so a per-category
    // filter for writes would only gate a redundant `Successfully ...`
    // chrome line.
    expect(new Set(VERBOSE_CATEGORIES)).toEqual(
      new Set([
        'shell',
        'read',
        'web',
        'grep',
        'glob',
        'code',
        'introspect',
        'task',
        'subagent',
        'mcp',
      ])
    );
  });
});

describe('cli.json mirror — write path', () => {
  beforeEach(() => {
    setConfigFile(null);
    resetVerboseCache();
  });

  test.each([
    ['showThinkingContent', false, Settings.CHAT_SHOW_THINKING, false],
    ['showTasks', false, Settings.CHAT_SHOW_TASKS, false],
    ['showToolReasoning', false, Settings.CHAT_TOOLS_SHOW_REASONING, false],
    ['showElapsed', false, Settings.CHAT_TOOLS_SHOW_ELAPSED, false],
    ['toolArgsMode', 'inline', Settings.CHAT_TOOLS_ARGS_MODE, 'inline'],
    ['toolArgsMode', 'off', Settings.CHAT_TOOLS_ARGS_MODE, 'off'],
  ] as const)(
    'display.%s=%p mirrors to %s',
    (prop, value, settingKey, expected) => {
      setVerboseConfig({ display: { [prop]: value } });
      expect(readCliJson()[settingKey]).toBe(expected);
    }
  );

  test('display caps mirror as numbers (positive integers)', () => {
    setVerboseConfig({
      display: {
        argsMaxLines: 7,
        outputMaxLines: 12,
        argsMaxChars: 80,
        outputMaxChars: 200,
      },
    });
    const cli = readCliJson();
    expect(cli[Settings.CHAT_TOOLS_ARGS_MAX_LINES]).toBe(7);
    expect(cli[Settings.CHAT_TOOLS_OUTPUT_MAX_LINES]).toBe(12);
    expect(cli[Settings.CHAT_TOOLS_ARGS_MAX_CHARS]).toBe(80);
    expect(cli[Settings.CHAT_TOOLS_OUTPUT_MAX_CHARS]).toBe(200);
  });

  test('display caps mirror null as JSON null (unbounded)', () => {
    setVerboseConfig({ display: { outputMaxLines: null } });
    expect(readCliJson()[Settings.CHAT_TOOLS_OUTPUT_MAX_LINES]).toBeNull();
  });

  test('subagent.* fields mirror to chat.subagent.*', () => {
    setVerboseConfig({
      display: {
        subagent: {
          pipeline: false,
          prompts: false,
          roles: false,
          deps: false,
          responses: false,
        },
      },
    });
    const cli = readCliJson();
    expect(cli[Settings.CHAT_SUBAGENT_SHOW_PIPELINE]).toBe(false);
    expect(cli[Settings.CHAT_SUBAGENT_SHOW_PROMPTS]).toBe(false);
    expect(cli[Settings.CHAT_SUBAGENT_SHOW_ROLES]).toBe(false);
    expect(cli[Settings.CHAT_SUBAGENT_SHOW_DEPS]).toBe(false);
    expect(cli[Settings.CHAT_SUBAGENT_SHOW_RESPONSES]).toBe(false);
  });

  test('partial subagent patch only mirrors changed fields', () => {
    // Pre-seed cli.json with all-true so we can detect any unintended writes.
    writeCliJson({
      [Settings.CHAT_SUBAGENT_SHOW_PIPELINE]: true,
      [Settings.CHAT_SUBAGENT_SHOW_PROMPTS]: true,
      [Settings.CHAT_SUBAGENT_SHOW_ROLES]: true,
      [Settings.CHAT_SUBAGENT_SHOW_DEPS]: true,
      [Settings.CHAT_SUBAGENT_SHOW_RESPONSES]: true,
    });
    resetVerboseCache();
    setVerboseConfig({ display: { subagent: { pipeline: false } } });
    const cli = readCliJson();
    expect(cli[Settings.CHAT_SUBAGENT_SHOW_PIPELINE]).toBe(false);
    // Other subagent keys must remain untouched at their seeded values.
    expect(cli[Settings.CHAT_SUBAGENT_SHOW_PROMPTS]).toBe(true);
    expect(cli[Settings.CHAT_SUBAGENT_SHOW_ROLES]).toBe(true);
    expect(cli[Settings.CHAT_SUBAGENT_SHOW_DEPS]).toBe(true);
    expect(cli[Settings.CHAT_SUBAGENT_SHOW_RESPONSES]).toBe(true);
  });

  test('filters mirror to chat.tools.filters', () => {
    setVerboseConfig({ filters: ['shell', 'mcp'] });
    expect(readCliJson()[Settings.CHAT_TOOLS_FILTERS]).toEqual([
      'shell',
      'mcp',
    ]);
    setVerboseConfig({ filters: [] });
    expect(readCliJson()[Settings.CHAT_TOOLS_FILTERS]).toEqual([]);
  });

  test('density preset writes mirror every display field at once', () => {
    applyDensityPreset('minimal');
    const cli = readCliJson();
    expect(cli[Settings.CHAT_TOOLS_SHOW_REASONING]).toBe(false);
    expect(cli[Settings.CHAT_TOOLS_ARGS_MODE]).toBe('off');
    expect(cli[Settings.CHAT_TOOLS_SHOW_ELAPSED]).toBe(false);
    expect(cli[Settings.CHAT_SHOW_THINKING]).toBe(false);
    expect(cli[Settings.CHAT_SHOW_TASKS]).toBe(false);
    expect(cli[Settings.CHAT_TOOLS_OUTPUT_MAX_LINES]).toBe(5);
    expect(cli[Settings.CHAT_TOOLS_ARGS_MAX_CHARS]).toBe(60);
    expect(cli[Settings.CHAT_TOOLS_FILTERS]).toEqual([]);
  });

  test('mirror failures do not abort the lite_verbose.json save', () => {
    // Seed cli.json with junk that won't parse as an object.
    // readCliSettings recovers to {} on parse error, then we write
    // a fresh object with the new key — well-formed.
    writeFileSync(cliJsonFile(), 'this is not json');
    const ok = setVerboseConfig({ display: { showTasks: false } });
    expect(ok).toBe(true);
    const lite = JSON.parse(readFileSync(configFile, 'utf-8'));
    expect(lite.display.showTasks).toBe(false);
  });
});

describe('cli.json mirror — read path (getVerboseDisplay)', () => {
  beforeEach(() => {
    setConfigFile(null);
    resetVerboseCache();
  });

  test('cli.json values override DEFAULT_DISPLAY when no lite_verbose.json exists', () => {
    writeCliJson({
      [Settings.CHAT_SHOW_THINKING]: false,
      [Settings.CHAT_SHOW_TASKS]: false,
      [Settings.CHAT_TOOLS_SHOW_REASONING]: false,
      [Settings.CHAT_TOOLS_SHOW_ELAPSED]: false,
      [Settings.CHAT_TOOLS_ARGS_MODE]: 'inline',
      [Settings.CHAT_TOOLS_ARGS_MAX_LINES]: 12,
      [Settings.CHAT_TOOLS_OUTPUT_MAX_LINES]: 25,
      [Settings.CHAT_TOOLS_ARGS_MAX_CHARS]: 60,
      [Settings.CHAT_TOOLS_OUTPUT_MAX_CHARS]: 100,
      [Settings.CHAT_SUBAGENT_SHOW_PIPELINE]: false,
      [Settings.CHAT_SUBAGENT_SHOW_PROMPTS]: false,
      [Settings.CHAT_SUBAGENT_SHOW_ROLES]: false,
      [Settings.CHAT_SUBAGENT_SHOW_DEPS]: false,
      [Settings.CHAT_SUBAGENT_SHOW_RESPONSES]: false,
    });
    const display = getVerboseDisplay();
    expect(display.showThinkingContent).toBe(false);
    expect(display.showTasks).toBe(false);
    expect(display.showToolReasoning).toBe(false);
    expect(display.showElapsed).toBe(false);
    expect(display.toolArgsMode).toBe('inline');
    expect(display.argsMaxLines).toBe(12);
    expect(display.outputMaxLines).toBe(25);
    expect(display.argsMaxChars).toBe(60);
    expect(display.outputMaxChars).toBe(100);
    expect(display.subagent).toEqual({
      pipeline: false,
      prompts: false,
      roles: false,
      deps: false,
      responses: false,
    });
  });

  test('cli.json overrides lite_verbose.json on a per-field basis', () => {
    setConfigFile(
      JSON.stringify({
        filters: [],
        display: { ...DEFAULT_DISPLAY, showTasks: true },
      })
    );
    writeCliJson({ [Settings.CHAT_SHOW_TASKS]: false });
    expect(getVerboseDisplay().showTasks).toBe(false);
  });

  test('missing cli.json key falls back to lite_verbose.json value', () => {
    setConfigFile(
      JSON.stringify({
        filters: [],
        display: { ...DEFAULT_DISPLAY, showTasks: false },
      })
    );
    writeCliJson({}); // explicitly empty, no chat.showTasks key
    expect(getVerboseDisplay().showTasks).toBe(false);
  });

  test('missing cli.json key + missing lite_verbose key falls back to DEFAULT_DISPLAY', () => {
    writeCliJson({});
    expect(getVerboseDisplay().showTasks).toBe(DEFAULT_DISPLAY.showTasks);
    expect(getVerboseDisplay().showThinkingContent).toBe(
      DEFAULT_DISPLAY.showThinkingContent
    );
  });

  test('malformed cli.json values are ignored (fall through to lite_verbose / DEFAULT)', () => {
    setConfigFile(
      JSON.stringify({
        filters: [],
        display: { ...DEFAULT_DISPLAY, showTasks: false },
      })
    );
    writeCliJson({ [Settings.CHAT_SHOW_TASKS]: 'yeah' });
    expect(getVerboseDisplay().showTasks).toBe(false);
  });

  test('cap fields: 0 / negative cli values fall through to fallback', () => {
    setConfigFile(
      JSON.stringify({
        filters: [],
        display: { ...DEFAULT_DISPLAY, outputMaxLines: 7 },
      })
    );
    writeCliJson({ [Settings.CHAT_TOOLS_OUTPUT_MAX_LINES]: 0 });
    expect(getVerboseDisplay().outputMaxLines).toBe(7);
    writeCliJson({ [Settings.CHAT_TOOLS_OUTPUT_MAX_LINES]: -3 });
    expect(getVerboseDisplay().outputMaxLines).toBe(7);
  });

  test('cap fields: explicit JSON null is preserved as unbounded', () => {
    setConfigFile(
      JSON.stringify({
        filters: [],
        display: { ...DEFAULT_DISPLAY, outputMaxLines: 7 },
      })
    );
    writeCliJson({ [Settings.CHAT_TOOLS_OUTPUT_MAX_LINES]: null });
    expect(getVerboseDisplay().outputMaxLines).toBeNull();
  });

  test('toolArgsMode: invalid cli values fall through', () => {
    setConfigFile(
      JSON.stringify({
        filters: [],
        display: { ...DEFAULT_DISPLAY, toolArgsMode: 'block' },
      })
    );
    writeCliJson({ [Settings.CHAT_TOOLS_ARGS_MODE]: 'banana' });
    expect(getVerboseDisplay().toolArgsMode).toBe('block');
  });

  test('object identity is preserved when cli.json matches the cache', () => {
    // No cli.json overrides → getVerboseDisplay should return the same
    // object reference on each call (the cache). Critical for upstream
    // useMemo([display]) deps in the modern TUI to be stable.
    setConfigFile(
      JSON.stringify({ filters: [], display: { ...DEFAULT_DISPLAY } })
    );
    writeCliJson({});
    const a = getVerboseDisplay();
    const b = getVerboseDisplay();
    expect(a).toBe(b);
  });
});

describe('getVerboseFilters — cli.json mirror', () => {
  beforeEach(() => {
    setConfigFile(null);
    resetVerboseCache();
  });

  test('cli.json filters override lite_verbose.json filters', () => {
    setConfigFile(JSON.stringify({ filters: ['shell'] }));
    writeCliJson({ [Settings.CHAT_TOOLS_FILTERS]: ['mcp', 'web'] });
    expect(getVerboseFilters()).toEqual(['mcp', 'web']);
  });

  test('missing cli.json filters falls back to lite_verbose.json', () => {
    setConfigFile(JSON.stringify({ filters: ['shell'] }));
    writeCliJson({});
    expect(getVerboseFilters()).toEqual(['shell']);
  });

  test('non-array cli.json values fall through to lite_verbose.json', () => {
    setConfigFile(JSON.stringify({ filters: ['shell'] }));
    writeCliJson({ [Settings.CHAT_TOOLS_FILTERS]: 'shell' });
    expect(getVerboseFilters()).toEqual(['shell']);
  });

  test('["all"] in cli.json collapses any mixed list', () => {
    writeCliJson({
      [Settings.CHAT_TOOLS_FILTERS]: ['all', 'shell', 'mcp'],
    });
    expect(getVerboseFilters()).toEqual(['all']);
  });

  test('non-string array entries are dropped', () => {
    writeCliJson({
      [Settings.CHAT_TOOLS_FILTERS]: ['shell', 42, null, '', 'mcp'],
    });
    expect(getVerboseFilters()).toEqual(['shell', 'mcp']);
  });
});

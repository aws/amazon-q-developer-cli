import {
  afterAll,
  beforeAll,
  beforeEach,
  describe,
  expect,
  test,
} from 'bun:test';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'fs';
import { dirname, join } from 'path';
import { tmpdir } from 'os';
import { Settings } from '../../constants/settings.js';
import {
  DENSITY_DISPLAY,
  DENSITY_PRESETS,
  DEFAULT_DISPLAY,
  TUI_DEFAULT_DISPLAY,
  VERBOSE_CATEGORIES,
  applyDensityPreset,
  cacheByVerboseVersion,
  categorize,
  getDensityPresetDisplay,
  getDensityPresetFilters,
  getTuiVerboseDisplay,
  getTuiVerboseFilters,
  getVerboseConfig,
  getVerboseDisplay,
  getVerboseFilters,
  getVerboseVersion,
  resetVerboseCache,
  setVerboseConfig,
  shouldShowToolOutput,
  subscribeVerbose,
  validateTokens,
  type VerboseConfig,
  type VerbositySurface,
} from '../verbose.js';

let tmpHome: string;
let originalKiroHome: string | undefined;
let originalRollout: string | undefined;

const cliJsonFile = () => join(tmpHome, 'settings', 'cli.json');
const legacyFile = () => join(tmpHome, 'settings', 'lite_verbose.json');

beforeAll(() => {
  originalKiroHome = process.env.KIRO_HOME;
  originalRollout = process.env.KIRO_LITE_ROLLOUT_ENABLED;
  tmpHome = mkdtempSync(join(tmpdir(), 'kiro-verbose-test-'));
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

beforeEach(() => {
  rmSync(join(tmpHome, 'settings'), { recursive: true, force: true });
  delete process.env.KIRO_LITE_VERBOSE;
  process.env.KIRO_LITE_ROLLOUT_ENABLED = '1';
  resetVerboseCache();
});

function readCliJson(): Record<string, unknown> {
  if (!existsSync(cliJsonFile())) return {};
  return JSON.parse(readFileSync(cliJsonFile(), 'utf-8'));
}

function writeCliJson(settings: Record<string, unknown>): void {
  mkdirSync(dirname(cliJsonFile()), { recursive: true });
  writeFileSync(cliJsonFile(), JSON.stringify(settings));
}

function writeLegacy(config: unknown): void {
  mkdirSync(dirname(legacyFile()), { recursive: true });
  writeFileSync(legacyFile(), JSON.stringify(config));
}

function setConfigFile(content: string | null): void {
  if (content == null) rmSync(legacyFile(), { force: true });
  else {
    mkdirSync(dirname(legacyFile()), { recursive: true });
    writeFileSync(legacyFile(), content);
  }
  rmSync(cliJsonFile(), { force: true });
}

function surfaceKey(surface: VerbositySurface): string {
  return surface === 'lite'
    ? Settings.CHAT_VERBOSITY_LITE
    : Settings.CHAT_VERBOSITY_TUI;
}

function savedSurface(surface: VerbositySurface): VerboseConfig {
  return readCliJson()[surfaceKey(surface)] as VerboseConfig;
}

describe('surface-specific persistence', () => {
  test('zero-config defaults remain different', () => {
    expect(getVerboseFilters()).toEqual(['shell']);
    expect(getVerboseDisplay()).toMatchObject({
      persistOutput: true,
      showToolReasoning: true,
      subagent: { pipeline: true, responses: true },
    });
    expect(getTuiVerboseFilters()).toEqual(['all', '-subagent']);
    expect(getTuiVerboseDisplay()).toMatchObject({
      persistOutput: false,
      showToolReasoning: false,
      subagent: { pipeline: false, responses: false },
    });
    expect(readCliJson()).toEqual({});
  });

  test('custom edits persist independently across restarts', () => {
    setVerboseConfig(
      {
        filters: ['mcp'],
        display: { showElapsed: false, outputMaxLines: 8 },
      },
      'lite'
    );
    setVerboseConfig(
      {
        filters: ['read'],
        display: { showElapsed: true, outputMaxLines: 3 },
      },
      'tui'
    );

    expect(savedSurface('lite')).toMatchObject({
      filters: ['mcp'],
      display: { showElapsed: false, outputMaxLines: 8 },
    });
    expect(savedSurface('tui')).toMatchObject({
      filters: ['read'],
      display: { showElapsed: true, outputMaxLines: 3 },
    });
    expect(readCliJson()).not.toHaveProperty(Settings.CHAT_TOOLS_FILTERS);
    expect(existsSync(legacyFile())).toBe(false);

    resetVerboseCache();
    expect(getVerboseFilters()).toEqual(['mcp']);
    expect(getVerboseDisplay().outputMaxLines).toBe(8);
    expect(getTuiVerboseFilters()).toEqual(['read']);
    expect(getTuiVerboseDisplay().outputMaxLines).toBe(3);
  });

  test.each(DENSITY_PRESETS.map((preset) => [preset] as const))(
    '%s preset changes only the selected surface',
    (preset) => {
      const untouchedTui = getVerboseConfig('tui');
      applyDensityPreset(preset, 'lite');
      expect(getVerboseDisplay()).toEqual(
        getDensityPresetDisplay(preset, 'lite')
      );
      expect(getVerboseFilters()).toEqual([
        ...getDensityPresetFilters(preset, 'lite'),
      ]);
      expect(getVerboseConfig('tui')).toEqual(untouchedTui);

      applyDensityPreset(preset, 'tui');
      expect(getTuiVerboseDisplay()).toEqual(
        getDensityPresetDisplay(preset, 'tui')
      );
      expect(getTuiVerboseFilters()).toEqual([
        ...getDensityPresetFilters(preset, 'tui'),
      ]);
    }
  );

  test('a partial patch preserves the rest of its surface record', () => {
    applyDensityPreset('full', 'tui');
    const before = getVerboseConfig('tui');
    const beforeDisplay = before.display;
    setVerboseConfig({ display: { showElapsed: false } }, 'tui');
    expect(getVerboseConfig('tui')).toEqual({
      ...before,
      display: { ...beforeDisplay, showElapsed: false },
    });
    expect(getVerboseDisplay()).toEqual(DEFAULT_DISPLAY);
  });

  test('a failed cli.json write keeps the in-memory edit active', () => {
    getVerboseDisplay();
    const settingsPath = join(tmpHome, 'settings');
    writeFileSync(settingsPath, 'not a directory');
    expect(setVerboseConfig({ display: { showTasks: false } }, 'lite')).toBe(
      false
    );
    expect(getVerboseDisplay().showTasks).toBe(false);
    rmSync(settingsPath, { force: true });
  });

  test('malformed cli.json survives migration and mutation byte-for-byte', () => {
    const malformed = '{ keep this broken';
    mkdirSync(dirname(cliJsonFile()), { recursive: true });
    writeFileSync(cliJsonFile(), malformed);
    writeLegacy({ filters: ['mcp'] });

    expect(getVerboseFilters()).toEqual(['mcp']);
    expect(readFileSync(cliJsonFile(), 'utf-8')).toBe(malformed);
    expect(setVerboseConfig({ filters: ['read'] }, 'lite')).toBe(false);
    expect(readFileSync(cliJsonFile(), 'utf-8')).toBe(malformed);
  });
});

describe('legacy migration', () => {
  test('moves lite_verbose.json into the Lite cli.json record', () => {
    writeLegacy({
      filters: ['mcp'],
      display: { ...DEFAULT_DISPLAY, showElapsed: false },
    });
    expect(getVerboseFilters()).toEqual(['mcp']);
    expect(getVerboseDisplay().showElapsed).toBe(false);
    expect(savedSurface('lite')).toMatchObject({
      filters: ['mcp'],
      display: { showElapsed: false },
    });
    expect(readFileSync(legacyFile(), 'utf-8')).toContain('"mcp"');
    expect(getTuiVerboseDisplay()).toEqual(TUI_DEFAULT_DISPLAY);
  });

  test('migrates legacy enabled:false as filters off', () => {
    writeLegacy({ enabled: false, filters: ['all'] });
    expect(getVerboseFilters()).toEqual([]);
    expect(savedSurface('lite').filters).toEqual([]);
  });

  test('old shared cli.json values seed each surface once', () => {
    writeCliJson({
      [Settings.CHAT_TOOLS_FILTERS]: ['mcp'],
      [Settings.CHAT_TOOLS_ARGS_MODE]: 'inline',
      [Settings.CHAT_TOOLS_OUTPUT_MAX_LINES]: 7,
      [Settings.CHAT_SUBAGENT_SHOW_PIPELINE]: true,
    });

    expect(getVerboseFilters()).toEqual(['mcp']);
    expect(getVerboseDisplay()).toMatchObject({
      toolArgsMode: 'inline',
      outputMaxLines: 7,
      subagent: { pipeline: true },
    });
    expect(getTuiVerboseFilters()).toEqual(['mcp']);
    expect(getTuiVerboseDisplay()).toMatchObject({
      toolArgsMode: 'inline',
      outputMaxLines: 7,
      subagent: { pipeline: true },
    });
    expect(savedSurface('lite')).toBeDefined();
    expect(savedSurface('tui')).toBeDefined();
  });

  test('a surface record wins over stale shared keys', () => {
    writeCliJson({
      [Settings.CHAT_VERBOSITY_LITE]: {
        filters: ['read'],
        display: { ...DEFAULT_DISPLAY, showElapsed: false },
      },
      [Settings.CHAT_TOOLS_FILTERS]: ['all'],
      [Settings.CHAT_TOOLS_SHOW_ELAPSED]: true,
    });
    expect(getVerboseFilters()).toEqual(['read']);
    expect(getVerboseDisplay().showElapsed).toBe(false);
  });

  test('KIRO_LITE_VERBOSE only affects an unsaved Lite config', () => {
    process.env.KIRO_LITE_VERBOSE = '1';
    expect(getVerboseFilters()).toEqual(['all']);
    expect(getTuiVerboseFilters()).toEqual(['all', '-subagent']);
    expect(readCliJson()).toEqual({});
  });
});

describe('config normalization', () => {
  test('normalizes filters and display values from cli.json', () => {
    writeCliJson({
      [Settings.CHAT_VERBOSITY_LITE]: {
        filters: ['all', 'shell', 42],
        display: {
          ...DEFAULT_DISPLAY,
          thinkingDisplay: 'off',
          argsMaxLines: -3,
          outputMaxLines: 12.7,
          subagent: { ...DEFAULT_DISPLAY.subagent, prompts: false },
        },
      },
    });
    expect(getVerboseFilters()).toEqual(['all']);
    expect(getVerboseDisplay()).toMatchObject({
      thinkingDisplay: 'off',
      showThinkingContent: false,
      argsMaxLines: null,
      outputMaxLines: 12,
      subagent: { prompts: false },
    });
  });

  test('malformed saved records fall back to surface defaults', () => {
    writeCliJson({
      [Settings.CHAT_VERBOSITY_LITE]: 'bad',
      [Settings.CHAT_VERBOSITY_TUI]: [],
    });
    expect(getVerboseDisplay()).toEqual(DEFAULT_DISPLAY);
    expect(getTuiVerboseDisplay()).toEqual(TUI_DEFAULT_DISPLAY);
  });
});

describe('shouldShowToolOutput', () => {
  beforeEach(() => {
    setConfigFile(null);
    resetVerboseCache();
    delete process.env.KIRO_LITE_VERBOSE;
  });

  // Each row sets filters once, then asserts which tools surface output (true)
  // vs stay hidden (false). Covers empty/all/exact-name/category/mixed gates.
  test.each<{
    name: string;
    filters: string[];
    wants: Record<string, boolean>;
  }>([
    {
      name: 'empty filters: no tool ever surfaces output',
      filters: [],
      wants: {
        execute_bash: false,
        'mcp__nova-memory-mcp__recall': false,
        fs_read: false,
      },
    },
    {
      name: '"all" lets every tool through',
      filters: ['all'],
      wants: {
        execute_bash: true,
        'mcp__nova-memory-mcp__recall': true,
        some_unknown_tool: true,
      },
    },
    {
      name: 'exact tool name match (mcp category absent)',
      filters: ['mcp__nova-memory-mcp__recall'],
      wants: {
        'mcp__nova-memory-mcp__recall': true,
        'mcp__nova-memory-mcp__remember': false,
        execute_bash: false,
      },
    },
    {
      name: 'category match: shell allows execute_bash but not read',
      filters: ['shell'],
      wants: { execute_bash: true, fs_read: false },
    },
    {
      name: 'mixed category + exact name: only shell + recall',
      filters: ['shell', 'mcp__nova-memory-mcp__recall'],
      wants: {
        execute_bash: true,
        'mcp__nova-memory-mcp__recall': true,
        'mcp__builder-mcp__InternalSearch': false,
        fs_read: false,
      },
    },
    {
      name: 'mcp category covers any mcp__-prefixed tool',
      filters: ['mcp'],
      wants: {
        'mcp__nova-memory-mcp__recall': true,
        'mcp__builder-mcp__InternalSearch': true,
        execute_bash: false,
      },
    },
  ])('$name', ({ filters, wants }) => {
    setVerboseConfig({ filters });
    for (const [tool, want] of Object.entries(wants)) {
      expect(shouldShowToolOutput(tool)).toBe(want);
    }
  });
});

describe('categorize', () => {
  // fs_write → null: write is deliberately uncategorized (see verbose.ts) so
  // shouldShowToolOutput('fs_write') is false; the write call site instead
  // gates renderVerboseOutput on result.status === 'error'.
  test.each([
    ['mcp__nova-memory-mcp__recall', 'mcp'],
    ['execute_bash', 'shell'],
    // KAS shell-process tools umbrella under 'shell' (wire titles + ids).
    ['List Processes', 'shell'],
    ['get_process_output', 'shell'],
    ['fs_write', null],
    ['fs_read', 'read'],
    ['subagent', 'subagent'],
    ['totally_made_up_tool', null],
  ] as const)('categorize(%p) → %p', (tool, expected) => {
    expect(categorize(tool)).toBe(expected);
  });
});

describe('validateTokens', () => {
  // We don't reject typos (MCP tools load lazily): unknown non-category,
  // non-mcp__ tokens are accepted but partitioned into `unknown` so callers can
  // soft-warn. Only whitespace/shell-metachar tokens are rejected; empty tokens
  // drop silently. `write` is a legacy category → accepted + unknown so a saved
  // config survives a round-trip without erroring.
  test.each<{
    name: string;
    tokens: string[];
    accepted: string[];
    rejected?: string[];
    unknown?: string[];
  }>([
    {
      name: 'accepts categories and tool-name shapes',
      tokens: [
        'shell',
        'mcp',
        'all',
        'mcp__nova-memory-mcp__recall',
        'fs_write',
      ],
      accepted: [
        'shell',
        'mcp',
        'all',
        'mcp__nova-memory-mcp__recall',
        'fs_write',
      ],
      rejected: [],
    },
    {
      name: 'rejects tokens with whitespace or shell metachars',
      tokens: ['has space', 'pipe|bad', 'good'],
      accepted: ['good'],
      rejected: ['has space', 'pipe|bad'],
    },
    {
      name: 'drops empty tokens silently',
      tokens: ['', '   ', 'shell'],
      accepted: ['shell'],
      rejected: [],
    },
    {
      name: 'flags unknown non-mcp tokens via the `unknown` partition',
      tokens: ['shell', 'foo', 'bar'],
      accepted: ['shell', 'foo', 'bar'],
      rejected: [],
      unknown: ['foo', 'bar'],
    },
    {
      name: 'mcp__-prefixed tokens are not flagged unknown',
      tokens: ['mcp__nova-memory-mcp__recall', 'mcp__some-server__tool'],
      accepted: ['mcp__nova-memory-mcp__recall', 'mcp__some-server__tool'],
      unknown: [],
    },
    {
      name: 'known categories and `all` are not flagged unknown',
      tokens: ['all', 'shell', 'mcp', 'read', 'subagent'],
      accepted: ['all', 'shell', 'mcp', 'read', 'subagent'],
      unknown: [],
    },
    {
      name: 'legacy `write` token accepted but flagged unknown',
      tokens: ['write'],
      accepted: ['write'],
      unknown: ['write'],
    },
  ])('$name', ({ tokens, accepted, rejected, unknown }) => {
    const result = validateTokens(tokens);
    expect(result.accepted).toEqual(accepted);
    if (rejected !== undefined) expect(result.rejected).toEqual(rejected);
    if (unknown !== undefined) expect(result.unknown).toEqual(unknown);
  });
});

describe('truncation cap config (argsMaxLines / outputMaxLines)', () => {
  beforeEach(() => {
    setConfigFile(null);
    resetVerboseCache();
    delete process.env.KIRO_LITE_VERBOSE;
  });

  test('defaults: args unlimited and output capped to 5 lines', () => {
    expect(DEFAULT_DISPLAY.argsMaxLines).toBeNull();
    expect(DEFAULT_DISPLAY.argsMaxChars).toBeNull();
    // Tail-window cap on streamed shell output — 5 visible lines + the
    // "+N more lines above" marker. Paired with filters: ['shell'] so a
    // fresh install actually streams something.
    expect(DEFAULT_DISPLAY.outputMaxLines).toBe(5);
    expect(DEFAULT_DISPLAY.outputMaxChars).toBeNull();
    expect(TUI_DEFAULT_DISPLAY).toMatchObject({
      argsMaxLines: null,
      argsMaxChars: null,
      outputMaxLines: 5,
      outputMaxChars: null,
    });
  });

  test.each([
    ['lean', 10, undefined],
    ['default', 5, null],
  ] as const)(
    'density "%s" caps output at %i lines',
    (preset, cap, argsMaxLines) => {
      expect(DENSITY_DISPLAY[preset].outputMaxLines).toBe(cap);
      if (argsMaxLines !== undefined) {
        expect(DENSITY_DISPLAY[preset].argsMaxLines).toBe(argsMaxLines);
      }
    }
  );

  test.each([
    ['lean', [], 10],
    ['full', ['all'], DENSITY_DISPLAY.full.outputMaxLines],
    ['default', ['shell'], 5],
  ] as const)(
    'applyDensityPreset(%s) resets filters and output cap',
    (preset, expectedFilters, expectedCap) => {
      setVerboseConfig({ filters: ['shell', 'mcp'] });
      applyDensityPreset(preset);
      expect(getVerboseConfig().filters).toEqual([...expectedFilters]);
      expect(getVerboseConfig().display.outputMaxLines).toBe(expectedCap);
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
    expect(cfg.display.argsMaxLines).toBe(7);
    expect(cfg.display.outputMaxLines).toBe(25);
  });

  test('mergeDisplay coerces 0 / negative / non-numeric to null', () => {
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
    expect(cfg.display.argsMaxLines).toBeNull();
    expect(cfg.display.outputMaxLines).toBeNull();
  });

  test('mergeDisplay floors fractional caps to integers', () => {
    setConfigFile(
      JSON.stringify({
        filters: [],
        display: { ...DEFAULT_DISPLAY, outputMaxLines: 12.7 },
      })
    );
    expect(getVerboseConfig().display.outputMaxLines).toBe(12);
  });

  test('configs lacking the new fields default to null', () => {
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
    expect(cfg.display.argsMaxLines).toBeNull();
    expect(cfg.display.outputMaxLines).toBeNull();
  });
});

describe('VERBOSE_CATEGORIES', () => {
  test('covers the documented buckets', () => {
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

describe('rollout and reactivity', () => {
  test('off-cohort TUI ignores the saved verbosity record', () => {
    applyDensityPreset('full', 'tui');
    delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
    resetVerboseCache();
    expect(getTuiVerboseDisplay()).toEqual(TUI_DEFAULT_DISPLAY);
    expect(getTuiVerboseFilters()).toEqual(['all']);
  });

  test('writes and resets notify subscribers', () => {
    let calls = 0;
    const unsubscribe = subscribeVerbose(() => calls++);
    const before = getVerboseVersion();
    setVerboseConfig({ display: { showElapsed: false } }, 'lite');
    expect(getVerboseVersion()).toBeGreaterThan(before);
    resetVerboseCache();
    expect(calls).toBe(2);
    unsubscribe();
  });

  test('version-cached snapshots read once per revision', () => {
    let reads = 0;
    const snapshot = cacheByVerboseVersion(() => ++reads);
    expect([snapshot(), snapshot()]).toEqual([1, 1]);
    resetVerboseCache();
    expect([snapshot(), snapshot()]).toEqual([2, 2]);
  });
});

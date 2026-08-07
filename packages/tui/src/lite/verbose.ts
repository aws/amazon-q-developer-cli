/**
 * Surface-specific verbosity configuration stored in the global cli.json.
 * Lite and TUI own independent records; legacy shared settings and
 * lite_verbose.json are migration inputs only.
 */

import { readFileSync } from 'fs';
import { kiroHomePath } from '../utils/kiro-home.js';
import { logger } from '../utils/logger.js';
import {
  readCliSettings,
  readCliSettingsStrict,
  writeCliSettings,
} from '../utils/cli-settings.js';
import { Settings } from '../constants/settings.js';
import type { UiMode } from '../types/ui-mode.js';
import {
  KNOWN_TOOL_NAMES,
  TOOL_OUTPUT_CATEGORIES,
  toolVerbosityPolicy,
  type ToolCallOrigin,
  type ToolKind,
} from '../types/tool-capabilities.js';

export type ToolArgsMode = 'off' | 'inline' | 'block';

export type ThinkingDisplayMode = 'off' | 'collapsed' | 'expanded';

export interface SubagentDisplayConfig {
  pipeline: boolean;
  prompts: boolean;
  roles: boolean;
  deps: boolean;
  responses: boolean;
}

export interface VerboseDisplayConfig {
  showToolReasoning: boolean;
  toolArgsMode: ToolArgsMode;
  showElapsed: boolean;
  subagent: SubagentDisplayConfig;
  thinkingDisplay: ThinkingDisplayMode;
  showThinkingContent: boolean;
  showWriteDiffs: boolean;
  showTasks: boolean;
  persistOutput: boolean;
  argsMaxLines: number | null;
  outputMaxLines: number | null;
  argsMaxChars: number | null;
  outputMaxChars: number | null;
}

export interface VerboseConfig {
  filters: string[];
  display: VerboseDisplayConfig;
}

export const DEFAULT_DISPLAY: VerboseDisplayConfig = {
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
  thinkingDisplay: 'expanded',
  showThinkingContent: true,
  showWriteDiffs: true,
  showTasks: true,
  persistOutput: true,
  argsMaxLines: null,
  outputMaxLines: 5,
  argsMaxChars: null,
  outputMaxChars: null,
};

export const TUI_DEFAULT_DISPLAY: VerboseDisplayConfig = {
  ...DEFAULT_DISPLAY,
  showToolReasoning: false,
  showElapsed: false,
  subagent: {
    pipeline: false,
    prompts: false,
    roles: false,
    deps: false,
    responses: false,
  },
  persistOutput: false,
};

/** Permission decisions always expose the payload being approved. */
export function approvalDisplayConfig(
  current: VerboseDisplayConfig
): VerboseDisplayConfig {
  if (
    current.toolArgsMode === 'block' &&
    current.showWriteDiffs &&
    current.argsMaxLines === null &&
    current.argsMaxChars === null &&
    current.outputMaxLines === null &&
    current.outputMaxChars === null &&
    current.subagent.pipeline &&
    current.subagent.prompts &&
    current.subagent.roles &&
    current.subagent.deps
  ) {
    return current;
  }
  return {
    ...current,
    toolArgsMode: 'block',
    showWriteDiffs: true,
    argsMaxLines: null,
    argsMaxChars: null,
    outputMaxLines: null,
    outputMaxChars: null,
    subagent: {
      ...current.subagent,
      pipeline: true,
      prompts: true,
      roles: true,
      deps: true,
    },
  };
}

export const DENSITY_PRESETS = ['lean', 'default', 'full'] as const;
export type DensityPreset = (typeof DENSITY_PRESETS)[number];

const display = (
  o: Partial<Omit<VerboseDisplayConfig, 'subagent'>> & {
    subagent?: Partial<SubagentDisplayConfig>;
  }
): VerboseDisplayConfig => ({
  ...DEFAULT_DISPLAY,
  ...o,
  subagent: { ...DEFAULT_DISPLAY.subagent, ...o.subagent },
});

export const DENSITY_DISPLAY: Record<DensityPreset, VerboseDisplayConfig> = {
  lean: display({
    showToolReasoning: false,
    toolArgsMode: 'inline',
    subagent: { prompts: false, roles: false },
    thinkingDisplay: 'off',
    showThinkingContent: false,
    showWriteDiffs: false,
    outputMaxLines: 10,
    argsMaxChars: 80,
  }),
  default: { ...DEFAULT_DISPLAY },
  full: display({
    outputMaxLines: null,
  }),
};

export const DENSITY_FILTERS: Record<DensityPreset, readonly string[]> = {
  lean: [],
  default: ['shell'], // stream shell stdout out of the box
  full: ['all'],
};

/**
 * Built-in filter categories come from the tool capability registry. `mcp`
 * matches either explicit MCP provenance or the legacy `mcp__` prefix.
 *
 * NO `write` CATEGORY (referenced below): a write's tool result is just a
 * one-line "Successfully ..." that duplicates the diff body, so a write filter
 * would gate only redundant chrome — and errors surface regardless. Write
 * tools always render; the diff is bounded by outputMaxLines/outputMaxChars.
 */
export const VERBOSE_CATEGORIES = [...TOOL_OUTPUT_CATEGORIES, 'mcp'] as const;
export type VerboseCategory = (typeof VERBOSE_CATEGORIES)[number];

const TUI_DEFAULT_FILTERS = ['all', '-subagent'];

interface VerbosityVariantPolicy {
  setting: string;
  displayPresets: Record<DensityPreset, VerboseDisplayConfig>;
  filterPresets: Record<DensityPreset, readonly string[]>;
}

// Complete defaults and persistence policy for each supported UI variant.
const VERBOSITY_VARIANTS = {
  lite: {
    setting: Settings.CHAT_VERBOSITY_LITE,
    displayPresets: DENSITY_DISPLAY,
    filterPresets: DENSITY_FILTERS,
  },
  tui: {
    setting: Settings.CHAT_VERBOSITY_TUI,
    displayPresets: { ...DENSITY_DISPLAY, default: TUI_DEFAULT_DISPLAY },
    filterPresets: { ...DENSITY_FILTERS, default: TUI_DEFAULT_FILTERS },
  },
} satisfies Record<UiMode, VerbosityVariantPolicy>;

export type VerbositySurface = keyof typeof VERBOSITY_VARIANTS;

export function getDensityPresetDisplay(
  preset: DensityPreset,
  surface: VerbositySurface = 'lite'
): VerboseDisplayConfig {
  return VERBOSITY_VARIANTS[surface].displayPresets[preset];
}

export function getDensityPresetFilters(
  preset: DensityPreset,
  surface: VerbositySurface = 'lite'
): readonly string[] {
  return VERBOSITY_VARIANTS[surface].filterPresets[preset];
}

const excludedFilter = (token: string): string | null =>
  token.startsWith('-') && token.length > 1 ? token.slice(1) : null;

function normalizeFilters(filters: readonly string[]): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const raw of filters) {
    const token = raw.trim();
    if (!token || seen.has(token)) continue;
    seen.add(token);
    out.push(token);
  }
  if (!out.includes('all')) return out;
  return ['all', ...out.filter((token) => excludedFilter(token) !== null)];
}

export const expandFilterBaseline = (filters: readonly string[]): string[] => {
  if (!filters.includes('all')) return [...filters];
  const excluded = new Set(
    filters
      .map(excludedFilter)
      .filter((token): token is string => token !== null)
  );
  return VERBOSE_CATEGORIES.filter((category) => !excluded.has(category));
};

// These exhaustive tables define the display shape and map legacy shared
// cli.json keys into the new surface records. New writes persist the complete
// record selected by VERBOSITY_VARIANTS.
type KeysMatching<T, Value> = {
  [Key in keyof T]-?: T[Key] extends Value ? Key : never;
}[keyof T];

type BoolDisplayKey = Exclude<
  KeysMatching<VerboseDisplayConfig, boolean>,
  'showThinkingContent'
>;
const BOOL_FIELDS = {
  showToolReasoning: Settings.CHAT_TOOLS_SHOW_REASONING,
  showElapsed: Settings.CHAT_TOOLS_SHOW_ELAPSED,
  showWriteDiffs: Settings.CHAT_TOOLS_SHOW_WRITE_DIFFS,
  showTasks: Settings.CHAT_SHOW_TASKS,
  persistOutput: Settings.CHAT_TOOLS_PERSIST_OUTPUT,
} as const satisfies Record<BoolDisplayKey, string>;
const BOOL_FIELD_KEYS = Object.keys(BOOL_FIELDS) as BoolDisplayKey[];

type CapDisplayKey = KeysMatching<VerboseDisplayConfig, number | null>;
// `defaultOnMissing`: char caps fall back to DEFAULT_DISPLAY when the saved
// field is absent (so a pre-field upgrade keeps the chip cap); line caps treat
// missing as unbounded.
const CAP_FIELDS = {
  argsMaxLines: {
    setting: Settings.CHAT_TOOLS_ARGS_MAX_LINES,
    defaultOnMissing: false,
  },
  outputMaxLines: {
    setting: Settings.CHAT_TOOLS_OUTPUT_MAX_LINES,
    defaultOnMissing: false,
  },
  argsMaxChars: {
    setting: Settings.CHAT_TOOLS_ARGS_MAX_CHARS,
    defaultOnMissing: true,
  },
  outputMaxChars: {
    setting: Settings.CHAT_TOOLS_OUTPUT_MAX_CHARS,
    defaultOnMissing: true,
  },
} as const satisfies Record<
  CapDisplayKey,
  { setting: string; defaultOnMissing: boolean }
>;
const CAP_FIELD_KEYS = Object.keys(CAP_FIELDS) as CapDisplayKey[];

type SubagentFieldSettings = {
  [Key in keyof SubagentDisplayConfig]: SubagentDisplayConfig[Key] extends boolean
    ? string
    : never;
};
const SUBAGENT_FIELDS = {
  pipeline: Settings.CHAT_SUBAGENT_SHOW_PIPELINE,
  prompts: Settings.CHAT_SUBAGENT_SHOW_PROMPTS,
  roles: Settings.CHAT_SUBAGENT_SHOW_ROLES,
  deps: Settings.CHAT_SUBAGENT_SHOW_DEPS,
  responses: Settings.CHAT_SUBAGENT_SHOW_RESPONSES,
} as const satisfies SubagentFieldSettings;
const SUBAGENT_FIELD_KEYS = Object.keys(
  SUBAGENT_FIELDS
) as (keyof SubagentDisplayConfig)[];

type BespokeDisplayKey = Exclude<
  keyof VerboseDisplayConfig,
  BoolDisplayKey | CapDisplayKey
>;
interface BespokeDisplayMetadata {
  toolArgsMode: { kind: 'argsMode'; setting: string };
  thinkingDisplay: { kind: 'thinking'; setting: string };
  showThinkingContent: { kind: 'derivedThinking'; source: 'thinkingDisplay' };
  subagent: { kind: 'subagent'; fields: SubagentFieldSettings };
}
const BESPOKE_FIELDS = {
  toolArgsMode: {
    kind: 'argsMode',
    setting: Settings.CHAT_TOOLS_ARGS_MODE,
  },
  thinkingDisplay: {
    kind: 'thinking',
    setting: Settings.CHAT_SHOW_THINKING,
  },
  showThinkingContent: {
    kind: 'derivedThinking',
    source: 'thinkingDisplay',
  },
  subagent: {
    kind: 'subagent',
    fields: SUBAGENT_FIELDS,
  },
} as const satisfies {
  [Key in BespokeDisplayKey]: Key extends keyof BespokeDisplayMetadata
    ? BespokeDisplayMetadata[Key]
    : never;
};

function legacyConfigPath(): string {
  return kiroHomePath('settings', 'lite_verbose.json');
}

const cached: Record<VerbositySurface, VerboseConfig | null> = {
  lite: null,
  tui: null,
};

let version = 0;
const subscribers = new Set<() => void>();

export function subscribeVerbose(cb: () => void): () => void {
  subscribers.add(cb);
  return () => subscribers.delete(cb);
}

export function getVerboseVersion(): number {
  return version;
}

export function cacheByVerboseVersion<T>(read: () => T): () => T {
  let cachedValue: { version: number; value: T } | undefined;
  return () => {
    const currentVersion = getVerboseVersion();
    if (cachedValue?.version === currentVersion) return cachedValue.value;
    cachedValue = { version: currentVersion, value: read() };
    return cachedValue.value;
  };
}

function notifyVerboseChanged(): void {
  version++;
  for (const cb of subscribers) cb();
}

function parseLineCap(raw: unknown): number | null {
  if (typeof raw !== 'number' || !Number.isFinite(raw)) return null;
  const n = Math.floor(raw);
  return n > 0 ? n : null;
}

function cloneDisplay(display: VerboseDisplayConfig): VerboseDisplayConfig {
  return {
    ...display,
    subagent: { ...display.subagent },
  };
}

function defaultConfig(surface: VerbositySurface): VerboseConfig {
  return {
    filters: [...getDensityPresetFilters('default', surface)],
    display: cloneDisplay(getDensityPresetDisplay('default', surface)),
  };
}

function mergeDisplay(
  raw: unknown,
  fallback: VerboseDisplayConfig,
  legacyCaps = false
): VerboseDisplayConfig {
  const out: VerboseDisplayConfig = {
    ...fallback,
    subagent: { ...fallback.subagent },
  };
  if (!raw || typeof raw !== 'object') return out;
  const obj = raw as Record<string, unknown>;
  for (const local of BOOL_FIELD_KEYS) {
    if (typeof obj[local] === 'boolean') out[local] = obj[local] as boolean;
  }
  if (
    obj.toolArgsMode === 'off' ||
    obj.toolArgsMode === 'inline' ||
    obj.toolArgsMode === 'block'
  ) {
    out.toolArgsMode = obj.toolArgsMode;
  }
  if (
    obj.thinkingDisplay === 'off' ||
    obj.thinkingDisplay === 'collapsed' ||
    obj.thinkingDisplay === 'expanded'
  ) {
    out.thinkingDisplay = obj.thinkingDisplay;
  } else if (typeof obj.showThinkingContent === 'boolean') {
    out.thinkingDisplay = obj.showThinkingContent ? 'collapsed' : 'off';
  }
  out.showThinkingContent = out.thinkingDisplay !== 'off';
  for (const local of CAP_FIELD_KEYS) {
    const { defaultOnMissing } = CAP_FIELDS[local];
    if (obj[local] === undefined) {
      if (legacyCaps && !defaultOnMissing) out[local] = null;
      continue;
    }
    out[local] = parseLineCap(obj[local]);
  }
  if (obj.subagent && typeof obj.subagent === 'object') {
    const sa = obj.subagent as Record<string, unknown>;
    for (const local of SUBAGENT_FIELD_KEYS) {
      if (typeof sa[local] === 'boolean')
        out.subagent[local] = sa[local] as boolean;
    }
  }
  return out;
}

function parseConfig(
  raw: unknown,
  surface: VerbositySurface,
  legacy = false
): VerboseConfig | null {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return null;
  const obj = raw as Record<string, unknown>;
  const base = defaultConfig(surface);
  let filters = base.filters;
  if (Array.isArray(obj.filters)) {
    filters = normalizeFilters(
      obj.filters.filter((token): token is string => typeof token === 'string')
    );
  } else if (legacy) {
    filters = [];
  }
  if (legacy && obj.enabled === false) filters = [];
  return {
    filters,
    display:
      obj.display === undefined
        ? base.display
        : mergeDisplay(obj.display, base.display, legacy),
  };
}

function readLegacyLiteConfig(): VerboseConfig | null {
  try {
    return parseConfig(
      JSON.parse(readFileSync(legacyConfigPath(), 'utf-8')),
      'lite',
      true
    );
  } catch {
    return null;
  }
}

const LEGACY_SETTINGS = [
  Settings.CHAT_TOOLS_FILTERS,
  BESPOKE_FIELDS.toolArgsMode.setting,
  BESPOKE_FIELDS.thinkingDisplay.setting,
  ...Object.values(BOOL_FIELDS),
  ...CAP_FIELD_KEYS.map((key) => CAP_FIELDS[key].setting),
  ...Object.values(SUBAGENT_FIELDS),
];

function resolveLegacyDisplay(
  cur: VerboseDisplayConfig,
  cli: Record<string, unknown>
): VerboseDisplayConfig {
  const bool = (key: string, fallback: boolean): boolean => {
    const value = cli[key];
    return typeof value === 'boolean' ? value : fallback;
  };
  const cap = (key: string, fallback: number | null): number | null => {
    if (cli[key] === null) return null;
    const value = cli[key];
    return typeof value === 'number' && Number.isFinite(value) && value > 0
      ? Math.floor(value)
      : fallback;
  };
  const argsMode = (fallback: ToolArgsMode): ToolArgsMode => {
    const value = cli[BESPOKE_FIELDS.toolArgsMode.setting];
    return value === 'off' || value === 'inline' || value === 'block'
      ? value
      : fallback;
  };
  const thinkingDisplay = resolveThinkingDisplay(
    cli[BESPOKE_FIELDS.thinkingDisplay.setting],
    cur.thinkingDisplay
  );
  const resolved: VerboseDisplayConfig = {
    ...cur,
    subagent: { ...cur.subagent },
    toolArgsMode: argsMode(cur.toolArgsMode),
    thinkingDisplay,
    showThinkingContent: thinkingDisplay !== 'off',
  };
  for (const local of BOOL_FIELD_KEYS) {
    resolved[local] = bool(BOOL_FIELDS[local], cur[local]);
  }
  for (const local of CAP_FIELD_KEYS) {
    resolved[local] = cap(CAP_FIELDS[local].setting, cur[local]);
  }
  for (const local of SUBAGENT_FIELD_KEYS) {
    resolved.subagent[local] = bool(
      SUBAGENT_FIELDS[local],
      cur.subagent[local]
    );
  }
  return sameDisplay(cur, resolved) ? cur : resolved;
}

function resolveLegacyFilters(
  fallback: string[],
  cli: Record<string, unknown>
): string[] {
  const value = cli[Settings.CHAT_TOOLS_FILTERS];
  if (!Array.isArray(value)) return fallback;
  return normalizeFilters(
    value.filter((token): token is string => typeof token === 'string')
  );
}

function loadConfig(surface: VerbositySurface): VerboseConfig {
  const cli = readCliSettings();
  const setting = VERBOSITY_VARIANTS[surface].setting;
  const saved = parseConfig(cli[setting], surface);
  if (saved) return saved;

  const legacyLite = surface === 'lite' ? readLegacyLiteConfig() : null;
  let config = legacyLite ?? defaultConfig(surface);
  const hasLegacyShared = LEGACY_SETTINGS.some((key) =>
    Object.prototype.hasOwnProperty.call(cli, key)
  );
  if (hasLegacyShared) {
    config = {
      filters: resolveLegacyFilters(config.filters, cli),
      display: resolveLegacyDisplay(config.display, cli),
    };
  } else if (
    surface === 'lite' &&
    legacyLite == null &&
    process.env.KIRO_LITE_VERBOSE === '1'
  ) {
    config.filters = ['all'];
  }

  if (legacyLite || hasLegacyShared) {
    try {
      const writable = readCliSettingsStrict();
      writable[setting] = config;
      writeCliSettings(writable);
    } catch (err) {
      logger.warn('[verbose] failed to migrate verbosity into cli.json', err);
    }
  }
  return config;
}

export function getVerboseConfig(
  surface: VerbositySurface = 'lite'
): VerboseConfig {
  cached[surface] ??= loadConfig(surface);
  return cached[surface]!;
}

/** Coerce the legacy shared thinking value to the current tri-state. */
export function resolveThinkingDisplay(
  v: unknown,
  fallback: ThinkingDisplayMode = 'expanded'
): ThinkingDisplayMode {
  if (v === 'off' || v === 'collapsed' || v === 'expanded') return v;
  if (v === false) return 'off';
  if (v === true) return 'collapsed';
  return fallback;
}

export function getVerboseDisplay(): VerboseDisplayConfig {
  return getVerboseConfig('lite').display;
}

export function getTuiVerboseDisplay(): VerboseDisplayConfig {
  if (process.env.KIRO_LITE_ROLLOUT_ENABLED !== '1') return TUI_DEFAULT_DISPLAY;
  return getVerboseConfig('tui').display;
}

export function getVerboseFilters(): string[] {
  return getVerboseConfig('lite').filters;
}

/** TUI filters preserve the pre-verbosity "all tool output" behavior. */
export function getTuiVerboseFilters(): string[] {
  if (process.env.KIRO_LITE_ROLLOUT_ENABLED !== '1') return ['all'];
  return getVerboseConfig('tui').filters;
}

export function sameDisplay(
  a: VerboseDisplayConfig,
  b: VerboseDisplayConfig
): boolean {
  if (a.toolArgsMode !== b.toolArgsMode) return false;
  if (a.thinkingDisplay !== b.thinkingDisplay) return false;
  for (const local of BOOL_FIELD_KEYS) if (a[local] !== b[local]) return false;
  for (const local of CAP_FIELD_KEYS) if (a[local] !== b[local]) return false;
  for (const local of SUBAGENT_FIELD_KEYS)
    if (a.subagent[local] !== b.subagent[local]) return false;
  return true;
}

export function sameFilters(
  a: readonly string[],
  b: readonly string[]
): boolean {
  if (a.length !== b.length) return false;
  if (a.length === 0) return true;
  const set = new Set(a);
  for (const t of b) {
    if (!set.has(t)) return false;
  }
  return true;
}

export interface VerboseConfigPatch {
  filters?: string[];
  display?: Partial<Omit<VerboseDisplayConfig, 'subagent'>> & {
    subagent?: Partial<SubagentDisplayConfig>;
  };
}

export function setVerboseConfig(
  patch: VerboseConfigPatch,
  surface: VerbositySurface = 'lite'
): boolean {
  const cur = getVerboseConfig(surface);
  const next: VerboseConfig = {
    filters: patch.filters !== undefined ? patch.filters : cur.filters,
    display: cur.display,
  };
  if (patch.filters !== undefined) {
    next.filters = normalizeFilters(patch.filters);
  }
  if (patch.display) {
    const curDisplay = cur.display;
    next.display = {
      ...curDisplay,
      ...patch.display,
      subagent: { ...curDisplay.subagent, ...(patch.display.subagent ?? {}) },
    };
    if (patch.display.thinkingDisplay !== undefined) {
      next.display.showThinkingContent =
        patch.display.thinkingDisplay !== 'off';
    } else if (patch.display.showThinkingContent !== undefined) {
      next.display.thinkingDisplay = patch.display.showThinkingContent
        ? 'collapsed'
        : 'off';
    }
  }
  // Keep the edit active for this session even if persistence fails; false
  // means "applied only", while true means "applied and saved".
  cached[surface] = next;
  let saved = true;
  try {
    const cli = readCliSettingsStrict();
    cli[VERBOSITY_VARIANTS[surface].setting] = next;
    // The off-cohort Display panel still consumes the legacy shared thinking
    // key; the verbosity rollout itself never writes shared settings.
    if (process.env.KIRO_LITE_ROLLOUT_ENABLED !== '1' && patch.display) {
      const mode =
        patch.display.thinkingDisplay ??
        (patch.display.showThinkingContent === undefined
          ? undefined
          : patch.display.showThinkingContent
            ? 'collapsed'
            : 'off');
      if (mode !== undefined)
        cli[BESPOKE_FIELDS.thinkingDisplay.setting] = mode;
    }
    writeCliSettings(cli);
  } catch (err) {
    logger.error('[verbose] failed to save config', err);
    saved = false;
  }
  notifyVerboseChanged();
  return saved;
}

export function applyDensityPreset(
  preset: DensityPreset,
  surface: VerbositySurface = 'lite'
): boolean {
  const display = getDensityPresetDisplay(preset, surface);
  const filters = [...getDensityPresetFilters(preset, surface)];
  return setVerboseConfig({ display, filters }, surface);
}

export function resetVerboseCache(): void {
  cached.lite = null;
  cached.tui = null;
  notifyVerboseChanged();
}

export function isMcpMessage(
  message: unknown
): message is { mcpServerName: string } {
  if (message == null || typeof message !== 'object') return false;
  const value = (message as { mcpServerName?: unknown }).mcpServerName;
  return typeof value === 'string' && value.length > 0;
}

export function shouldShowToolOutput(
  toolName: string,
  filtersOverride?: readonly string[],
  kind?: ToolKind,
  origin?: ToolCallOrigin
): boolean {
  const filters = filtersOverride ?? getVerboseFilters();
  const category = categorize(toolName, kind, origin);
  if (
    filters.some((token) => {
      const excluded = excludedFilter(token);
      return (
        excluded === toolName || (category != null && excluded === category)
      );
    })
  ) {
    return false;
  }
  if (filters.includes('all')) return true;
  if (filters.includes(toolName)) return true;
  return category != null && filters.includes(category);
}

export function categorize(
  toolName: string,
  kind?: ToolKind,
  origin?: ToolCallOrigin
): VerboseCategory | null {
  if (origin === 'mcp' || toolName.startsWith('mcp__')) return 'mcp';
  const policy = toolVerbosityPolicy(toolName, kind, origin);
  return 'category' in policy ? policy.category : null;
}

export function validateTokens(tokens: string[]): {
  accepted: string[];
  rejected: string[];
  unknown: string[];
} {
  const accepted: string[] = [];
  const rejected: string[] = [];
  const unknown: string[] = [];
  const knownCategories = new Set<string>(VERBOSE_CATEGORIES);
  for (const raw of tokens) {
    const t = raw.trim();
    if (!t) continue;
    if (t === 'all') {
      accepted.push(t);
      continue;
    }
    if (/[^a-zA-Z0-9_:.\-*]/.test(t)) {
      rejected.push(t);
      continue;
    }
    accepted.push(t);
    const candidate = excludedFilter(t) ?? t;
    if (
      !knownCategories.has(candidate) &&
      !KNOWN_TOOL_NAMES.has(candidate) &&
      !candidate.startsWith('mcp__')
    ) {
      unknown.push(t);
    }
  }
  return { accepted, rejected, unknown };
}

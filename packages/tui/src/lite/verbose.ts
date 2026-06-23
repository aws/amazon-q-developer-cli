/**
 * Verbose-mode configuration for lite UI, persisted at
 * ~/.kiro/settings/lite_verbose.json. The filter list is the sole output-bar
 * gate (`[]` = none, `['all']` = every tool). KIRO_LITE_VERBOSE=1 seeds
 * `['all']` only when no saved config exists.
 *
 * CLI.JSON CONTRACT: each display field is unified with the modern TUI's
 * cli.json settings. Precedence on read is cli.json > lite_verbose.json >
 * DEFAULT_DISPLAY ({@link getVerboseDisplay}); {@link setVerboseConfig} mirrors
 * every patched field back to cli.json so both surfaces stay in lockstep. The
 * lite_verbose.json copy is a stale fallback.
 */

import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { join } from 'path';
import { kiroHomePath } from '../utils/kiro-home.js';
import { logger } from '../utils/logger.js';
import { readCliSettings, writeCliSettings } from '../utils/cli-settings.js';
import { Settings } from '../constants/settings.js';
import {
  READ_TOOL_NAMES,
  SHELL_TOOL_NAMES,
  WEB_SEARCH_TOOL_NAMES,
  WEB_FETCH_TOOL_NAMES,
  GREP_TOOL_NAMES,
  GLOB_TOOL_NAMES,
  CODE_TOOL_NAMES,
  INTROSPECT_TOOL_NAMES,
  TASK_TOOL_NAMES,
  SESSION_TOOL_NAMES,
} from '../types/agent-events.js';

/** off — name + reasoning only; inline — `tool [arg]` chip; block — key:value tree. */
export type ToolArgsMode = 'off' | 'inline' | 'block';

/** Per-section toggles for the subagent final block (default: all on). */
export interface SubagentDisplayConfig {
  pipeline: boolean;
  prompts: boolean;
  roles: boolean;
  deps: boolean;
  responses: boolean;
}

/**
 * Display knobs that shape the chat scrollback rendering itself, distinct
 * from `filters` which only gate the post-tool output bar.
 */
export interface VerboseDisplayConfig {
  showToolReasoning: boolean;
  toolArgsMode: ToolArgsMode;
  showElapsed: boolean;
  subagent: SubagentDisplayConfig;
  /** Distinct from showToolReasoning (per-tool why); unified with the modern
   *  TUI's chat.showThinking — see CLI.JSON CONTRACT below. */
  showThinkingContent: boolean;
  /** Suppress only the write diff body; the header still evidences the write. */
  showWriteDiffs: boolean;
  showTasks: boolean;
  /** null = unbounded. Append-only: caps apply on first render only
   *  (Static-owned scrollback never reflows). Same for the other *Max* fields. */
  argsMaxLines: number | null;
  outputMaxLines: number | null;
  argsMaxChars: number | null;
  outputMaxChars: number | null;
}

export interface VerboseConfig {
  /** Sole output-bar gate: `[]` = none, `["all"]` = every tool, else a tool
   *  renders if its name OR category appears. See {@link VERBOSE_CATEGORIES}. */
  filters: string[];
  /** Optional display config; missing fields fall back to defaults. */
  display?: VerboseDisplayConfig;
}

/** Default display: reasoning + block args + elapsed on, all subagent sections on. */
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
  showThinkingContent: true,
  showWriteDiffs: true,
  showTasks: true,
  argsMaxLines: null,
  // Tail-window the live output bar (paired with filters: ['shell']) so the
  // default streams shell stdout in a bounded strip.
  outputMaxLines: 5,
  argsMaxChars: null,
  outputMaxChars: null,
};

export const DENSITY_PRESETS = ['minimal', 'lean', 'default', 'full'] as const;
export type DensityPreset = (typeof DENSITY_PRESETS)[number];

export const DENSITY_DISPLAY: Record<DensityPreset, VerboseDisplayConfig> = {
  minimal: {
    showToolReasoning: false,
    toolArgsMode: 'off',
    showElapsed: false,
    subagent: {
      pipeline: true,
      prompts: false,
      roles: false,
      deps: false,
      responses: false,
    },
    showThinkingContent: false,
    showWriteDiffs: false,
    showTasks: false,
    argsMaxLines: null,
    outputMaxLines: 5,
    argsMaxChars: 60,
    outputMaxChars: null,
  },
  lean: {
    showToolReasoning: false,
    toolArgsMode: 'inline',
    showElapsed: true,
    subagent: {
      pipeline: true,
      prompts: false,
      roles: false,
      deps: true,
      responses: true,
    },
    showThinkingContent: false,
    showWriteDiffs: false,
    showTasks: true,
    argsMaxLines: null,
    outputMaxLines: 10,
    argsMaxChars: 80,
    outputMaxChars: null,
  },
  default: { ...DEFAULT_DISPLAY },
  // "show me everything" — caps off; pairs with DENSITY_FILTERS.full = ['all'].
  full: {
    ...DEFAULT_DISPLAY,
    argsMaxLines: null,
    outputMaxLines: null,
    argsMaxChars: null,
    outputMaxChars: null,
  },
};

/** Filter list per preset; picking a preset resets filters too. */
export const DENSITY_FILTERS: Record<DensityPreset, readonly string[]> = {
  minimal: [],
  lean: [],
  default: ['shell'], // stream shell stdout out of the box
  full: ['all'],
};

/**
 * Built-in filter categories (mirror the *_TOOL_NAMES sets). `mcp` matches the
 * `mcp__` prefix; `subagent` covers the pipeline tool + joiner.
 *
 * NO `write` CATEGORY (referenced below): a write's tool result is just a
 * one-line "Successfully ..." that duplicates the diff body, so a write filter
 * would gate only redundant chrome — and errors surface regardless. Write
 * tools always render; the diff is bounded by outputMaxLines/outputMaxChars.
 */
export const VERBOSE_CATEGORIES = [
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
] as const;
export type VerboseCategory = (typeof VERBOSE_CATEGORIES)[number];

const DEFAULT_CONFIG: VerboseConfig = {
  filters: ['shell'], // matches DENSITY_FILTERS.default
  display: DEFAULT_DISPLAY,
};

// ── Field metadata ──────────────────────────────────────────────────────────
// One row per top-level display field maps the local key ↔ its cli.json
// Settings key. mergeDisplay/getVerboseDisplay/sameDisplay/setVerboseConfig all
// drive off these tables so a new field is added in one place. `toolArgsMode`
// and `showThinkingContent` keep bespoke handling (enum / tri-state) below.

type BoolDisplayKey =
  | 'showToolReasoning'
  | 'showElapsed'
  | 'showWriteDiffs'
  | 'showTasks';
const BOOL_FIELDS: { local: BoolDisplayKey; setting: string }[] = [
  { local: 'showToolReasoning', setting: Settings.CHAT_TOOLS_SHOW_REASONING },
  { local: 'showElapsed', setting: Settings.CHAT_TOOLS_SHOW_ELAPSED },
  { local: 'showWriteDiffs', setting: Settings.CHAT_TOOLS_SHOW_WRITE_DIFFS },
  { local: 'showTasks', setting: Settings.CHAT_SHOW_TASKS },
];

type CapDisplayKey =
  | 'argsMaxLines'
  | 'outputMaxLines'
  | 'argsMaxChars'
  | 'outputMaxChars';
// `defaultOnMissing`: char caps fall back to DEFAULT_DISPLAY when the saved
// field is absent (so a pre-field upgrade keeps the chip cap); line caps treat
// missing as unbounded.
const CAP_FIELDS: {
  local: CapDisplayKey;
  setting: string;
  defaultOnMissing: boolean;
}[] = [
  {
    local: 'argsMaxLines',
    setting: Settings.CHAT_TOOLS_ARGS_MAX_LINES,
    defaultOnMissing: false,
  },
  {
    local: 'outputMaxLines',
    setting: Settings.CHAT_TOOLS_OUTPUT_MAX_LINES,
    defaultOnMissing: false,
  },
  {
    local: 'argsMaxChars',
    setting: Settings.CHAT_TOOLS_ARGS_MAX_CHARS,
    defaultOnMissing: true,
  },
  {
    local: 'outputMaxChars',
    setting: Settings.CHAT_TOOLS_OUTPUT_MAX_CHARS,
    defaultOnMissing: true,
  },
];

type SubagentKey = keyof SubagentDisplayConfig;
const SUBAGENT_FIELDS: { local: SubagentKey; setting: string }[] = [
  { local: 'pipeline', setting: Settings.CHAT_SUBAGENT_SHOW_PIPELINE },
  { local: 'prompts', setting: Settings.CHAT_SUBAGENT_SHOW_PROMPTS },
  { local: 'roles', setting: Settings.CHAT_SUBAGENT_SHOW_ROLES },
  { local: 'deps', setting: Settings.CHAT_SUBAGENT_SHOW_DEPS },
  { local: 'responses', setting: Settings.CHAT_SUBAGENT_SHOW_RESPONSES },
];

function configPath(): string {
  return kiroHomePath('settings', 'lite_verbose.json');
}

let cached: VerboseConfig | null = null;

/** Coerce a saved cap to `number | null`; null/missing/non-positive = unbounded. */
function parseLineCap(raw: unknown): number | null {
  if (typeof raw !== 'number' || !Number.isFinite(raw)) return null;
  const n = Math.floor(raw);
  return n > 0 ? n : null;
}

/** Merge a partial on-disk display object with defaults (older configs miss fields). */
function mergeDisplay(raw: unknown): VerboseDisplayConfig {
  const out: VerboseDisplayConfig = {
    ...DEFAULT_DISPLAY,
    subagent: { ...DEFAULT_DISPLAY.subagent },
  };
  if (!raw || typeof raw !== 'object') return out;
  const obj = raw as Record<string, unknown>;
  for (const { local } of BOOL_FIELDS) {
    if (typeof obj[local] === 'boolean') out[local] = obj[local] as boolean;
  }
  if (
    obj.toolArgsMode === 'off' ||
    obj.toolArgsMode === 'inline' ||
    obj.toolArgsMode === 'block'
  ) {
    out.toolArgsMode = obj.toolArgsMode;
  }
  if (typeof obj.showThinkingContent === 'boolean')
    out.showThinkingContent = obj.showThinkingContent;
  for (const { local, defaultOnMissing } of CAP_FIELDS) {
    out[local] =
      defaultOnMissing && obj[local] === undefined
        ? DEFAULT_DISPLAY[local]
        : parseLineCap(obj[local]);
  }
  if (obj.subagent && typeof obj.subagent === 'object') {
    const sa = obj.subagent as Record<string, unknown>;
    for (const { local } of SUBAGENT_FIELDS) {
      if (typeof sa[local] === 'boolean')
        out.subagent[local] = sa[local] as boolean;
    }
  }
  return out;
}

/** Load config from disk. Cached after first call; safe across module
 *  reloads in tests because resetVerboseCache() clears it. */
export function getVerboseConfig(): VerboseConfig {
  if (cached) return cached;
  let filters = DEFAULT_CONFIG.filters;
  let display = DEFAULT_DISPLAY;
  let fileExists = false;
  try {
    const raw = readFileSync(configPath(), 'utf-8');
    fileExists = true;
    const obj = JSON.parse(raw);
    if (obj && typeof obj === 'object') {
      const sanitized = Array.isArray(obj.filters)
        ? obj.filters.filter(
            (f: unknown) => typeof f === 'string' && f.length > 0
          )
        : [];
      // Migration: legacy `enabled === false` forces filters off (dropped on
      // next save; setVerboseConfig never writes `enabled`).
      if (obj.enabled === false) {
        filters = [];
      } else {
        filters = sanitized;
      }
      if (obj.display) {
        display = mergeDisplay(obj.display);
      }
    }
  } catch {
    // Missing file or unparsable — start fresh from defaults.
  }
  // KIRO_LITE_VERBOSE=1 acts as a startup hint: when no config file exists,
  // seed filters to ['all'] so debugging from a shell shows tool output
  // without writing to disk. With a saved config it's a no-op.
  if (!fileExists && process.env.KIRO_LITE_VERBOSE === '1') {
    filters = ['all'];
  }
  cached = { filters, display };
  return cached;
}

/** Session display config with cli.json overrides applied (see CLI.JSON
 *  CONTRACT). Preserves object identity when nothing changed so upstream
 *  useMemo/Zustand selectors don't see a fresh reference each render. */
export function getVerboseDisplay(): VerboseDisplayConfig {
  const cur = getVerboseConfig().display ?? DEFAULT_DISPLAY;
  const cli = readCliSettings();

  // Each helper returns the well-typed cli.json value or falls back to `cur`
  // (already DEFAULT_DISPLAY-merged) so a corrupt entry can't break rendering.
  const bool = (key: string, fallback: boolean): boolean => {
    const v = cli[key];
    return typeof v === 'boolean' ? v : fallback;
  };
  const cap = (key: string, fallback: number | null): number | null => {
    if (cli[key] === null) return null;
    const v = cli[key];
    if (typeof v === 'number' && Number.isFinite(v) && v > 0) {
      return Math.floor(v);
    }
    return fallback;
  };
  const argsMode = (fallback: ToolArgsMode): ToolArgsMode => {
    const v = cli[Settings.CHAT_TOOLS_ARGS_MODE];
    if (v === 'off' || v === 'inline' || v === 'block') return v;
    return fallback;
  };

  const resolved: VerboseDisplayConfig = {
    ...cur,
    subagent: { ...cur.subagent },
    toolArgsMode: argsMode(cur.toolArgsMode),
    // CHAT_SHOW_THINKING is a shared tri-state ('collapsed'|'expanded'|'off';
    // legacy boolean honored). Lite collapses it to: shown unless 'off'/false.
    showThinkingContent: (() => {
      const v = cli[Settings.CHAT_SHOW_THINKING];
      if (v === 'off' || v === false) return false;
      if (v === 'collapsed' || v === 'expanded' || v === true) return true;
      return cur.showThinkingContent;
    })(),
  };
  for (const { local, setting } of BOOL_FIELDS) {
    resolved[local] = bool(setting, cur[local]);
  }
  for (const { local, setting } of CAP_FIELDS) {
    resolved[local] = cap(setting, cur[local]);
  }
  for (const { local, setting } of SUBAGENT_FIELDS) {
    resolved.subagent[local] = bool(setting, cur.subagent[local]);
  }

  if (sameDisplay(cur, resolved)) return cur; // preserve identity (see fn doc)
  return resolved;
}

/** Session filters with cli.json override (chat.tools.filters wins when it's a
 *  string array; else falls back to the cached lite_verbose list). */
export function getVerboseFilters(): string[] {
  const cur = getVerboseConfig().filters;
  const cli = readCliSettings();
  const v = cli[Settings.CHAT_TOOLS_FILTERS];
  if (!Array.isArray(v)) return cur;
  const out: string[] = [];
  for (const t of v) {
    if (typeof t === 'string' && t.length > 0) out.push(t);
  }
  // ['all'] short-circuits any mixed list (matches setVerboseConfig).
  return out.includes('all') ? ['all'] : out;
}

/** Structural equality for the identity-preservation guard above. */
function sameDisplay(
  a: VerboseDisplayConfig,
  b: VerboseDisplayConfig
): boolean {
  if (a.toolArgsMode !== b.toolArgsMode) return false;
  if (a.showThinkingContent !== b.showThinkingContent) return false;
  for (const { local } of BOOL_FIELDS) if (a[local] !== b[local]) return false;
  for (const { local } of CAP_FIELDS) if (a[local] !== b[local]) return false;
  for (const { local } of SUBAGENT_FIELDS)
    if (a.subagent[local] !== b.subagent[local]) return false;
  return true;
}

/** Patch for {@link setVerboseConfig}: display (and subagent) fields are
 *  shallow-merged; filters is the canonical full list (normalized/deduped). */
export interface VerboseConfigPatch {
  filters?: string[];
  display?: Partial<Omit<VerboseDisplayConfig, 'subagent'>> & {
    subagent?: Partial<SubagentDisplayConfig>;
  };
}

/** Persist a config patch and update the cache. Returns true on success. */
export function setVerboseConfig(patch: VerboseConfigPatch): boolean {
  // Explicit `display: undefined` (some tests) clears the saved display so
  // reads fall through to DEFAULT_DISPLAY; the merge below is gated on truthy.
  const cur = getVerboseConfig();
  const explicitlyClearedDisplay =
    'display' in patch && patch.display === undefined;
  const next: VerboseConfig = {
    filters: patch.filters !== undefined ? patch.filters : cur.filters,
    display: explicitlyClearedDisplay ? undefined : cur.display,
  };
  // Normalize filters: trim/dedupe; `[]` stays empty, `['all']` collapses.
  if (patch.filters) {
    const seen = new Set<string>();
    const out: string[] = [];
    for (const f of patch.filters) {
      const t = f.trim();
      if (!t || seen.has(t)) continue;
      seen.add(t);
      out.push(t);
    }
    next.filters = out.includes('all') ? ['all'] : out;
  }
  // Shallow-merge display (and subagent) so a single-key patch keeps the rest.
  if (patch.display) {
    const curDisplay = getVerboseDisplay();
    next.display = {
      ...curDisplay,
      ...patch.display,
      subagent: { ...curDisplay.subagent, ...(patch.display.subagent ?? {}) },
    };
  }
  // Cache before persisting so the new state holds for the session even if the
  // disk write fails; the bool return is "applied + saved" vs "applied only".
  cached = next;
  // Mirror each patched field to cli.json (see CLI.JSON CONTRACT). Only the
  // fields the patch sets are mirrored; best-effort (failure doesn't abort the
  // lite_verbose.json save). ACP setSetting is the call site's job.
  let mirroredAny = false;
  let cli: Record<string, unknown> | null = null;
  const stage = (key: string, value: unknown) => {
    if (cli == null) cli = readCliSettings();
    cli[key] = value;
    mirroredAny = true;
  };
  if (patch.filters !== undefined) {
    stage(Settings.CHAT_TOOLS_FILTERS, [...next.filters]);
  }
  if (patch.display) {
    const d = patch.display;
    for (const { local, setting } of BOOL_FIELDS) {
      if (d[local] !== undefined) stage(setting, !!d[local]);
    }
    if (d.toolArgsMode !== undefined) {
      stage(Settings.CHAT_TOOLS_ARGS_MODE, d.toolArgsMode);
    }
    if (d.showThinkingContent !== undefined) {
      stage(Settings.CHAT_SHOW_THINKING, !!d.showThinkingContent);
    }
    for (const { local, setting } of CAP_FIELDS) {
      if (d[local] !== undefined) stage(setting, d[local]);
    }
    if (d.subagent) {
      const sa = d.subagent;
      for (const { local, setting } of SUBAGENT_FIELDS) {
        if (sa[local] !== undefined) stage(setting, !!sa[local]);
      }
    }
  }
  if (mirroredAny && cli) {
    try {
      writeCliSettings(cli);
    } catch (err) {
      logger.warn('[verbose] failed to mirror verbosity to cli.json', err);
    }
  }
  try {
    const path = configPath();
    mkdirSync(join(path, '..'), { recursive: true });
    writeFileSync(path, JSON.stringify(next, null, 2) + '\n');
    return true;
  } catch (err) {
    logger.error('[verbose] failed to save config', err);
    return false;
  }
}

/** Apply a density preset: rewrite display AND reset filters to match. */
export function applyDensityPreset(preset: DensityPreset): boolean {
  const display = DENSITY_DISPLAY[preset];
  const filters = [...DENSITY_FILTERS[preset]];
  return setVerboseConfig({ display, filters });
}

/** Test/dev helper — drops the in-memory cache so the next get re-reads. */
export function resetVerboseCache(): void {
  cached = null;
}

/**
 * Whether a tool's output renders: `["all"]` → true; else an exact name or
 * matching category. `filtersOverride` lets the preview pane test a draft list.
 */
export function shouldShowToolOutput(
  toolName: string,
  filtersOverride?: readonly string[]
): boolean {
  // getVerboseFilters() (not the raw config) so the cli.json override applies
  // on the static path, which doesn't thread filtersOverride.
  const filters = filtersOverride ?? getVerboseFilters();
  if (filters.includes('all')) return true;
  if (filters.includes(toolName)) return true;
  const cat = categorize(toolName);
  return cat != null && filters.includes(cat);
}

/** Map a tool name to a category token; null when it fits none. No write
 *  category — see VERBOSE_CATEGORIES. */
export function categorize(toolName: string): VerboseCategory | null {
  if (toolName.startsWith('mcp__')) return 'mcp';
  if (SHELL_TOOL_NAMES.has(toolName)) return 'shell';
  if (READ_TOOL_NAMES.has(toolName)) return 'read';
  // Intentionally no WRITE_TOOL_NAMES branch — see docstring.
  if (WEB_SEARCH_TOOL_NAMES.has(toolName) || WEB_FETCH_TOOL_NAMES.has(toolName))
    return 'web';
  if (GREP_TOOL_NAMES.has(toolName)) return 'grep';
  if (GLOB_TOOL_NAMES.has(toolName)) return 'glob';
  if (CODE_TOOL_NAMES.has(toolName)) return 'code';
  if (INTROSPECT_TOOL_NAMES.has(toolName)) return 'introspect';
  if (TASK_TOOL_NAMES.has(toolName)) return 'task';
  if (SESSION_TOOL_NAMES.has(toolName)) return 'subagent';
  return null;
}

/**
 * Partition user tokens into accepted/rejected/unknown. Rejected = bad syntax;
 * unknown = accepted but neither a known category nor `mcp__`-prefixed (so the
 * caller can soft-warn typos while still allowing lazily-loaded tool names).
 */
export function validateTokens(tokens: string[]): {
  accepted: string[];
  rejected: string[];
  unknown: string[];
} {
  const accepted: string[] = [];
  const rejected: string[] = [];
  const unknown: string[] = [];
  const known = new Set<string>(VERBOSE_CATEGORIES);
  for (const raw of tokens) {
    const t = raw.trim();
    if (!t) continue;
    if (t === 'all') {
      accepted.push(t);
      continue;
    }
    // Reject whitespace / shell metachars to keep stored config sane.
    if (/[^a-zA-Z0-9_:.\-*]/.test(t)) {
      rejected.push(t);
      continue;
    }
    accepted.push(t);
    // Flag tokens that aren't a known category or mcp__ name as unknown.
    if (!known.has(t) && !t.startsWith('mcp__')) {
      unknown.push(t);
    }
  }
  return { accepted, rejected, unknown };
}

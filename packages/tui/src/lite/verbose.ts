/**
 * Verbose-mode configuration for lite UI.
 *
 * Persisted at ~/.kiro/settings/lite_verbose.json. The filter list is the
 * single source of truth for which tools surface an output bar — there is
 * no separate enabled/disabled gate. `filters: []` means no output bar
 * renders for any tool; `filters: ['all']` shows it for every tool.
 *
 * KIRO_LITE_VERBOSE=1 acts as a startup hint when no saved config exists —
 * it seeds `filters: ['all']`. With a saved config it's a no-op.
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

/**
 * How tool args render below the tool-name line.
 *   off    — no args, just the name + reasoning
 *   inline — single-line `tool [arg]` chip (shell-style)
 *   block  — current behavior: full key:value tree under the name
 */
export type ToolArgsMode = 'off' | 'inline' | 'block';

/**
 * Per-section toggles for the subagent final block. The pipeline tree, prompts,
 * roles, dependency arrows, and per-stage responses each get their own knob so
 * a user can dial in exactly what survives in scrollback. Defaults match the
 * pre-extension behavior (everything on).
 */
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
  /**
   * Show the agent's thinking content — both the live preview as it streams
   * and the persisted block in scrollback once the turn finishes. Distinct
   * from {@link showToolReasoning} (which surfaces the *per-tool-call* "why"
   * field a model attaches to each tool); this knob controls only the
   * model's freeform pre-response thinking. `false` collapses the live region
   * back to the plain "thinking" indicator and skips the persisted block
   * entirely. Set per density preset (default + full = on, lean + minimal =
   * off) but exposed as an individual toggle so the Custom flow can override.
   *
   * Unified with the modern TUI's `chat.showThinking` setting
   * (Settings.CHAT_SHOW_THINKING, surfaced in /settings → Display → "Show
   * thinking"). Both lite's /verbosity and the modern TUI's settings panel
   * read AND write the same source of truth — `chat.showThinking` in
   * `cli.json`. {@link getVerboseDisplay} overrides this field at read time
   * with the value from `cli.json`, and {@link setVerboseConfig} mirrors any
   * write of `display.showThinkingContent` back to `cli.json` so the modern
   * TUI's `useShowThinking` hook (and its initial-load `readBoolSetting`)
   * stay in lockstep with whatever lite's menu shows. The verbose-config.json
   * copy is kept around so older callers reading `getVerboseConfig().display`
   * directly still see a coherent value.
   */
  showThinkingContent: boolean;
  /**
   * Whether the diff *body* of a write tool call surfaces in scrollback.
   * The tool-call header (name, status, elapsed) always renders — this knob
   * suppresses ONLY the diff body so a user who finds diffs noisy in
   * minimal/lean modes still sees evidence the write fired. Implementation
   * threads through {@link renderWriteToolCall}'s existing `suppressDiff`
   * parameter (which already returns just the header line).
   *
   * Defaults: `default` and `full` keep diffs on (the typical "I want to
   * see the change" expectation); `lean` and `minimal` turn diffs off so
   * those quieter modes stay quiet. Errors still render — the renderVerbose
   * Output error block fires regardless of this flag.
   */
  showWriteDiffs: boolean;
  /**
   * Whether the lite task tray ({@link LiteTaskTray}) renders above the
   * input. The tray surfaces `todo_list` / `task` tool state regardless
   * of which tools are visible in scrollback, so users who don't want the
   * pinned tray on screen need a knob independent of the per-tool
   * filters. `false` suppresses both the collapsed summary and the
   * expanded list — the tray returns null entirely. Tasks themselves are
   * still tracked in the store; toggling back to `true` re-renders the
   * current task state.
   *
   * Default: `true`. The `minimal` density preset turns it off so a user
   * picking minimal sees only the chat itself with no pinned UI surface.
   */
  showTasks: boolean;
  /**
   * Max VISUAL rows to render in the tool-args block (block mode only)
   * before truncating with a non-interactive marker. `null` (or any
   * non-positive value) means unbounded.
   *
   * Append-only: caps apply on the FIRST render only. Already-rendered
   * scrollback never changes when the user adjusts the value, since
   * lite mode renders into <Static> and the terminal owns the buffer.
   */
  argsMaxLines: number | null;
  /**
   * Max VISUAL rows to render in the tool-output `│` bar before
   * truncating with a non-interactive marker. `null` (or any non-positive
   * value) means unbounded. Same append-only semantics as argsMaxLines.
   */
  outputMaxLines: number | null;
  /**
   * Max characters per args VALUE before tail-truncating with `…`. Applies
   * to the inline arg chip (`tool [command…]`) as well as long string
   * values inside the block-args tree. `null` means unbounded — useful
   * for users who want to see the full shell command on the chip line.
   * Defaults to 120 — wide enough that typical shell invocations and file
   * paths fit on the chip line without truncation, while still bounding
   * pathological multi-line args.
   */
  argsMaxChars: number | null;
  /**
   * Max characters per output line before tail-truncating with `…`.
   * Applies row-by-row inside the `│` output bar. `null` means unbounded.
   * Defaults to null — output lines wrap to terminal width naturally.
   */
  outputMaxChars: number | null;
}

export interface VerboseConfig {
  /**
   * Filter tokens — the sole gate on whether a tool's output bar renders.
   * `[]` means no output bar for any tool. `["all"]` means every tool.
   * Other valid tokens: category names (see {@link VERBOSE_CATEGORIES}) and
   * exact tool names. A tool's output renders if its name OR its category
   * appears in this list.
   */
  filters: string[];
  /** Optional display configuration. Missing fields fall back to defaults. */
  display?: VerboseDisplayConfig;
}

/**
 * Default display config. Matches the pre-extension renderer behavior so
 * existing users don't see anything change after upgrading: reasoning + block
 * args + elapsed all on, every subagent section visible.
 */
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
  // Tail-window cap on the live tool-output bar. Five lines is enough to
  // show the latest activity from a streaming command without dominating
  // the screen — older lines are surfaced via the "+N more lines above"
  // marker. The default is paired with `filters: ['shell']` below so the
  // out-of-the-box experience streams shell stdout in a tail-windowed
  // strip; users who want unbounded output can switch to /verbose density
  // full or set outputMaxLines manually.
  outputMaxLines: 5,
  // Args are uncapped by default — full shell commands and file paths read
  // cleaner without mid-string ellipsis. Users who want short chips can set
  // argsMaxChars manually or pick a tighter density preset.
  argsMaxChars: null,
  outputMaxChars: null,
};

/**
 * Density presets the user can select from the menu instead of toggling each
 * knob by hand. Each preset rewrites the display fields and leaves the filter
 * list alone, so a user who set up custom filters doesn't lose them by
 * switching preset.
 */
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
    // args mode is 'off' so argsMaxLines is moot, but cap the output bar
    // tightly — minimal density users want short tool footprints.
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
    // inline mode collapses args onto the tool-name line, so the args
    // block doesn't render — argsMaxLines is irrelevant. Output bar
    // gets a moderate cap.
    argsMaxLines: null,
    outputMaxLines: 10,
    argsMaxChars: 80,
    outputMaxChars: null,
  },
  // `default` is the out-of-the-box preset — matches DEFAULT_DISPLAY exactly
  // and is the canonical "I want the standard view" choice. Replaces the
  // old standalone `Reset to defaults` row.
  default: { ...DEFAULT_DISPLAY },
  // `full` is the "show me everything" preset — every cap explicitly off so
  // args and tool output render in full. The filter list also expands to
  // ['all'] (DENSITY_FILTERS.full below) so every tool's output surfaces.
  full: {
    ...DEFAULT_DISPLAY,
    argsMaxLines: null,
    outputMaxLines: null,
    argsMaxChars: null,
    outputMaxChars: null,
  },
};

/**
 * Filter-list shape per preset. Density now fully replaces the saved config
 * (display + filters) rather than only patching the display block — picking
 * a preset is a clean reset to that preset's full intent. `default` and the
 * non-`full` presets clear filters; `full` enables `['all']`.
 */
export const DENSITY_FILTERS: Record<DensityPreset, readonly string[]> = {
  minimal: [],
  lean: [],
  // Shell-only by default: a freshly-installed user sees streaming stdout
  // for `execute_bash`-class tools (the most common "what is this thing
  // doing?" signal) without other tool categories crowding the chat log.
  // Pairs with DEFAULT_DISPLAY.outputMaxLines = 5 above to bound footprint.
  default: ['shell'],
  full: ['all'],
};

/**
 * Built-in categories available as filter tokens. Mirrors the *_TOOL_NAMES
 * sets in agent-events.ts so the user can write the same vocabulary they
 * see in tool docs. `mcp` matches anything starting with the `mcp__` prefix
 * (the wire convention for MCP-routed tools); `subagent` covers the parent
 * pipeline tool and its joiner.
 *
 * `write` is deliberately NOT a category. Write tools render as a unified
 * diff (see {@link renderWriteToolCall}); their
 * actual tool result is just a one-line `Successfully ...` confirmation
 * that duplicates what the diff already shows visually. A user-facing
 * `write` filter would only gate that redundant chrome line — and errors
 * always surface regardless of filter — so the toggle was effectively a
 * no-op. Write tools always render in scrollback like any other tool;
 * the diff body renders in full (it opts out of outputMaxLines), and the
 * argsMode toggle controls how their args render.
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
  // Match DENSITY_FILTERS.default so a fresh install (no saved file)
  // and `/verbose density default` produce the same on-screen state.
  filters: ['shell'],
  display: DEFAULT_DISPLAY,
};

function configPath(): string {
  return kiroHomePath('settings', 'lite_verbose.json');
}

let cached: VerboseConfig | null = null;

/**
 * Coerce a saved `argsMaxLines` / `outputMaxLines` value into the
 * canonical `number | null` shape. `null`, missing, or non-positive
 * values all mean "unbounded" so a stale config can't accidentally
 * truncate everything to zero.
 */
function parseLineCap(raw: unknown): number | null {
  if (typeof raw !== 'number' || !Number.isFinite(raw)) return null;
  const n = Math.floor(raw);
  return n > 0 ? n : null;
}

/**
 * Merge a possibly-partial display object from disk with the defaults so
 * older saved configs without the new fields still produce a fully-populated
 * display config (no `undefined` checks needed at every render site).
 */
function mergeDisplay(raw: unknown): VerboseDisplayConfig {
  const out: VerboseDisplayConfig = {
    ...DEFAULT_DISPLAY,
    subagent: { ...DEFAULT_DISPLAY.subagent },
  };
  if (!raw || typeof raw !== 'object') return out;
  const obj = raw as Record<string, unknown>;
  if (typeof obj.showToolReasoning === 'boolean')
    out.showToolReasoning = obj.showToolReasoning;
  if (
    obj.toolArgsMode === 'off' ||
    obj.toolArgsMode === 'inline' ||
    obj.toolArgsMode === 'block'
  ) {
    out.toolArgsMode = obj.toolArgsMode;
  }
  if (typeof obj.showElapsed === 'boolean') out.showElapsed = obj.showElapsed;
  if (typeof obj.showThinkingContent === 'boolean')
    out.showThinkingContent = obj.showThinkingContent;
  if (typeof obj.showWriteDiffs === 'boolean')
    out.showWriteDiffs = obj.showWriteDiffs;
  if (typeof obj.showTasks === 'boolean') out.showTasks = obj.showTasks;
  // argsMaxLines / outputMaxLines / argsMaxChars / outputMaxChars accept
  // either `null` (unbounded) or a positive integer. Anything else
  // (negative, NaN, non-number) is treated as unbounded so a corrupted
  // config doesn't suppress all output silently. Char and line caps are
  // independent — both can be active and whichever fires first wins.
  out.argsMaxLines = parseLineCap(obj.argsMaxLines);
  out.outputMaxLines = parseLineCap(obj.outputMaxLines);
  // For char caps, missing fields fall back to the DEFAULT_DISPLAY values
  // so an upgrade from a config that pre-dates these fields keeps the
  // current chip cap (DEFAULT.argsMaxChars) rather than silently switching
  // to unlimited.
  out.argsMaxChars =
    obj.argsMaxChars === undefined
      ? DEFAULT_DISPLAY.argsMaxChars
      : parseLineCap(obj.argsMaxChars);
  out.outputMaxChars =
    obj.outputMaxChars === undefined
      ? DEFAULT_DISPLAY.outputMaxChars
      : parseLineCap(obj.outputMaxChars);
  if (obj.subagent && typeof obj.subagent === 'object') {
    const sa = obj.subagent as Record<string, unknown>;
    for (const k of [
      'pipeline',
      'prompts',
      'roles',
      'deps',
      'responses',
    ] as const) {
      if (typeof sa[k] === 'boolean') out.subagent[k] = sa[k] as boolean;
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
      // Migration: legacy configs carried a master `enabled` boolean.
      // `enabled === false` overrides the filter list to preserve the
      // user's "off" intent; `enabled === true` keeps the saved filters.
      // The field is dropped on next save (setVerboseConfig never writes it).
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

/** Always-defined display config for this session — falls back to defaults
 *  when the saved file lacks a `display` block.
 *
 *  Every field is overridden at read time with the value from `cli.json`
 *  when present, so lite's /verbosity menu and the modern TUI's settings
 *  surfaces share a single source of truth. The `lite_verbose.json` copy
 *  is still maintained by {@link setVerboseConfig} but is treated as a
 *  stale fallback — the cli.json value wins on every read, falling back
 *  to the cached lite_verbose value (which itself falls back to
 *  DEFAULT_DISPLAY via mergeDisplay) when cli.json lacks the key.
 *
 *  Cost: one tiny `readFileSync` of `cli.json` per call. The file is
 *  small and on the OS page cache after the first read; the modern TUI's
 *  surfaces (DisplaySettingsPanel, useGlyphs initial-load) hit the same
 *  path without issue. The returned object preserves identity when every
 *  cli.json override matches the cache so `useMemo`/Zustand selectors
 *  upstream don't see a fresh reference each render.
 */
export function getVerboseDisplay(): VerboseDisplayConfig {
  const cur = getVerboseConfig().display ?? DEFAULT_DISPLAY;
  const cli = readCliSettings();

  // Per-field cli.json overrides. Each helper returns the cli.json value
  // when present and well-typed, otherwise falls back to `cur` (which
  // already has DEFAULT_DISPLAY applied via mergeDisplay). The fallback
  // chain is cli.json > lite_verbose.json > DEFAULT_DISPLAY.
  const bool = (key: string, fallback: boolean): boolean => {
    const v = cli[key];
    return typeof v === 'boolean' ? v : fallback;
  };
  // Numeric caps accept either a positive integer or null (unlimited).
  // Anything else (negative, NaN, non-number) falls back to `cur` so a
  // corrupt cli.json entry can't silently truncate output to zero.
  const cap = (key: string, fallback: number | null): number | null => {
    if (cli[key] === null) return null;
    const v = cli[key];
    if (typeof v === 'number' && Number.isFinite(v) && v > 0) {
      return Math.floor(v);
    }
    return fallback;
  };
  // toolArgsMode is a string enum; only the three valid tokens are
  // accepted. Other values (typo, missing key) fall back.
  const argsMode = (fallback: ToolArgsMode): ToolArgsMode => {
    const v = cli[Settings.CHAT_TOOLS_ARGS_MODE];
    if (v === 'off' || v === 'inline' || v === 'block') return v;
    return fallback;
  };

  const resolved: VerboseDisplayConfig = {
    showToolReasoning: bool(
      Settings.CHAT_TOOLS_SHOW_REASONING,
      cur.showToolReasoning
    ),
    toolArgsMode: argsMode(cur.toolArgsMode),
    showElapsed: bool(Settings.CHAT_TOOLS_SHOW_ELAPSED, cur.showElapsed),
    subagent: {
      pipeline: bool(
        Settings.CHAT_SUBAGENT_SHOW_PIPELINE,
        cur.subagent.pipeline
      ),
      prompts: bool(Settings.CHAT_SUBAGENT_SHOW_PROMPTS, cur.subagent.prompts),
      roles: bool(Settings.CHAT_SUBAGENT_SHOW_ROLES, cur.subagent.roles),
      deps: bool(Settings.CHAT_SUBAGENT_SHOW_DEPS, cur.subagent.deps),
      responses: bool(
        Settings.CHAT_SUBAGENT_SHOW_RESPONSES,
        cur.subagent.responses
      ),
    },
    // CHAT_SHOW_THINKING is now main's shared tri-state setting
    // ('collapsed' | 'expanded' | 'off'; legacy boolean still honored — see
    // useGlyphs.resolveThinkingMode). Lite has no separate collapsed/expanded
    // rendering, so it collapses the tri-state to a boolean: thinking is shown
    // unless the mode is 'off' (or the legacy `false`). A missing value falls
    // back to the lite_verbose.json / DEFAULT_DISPLAY value, preserving lite's
    // cli.json > lite_verbose.json > default precedence.
    showThinkingContent: (() => {
      const v = cli[Settings.CHAT_SHOW_THINKING];
      if (v === 'off' || v === false) return false;
      if (v === 'collapsed' || v === 'expanded' || v === true) return true;
      return cur.showThinkingContent;
    })(),
    showWriteDiffs: bool(
      Settings.CHAT_TOOLS_SHOW_WRITE_DIFFS,
      cur.showWriteDiffs
    ),
    showTasks: bool(Settings.CHAT_SHOW_TASKS, cur.showTasks),
    argsMaxLines: cap(Settings.CHAT_TOOLS_ARGS_MAX_LINES, cur.argsMaxLines),
    outputMaxLines: cap(
      Settings.CHAT_TOOLS_OUTPUT_MAX_LINES,
      cur.outputMaxLines
    ),
    argsMaxChars: cap(Settings.CHAT_TOOLS_ARGS_MAX_CHARS, cur.argsMaxChars),
    outputMaxChars: cap(
      Settings.CHAT_TOOLS_OUTPUT_MAX_CHARS,
      cur.outputMaxChars
    ),
  };

  // Preserve object identity when nothing changed — most ticks are no-op
  // re-renders that read the same cli.json values. A fresh allocation
  // every call would dirty `useMemo([display])` deps in the modern TUI.
  if (sameDisplay(cur, resolved)) return cur;
  return resolved;
}

/** Always-defined filters list for this session, with the same cli.json
 *  override semantics as {@link getVerboseDisplay}: `chat.tools.filters`
 *  in cli.json wins over the lite_verbose.json `filters` array. The
 *  override only applies when cli.json contains a string array; any
 *  other type (missing, malformed, non-array) falls back to the cached
 *  lite_verbose value so a corrupt cli.json entry can't silently disable
 *  every output bar. */
export function getVerboseFilters(): string[] {
  const cur = getVerboseConfig().filters;
  const cli = readCliSettings();
  const v = cli[Settings.CHAT_TOOLS_FILTERS];
  if (!Array.isArray(v)) return cur;
  const out: string[] = [];
  for (const t of v) {
    if (typeof t === 'string' && t.length > 0) out.push(t);
  }
  // Match setVerboseConfig's normalization: ['all'] short-circuits any
  // mixed list. Without this a stale cli.json entry could drift from
  // the canonical form.
  return out.includes('all') ? ['all'] : out;
}

/** Pure helper: structural equality on two VerboseDisplayConfig values.
 *  Used by getVerboseDisplay's identity-preservation guard. Kept here
 *  rather than imported from effects.ts (where the effect handler has
 *  its own `sameDisplay`) to avoid pulling the verbose-config module
 *  into the effect handler's import graph. */
function sameDisplay(
  a: VerboseDisplayConfig,
  b: VerboseDisplayConfig
): boolean {
  return (
    a.showToolReasoning === b.showToolReasoning &&
    a.toolArgsMode === b.toolArgsMode &&
    a.showElapsed === b.showElapsed &&
    a.showThinkingContent === b.showThinkingContent &&
    a.showWriteDiffs === b.showWriteDiffs &&
    a.showTasks === b.showTasks &&
    a.argsMaxLines === b.argsMaxLines &&
    a.outputMaxLines === b.outputMaxLines &&
    a.argsMaxChars === b.argsMaxChars &&
    a.outputMaxChars === b.outputMaxChars &&
    a.subagent.pipeline === b.subagent.pipeline &&
    a.subagent.prompts === b.subagent.prompts &&
    a.subagent.roles === b.subagent.roles &&
    a.subagent.deps === b.subagent.deps &&
    a.subagent.responses === b.subagent.responses
  );
}

/**
 * Patch shape for {@link setVerboseConfig}. Accepts a partial display
 * block (the runtime shallow-merges fields against the current config)
 * so callers can patch a single key without spreading the full
 * DEFAULT_DISPLAY. The same applies to `subagent` — its sub-fields are
 * shallow-merged independently. Filters are still all-or-nothing because
 * the array is treated as the canonical full list (the runtime
 * normalizes / dedupes / collapses to `['all']`).
 */
export interface VerboseConfigPatch {
  filters?: string[];
  display?: Partial<Omit<VerboseDisplayConfig, 'subagent'>> & {
    subagent?: Partial<SubagentDisplayConfig>;
  };
}

/** Persist a config patch and update the cache. Returns true on success. */
export function setVerboseConfig(patch: VerboseConfigPatch): boolean {
  // Start with the current config, then apply patch fields below. Cannot
  // just spread `patch` since `patch.display` is partial — the merge
  // logic for `display` and `subagent` happens later. When the caller
  // explicitly passes `display: undefined` (some tests do this to reset
  // the saved display block), we honor that by not carrying forward the
  // current display either; the merge logic below is gated on
  // `patch.display` truthy and won't run, leaving `next.display`
  // undefined so subsequent reads fall through to DEFAULT_DISPLAY.
  const cur = getVerboseConfig();
  const explicitlyClearedDisplay =
    'display' in patch && patch.display === undefined;
  const next: VerboseConfig = {
    filters: patch.filters !== undefined ? patch.filters : cur.filters,
    display: explicitlyClearedDisplay ? undefined : cur.display,
  };
  // Normalize filters: trim, dedupe. `[]` is preserved as the canonical
  // "off" form (no output bar); `['all']` collapses any mixed list since
  // downstream matching short-circuits on it. Note: unlike the old behavior,
  // we no longer rewrite `[]` to `['all']` — empty means empty.
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
  // Same for display: shallow-merge so callers can pass `{ display: { showElapsed: false } }`
  // without nuking the other keys. Subagent is itself shallow-merged so a partial
  // `{ subagent: { pipeline: false } }` patch leaves the other section toggles alone.
  if (patch.display) {
    const curDisplay = getVerboseDisplay();
    next.display = {
      ...curDisplay,
      ...patch.display,
      subagent: { ...curDisplay.subagent, ...(patch.display.subagent ?? {}) },
    };
  }
  // Update the in-memory cache before attempting to persist so the new
  // state takes effect for the rest of the session even when the disk
  // write fails (e.g. read-only home dir, mocked fs in tests). The
  // boolean return distinguishes "applied + saved" from "applied only".
  cached = next;
  // Mirror display + filters fields to cli.json so the modern TUI's
  // /settings surfaces and lite's /verbosity menu read the same values
  // via getVerboseDisplay / getVerboseFilters (which both override from
  // cli.json) and via the modern TUI's useShowThinking / readBoolSetting
  // paths. We only mirror fields the patch explicitly sets — density-
  // preset writes carry the full display block (every field included)
  // and need every mirror; per-key toggles only mirror the field the
  // user actually changed. Best-effort: a failure to write cli.json
  // doesn't abort the lite_verbose.json save.
  //
  // NOTE: setSetting via ACP is the responsibility of the call site
  // (e.g. effects.ts verbosityConfig handler) — we write cli.json
  // directly here so the file stays in sync regardless of whether the
  // ACP backend is up. The Rust-side in-memory copy of any cached
  // settings is updated by the call site's setSetting RPC.
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
    if (d.showToolReasoning !== undefined) {
      stage(Settings.CHAT_TOOLS_SHOW_REASONING, !!d.showToolReasoning);
    }
    if (d.toolArgsMode !== undefined) {
      stage(Settings.CHAT_TOOLS_ARGS_MODE, d.toolArgsMode);
    }
    if (d.showElapsed !== undefined) {
      stage(Settings.CHAT_TOOLS_SHOW_ELAPSED, !!d.showElapsed);
    }
    if (d.showThinkingContent !== undefined) {
      stage(Settings.CHAT_SHOW_THINKING, !!d.showThinkingContent);
    }
    if (d.showWriteDiffs !== undefined) {
      stage(Settings.CHAT_TOOLS_SHOW_WRITE_DIFFS, !!d.showWriteDiffs);
    }
    if (d.showTasks !== undefined) {
      stage(Settings.CHAT_SHOW_TASKS, !!d.showTasks);
    }
    if (d.argsMaxLines !== undefined) {
      stage(Settings.CHAT_TOOLS_ARGS_MAX_LINES, d.argsMaxLines);
    }
    if (d.outputMaxLines !== undefined) {
      stage(Settings.CHAT_TOOLS_OUTPUT_MAX_LINES, d.outputMaxLines);
    }
    if (d.argsMaxChars !== undefined) {
      stage(Settings.CHAT_TOOLS_ARGS_MAX_CHARS, d.argsMaxChars);
    }
    if (d.outputMaxChars !== undefined) {
      stage(Settings.CHAT_TOOLS_OUTPUT_MAX_CHARS, d.outputMaxChars);
    }
    if (d.subagent) {
      const sa = d.subagent;
      if (sa.pipeline !== undefined) {
        stage(Settings.CHAT_SUBAGENT_SHOW_PIPELINE, !!sa.pipeline);
      }
      if (sa.prompts !== undefined) {
        stage(Settings.CHAT_SUBAGENT_SHOW_PROMPTS, !!sa.prompts);
      }
      if (sa.roles !== undefined) {
        stage(Settings.CHAT_SUBAGENT_SHOW_ROLES, !!sa.roles);
      }
      if (sa.deps !== undefined) {
        stage(Settings.CHAT_SUBAGENT_SHOW_DEPS, !!sa.deps);
      }
      if (sa.responses !== undefined) {
        stage(Settings.CHAT_SUBAGENT_SHOW_RESPONSES, !!sa.responses);
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

/**
 * Apply a density preset. Picking a preset is a clean reset to that preset's
 * full intent: the display block is rewritten AND the filter list is reset
 * to match (`['all']` for `full`, `[]` for the rest). Custom filter lists
 * survive only when the user is in the Custom flow — the Custom row in the
 * density menu does not pass through here.
 */
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
 * Decide if a given tool's output should render in the lite output bar.
 * The filter list is the single gate: empty filters means no tool output
 * surfaces; `["all"]` short-circuits to true; otherwise an exact name or
 * matching category lets the tool through.
 *
 * `filtersOverride` lets a caller test against an explicit list rather than
 * disk config — used by the /verbosity preview pane so toggling a filter
 * reflects in the synthetic preview without touching saved config.
 */
export function shouldShowToolOutput(
  toolName: string,
  filtersOverride?: readonly string[]
): boolean {
  // Fall back to getVerboseFilters() (not getVerboseConfig().filters) so the
  // cli.json CHAT_TOOLS_FILTERS override applies on the static render path,
  // which doesn't thread an explicit filtersOverride.
  const filters = filtersOverride ?? getVerboseFilters();
  if (filters.includes('all')) return true;
  if (filters.includes(toolName)) return true;
  const cat = categorize(toolName);
  return cat != null && filters.includes(cat);
}

/**
 * Map a tool name to a category token. MCP-routed tools (prefix `mcp__`)
 * always categorize as `mcp` — the prefix is the only stable signal we
 * have on the TUI side. Returns null for tools that don't fit any bucket.
 *
 * Write tools (fs_write, str_replace, edit, ...) deliberately don't
 * categorize. Their tool result is just a one-line `Successfully ...`
 * confirmation that duplicates the diff body the user is already seeing,
 * so there's no useful "write output bar" for a per-category filter to
 * gate. Errors still render via the bypass path in renderVerboseOutput.
 * Write tools always render in scrollback like any other tool — the diff
 * body is bounded by outputMaxLines / outputMaxChars. See
 * VERBOSE_CATEGORIES for the full rationale.
 */
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
 * Validate a list of tokens the user typed (e.g. `/verbose only shell foo`).
 * Returns a partition of accepted/rejected tokens so the effect handler can
 * surface a warning for typos rather than silently dropping them. Tokens that
 * pass the syntax check but match neither a known category nor the `mcp__`
 * prefix are returned in `unknown` — accepted (so MCP tool names loaded later
 * still work) but flagged so the caller can soft-warn the user.
 *
 * Exact tool names are always accepted — we can't enumerate every backend or
 * MCP tool the user might run, and rejecting unknown names would break
 * targeting (e.g. mcp__some-server-not-yet-loaded__tool).
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
    // Heuristic: we accept anything that looks like a tool name — bare
    // identifiers, mcp__ prefixed names, or recognized categories. Reject
    // tokens with whitespace or shell metachars to keep stored config sane.
    if (/[^a-zA-Z0-9_:.\-*]/.test(t)) {
      rejected.push(t);
      continue;
    }
    accepted.push(t);
    // mcp__ prefix is a known shape (MCP tools load lazily); known categories
    // are documented. Anything else we accept but mark as unknown so the
    // caller can warn the user — typos shouldn't silently land in config.
    if (!known.has(t) && !t.startsWith('mcp__')) {
      unknown.push(t);
    }
  }
  return { accepted, rejected, unknown };
}

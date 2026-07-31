/**
 * Which status-line segments a surface shows.
 *
 * Stored in the global `cli.json` as `{ [segmentId]: boolean }`, one key per
 * surface, because the two surfaces have different room: lite wraps sooner, so a
 * shared default would force one of them into a bad layout.
 *
 * Visibility only, never position: the full TUI splits its segments into a
 * left-aligned and a right-aligned group, so a caller-supplied order would apply
 * on lite and silently not there.
 *
 * Reads are synchronous on purpose — the bar paints on the first frame, before
 * the ACP backend is up, so it cannot wait on session info.
 */
import { logger } from '../../../utils/logger.js';
import {
  readCliSettings,
  updateCliSettingWith,
} from '../../../utils/cli-settings.js';
import { Settings } from '../../../constants/settings.js';
import type { UiMode } from '../../../types/ui-mode.js';
import {
  STATUS_SEGMENT_IDS,
  isStatusSegmentId,
  CLOCK_SEGMENT_IDS,
  BILLING_SEGMENT_IDS,
  type StatusSegmentId,
} from './segments.js';

const SETTING_KEY: Record<UiMode, string> = {
  tui: Settings.CHAT_STATUS_LINE_TUI,
  lite: Settings.CHAT_STATUS_LINE_LITE,
};

/**
 * Segments each surface shows with no saved configuration, matching what it
 * painted before any of this was configurable.
 *
 * The two differ: lite has never painted the code-intelligence indicator. New
 * segments start off on both so an existing user's bar is unchanged.
 */
const DEFAULT_VISIBLE: Record<UiMode, readonly StatusSegmentId[]> = {
  tui: [
    'agent',
    'autonomous',
    'model',
    'effort',
    'context',
    'tangent',
    'codeIntel',
    'goal',
    'location',
    'branch',
  ],
  lite: [
    'agent',
    'autonomous',
    'model',
    'effort',
    'context',
    'tangent',
    'goal',
    'location',
    'branch',
  ],
};

export type StatusSegmentVisibility = Record<StatusSegmentId, boolean>;

/** What `surface` shows with nothing overridden. */
export function defaultStatusSegments(
  surface: UiMode
): StatusSegmentVisibility {
  const on = new Set(DEFAULT_VISIBLE[surface]);
  return Object.fromEntries(
    STATUS_SEGMENT_IDS.map((id) => [id, on.has(id)])
  ) as StatusSegmentVisibility;
}

const subscribers = new Set<() => void>();

export function subscribeStatusLine(cb: () => void): () => void {
  subscribers.add(cb);
  return () => subscribers.delete(cb);
}

/**
 * Drop the cache so the next read hits the file.
 *
 * Does not notify, so a mounted bar keeps its current map until something else
 * re-renders it. That is enough for a fixture written before mount, which is the
 * only caller; a live external edit would need a watcher this module does not have.
 */
export function invalidateStatusSegments(): void {
  resolved.clear();
}

function notifyChanged(): void {
  resolved.clear();
  for (const cb of subscribers) {
    try {
      cb();
    } catch (err) {
      logger.warn('[status-line] subscriber threw', err);
    }
  }
}

/**
 * Overlay a persisted value onto the defaults.
 *
 * Only boolean entries under a known id are honoured. An unknown id is dropped
 * rather than rejecting the whole object, so a file written by a newer build
 * degrades to the segments this build understands instead of reverting wholesale.
 */
function applyOverrides(
  raw: unknown,
  surface: UiMode
): StatusSegmentVisibility {
  const merged = defaultStatusSegments(surface);
  if (raw === undefined || raw === null) return merged;
  if (typeof raw !== 'object' || Array.isArray(raw)) {
    logger.warn(
      `[status-line] ignoring non-object ${surface} config: ${JSON.stringify(raw)}`
    );
    return merged;
  }
  const unknown: string[] = [];
  for (const [key, value] of Object.entries(raw as Record<string, unknown>)) {
    if (!isStatusSegmentId(key)) {
      unknown.push(key);
      continue;
    }
    if (typeof value === 'boolean') merged[key] = value;
  }
  if (unknown.length > 0) {
    logger.warn(
      `[status-line] ignoring unknown ${surface} segments: ${unknown.join(', ')}`
    );
  }
  return merged;
}

/** Resolved visibility for `surface`, defaults overlaid with the saved config. */
/**
 * The resolved map, cached until something changes it.
 *
 * `useSyncExternalStore` compares snapshots by identity, so returning a freshly
 * built object on every call would make it re-render forever. Caching also keeps
 * the file read off the render path.
 */
const resolved = new Map<UiMode, StatusSegmentVisibility>();

export function getStatusSegments(surface: UiMode): StatusSegmentVisibility {
  const cached = resolved.get(surface);
  if (cached) return cached;
  let next: StatusSegmentVisibility;
  try {
    next = applyOverrides(readCliSettings()[SETTING_KEY[surface]], surface);
  } catch (err) {
    logger.warn('[status-line] failed to read config, using defaults', err);
    next = defaultStatusSegments(surface);
  }
  resolved.set(surface, next);
  return next;
}

/** Whether a visible segment needs the account's billing figures fetched. */
export function statusSegmentsNeedBilling(
  visibility: StatusSegmentVisibility
): boolean {
  return BILLING_SEGMENT_IDS.some((id) => visibility[id]);
}

/** Whether a visible segment reads a clock, so the surface has to tick. */
export function statusSegmentsNeedClock(
  visibility: StatusSegmentVisibility
): boolean {
  return CLOCK_SEGMENT_IDS.some((id) => visibility[id]);
}

/**
 * Persist one segment's visibility. Only the changed key is written, so a config
 * file stays a small diff from the defaults rather than a full snapshot that
 * would freeze this build's defaults into the user's settings.
 */
/** A stored map may only hold known ids with boolean values. */
function sanitizeOverrides(existing: unknown): Record<string, boolean> {
  const base: Record<string, boolean> = {};
  if (!existing || typeof existing !== 'object' || Array.isArray(existing)) {
    return base;
  }
  for (const [key, value] of Object.entries(existing)) {
    if (isStatusSegmentId(key) && typeof value === 'boolean') base[key] = value;
  }
  return base;
}

export async function setStatusSegmentVisible(
  surface: UiMode,
  id: StatusSegmentId,
  visible: boolean
): Promise<boolean> {
  try {
    // Read and write inside one queued step: two writes in the same task would
    // otherwise both start from the same snapshot and one would be lost.
    await updateCliSettingWith(SETTING_KEY[surface], (existing) => {
      const base = sanitizeOverrides(existing);
      if (visible === defaultStatusSegments(surface)[id]) delete base[id];
      else base[id] = visible;
      return base;
    });
    notifyChanged();
    return true;
  } catch (err) {
    logger.error('[status-line] failed to save config', err);
    return false;
  }
}

/**
 * Flip one segment.
 *
 * The flip happens against the value inside the queued step rather than one read
 * beforehand, so two quick toggles cannot both start from the same snapshot.
 */
export async function toggleStatusSegment(
  surface: UiMode,
  id: StatusSegmentId
): Promise<boolean> {
  const defaults = defaultStatusSegments(surface);
  try {
    await updateCliSettingWith(SETTING_KEY[surface], (existing) => {
      const base = sanitizeOverrides(existing);
      const current = base[id] ?? defaults[id];
      if (!current === defaults[id]) delete base[id];
      else base[id] = !current;
      return base;
    });
    notifyChanged();
    return true;
  } catch (err) {
    logger.error('[status-line] failed to save config', err);
    return false;
  }
}

/** Drop every override so `surface` falls back to the built-in set. */
export async function resetStatusSegments(surface: UiMode): Promise<boolean> {
  try {
    await updateCliSettingWith(SETTING_KEY[surface], () => ({}));
    notifyChanged();
    return true;
  } catch (err) {
    logger.error('[status-line] failed to reset config', err);
    return false;
  }
}

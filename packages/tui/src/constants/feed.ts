/**
 * Extensible feed system for the TUI. Reads release data from feed.json
 * (via KIRO_FEED_FILE path) and converts the latest releases into
 * announcement entries for the greeting screen.
 */

import { readFileSync } from 'fs';

import { logger } from '../utils/logger.js';

export enum FeedEntryType {
  Announcement = 'announcement',
}

export interface BaseFeedEntry {
  type: FeedEntryType;
  id: string;
  date: string;
  version: string;
}

export interface AnnouncementEntry extends BaseFeedEntry {
  type: FeedEntryType.Announcement;
  maxShowCount: number;
  priority: number;
  maxLines: number;
}

// Discriminated union — grows as new entry types are added
export type FeedEntry = AnnouncementEntry;

/** Structured release notes for a single version, rendered as markdown. */
export interface ReleaseNotes {
  version: string;
  date: string;
  /** Full markdown body with all change types grouped. */
  content: string;
}

/** Shape of a single change in the Rust feed.json */
interface RustFeedChange {
  type: string;
  description: string;
}

/** Shape of a release entry in the Rust feed.json */
interface RustFeedRelease {
  type: string;
  date: string;
  version: string;
  title?: string;
  hidden?: boolean;
  changes?: RustFeedChange[];
}

/** Shape of the Rust feed.json */
interface RustFeed {
  entries: RustFeedRelease[];
}

const TYPE_LABELS: Record<string, string> = {
  added: 'Added',
  fixed: 'Fixed',
  changed: 'Changed',
  security: 'Security',
  deprecated: 'Deprecated',
  removed: 'Removed',
};

/** Stable display order for change types. */
const TYPE_ORDER = ['added', 'changed', 'fixed', 'security', 'deprecated', 'removed'];

/** Default Unicode icons for each change type. */
export const UNICODE_ICONS: Record<string, string> = {
  added: '✦',
  changed: '~',
  fixed: '✓',
  security: '⛨',
  deprecated: '▽',
  removed: '✗',
};

/** ASCII-safe fallback icons. */
export const ASCII_ICONS: Record<string, string> = {
  added: '-',
  changed: '-',
  fixed: '-',
  security: '-',
  deprecated: '-',
  removed: '-',
};

/** Verb patterns to strip from descriptions (hoisted for performance). */
const VERB_PATTERNS: Record<string, RegExp> = {
  added: /^add(?:ed)?\b[:\s]*/i,
  fixed: /^fix(?:ed)?\b[:\s]*/i,
  changed: /^change(?:d)?\b[:\s]*/i,
  deprecated: /^deprecate(?:d)?\b[:\s]*/i,
  removed: /^remove(?:d)?\b[:\s]*/i,
};

export interface RenderOptions {
  /** Icon map for bullet prefixes. Defaults to UNICODE_ICONS. */
  icons?: Record<string, string>;
  /** If provided, only these change types are included. */
  onlyTypes?: string[];
}

/**
 * Convert a Rust feed release entry into grouped markdown.
 */
export function releaseToContent(
  entry: RustFeedRelease,
  options?: RenderOptions
): string {
  const icons = options?.icons ?? UNICODE_ICONS;
  const lines: string[] = [`**✨ What's new in ${entry.version}**`];
  const changes = entry.changes ?? [];

  // Group by type
  const groups = new Map<string, string[]>();
  for (const change of changes) {
    if (options?.onlyTypes && !options.onlyTypes.includes(change.type))
      continue;
    let desc = change.description.replace(/ - \[#\d+\]\([^)]+\)/, '');
    const pattern = VERB_PATTERNS[change.type];
    if (pattern && pattern.test(desc)) {
      desc = desc.replace(pattern, '');
    }
    const list = groups.get(change.type) ?? [];
    list.push(desc);
    groups.set(change.type, list);
  }

  for (const type of TYPE_ORDER) {
    const items = groups.get(type);
    if (!items) continue;
    const label = TYPE_LABELS[type] ?? type;
    const icon = icons[type] ?? '-';
    lines.push('', `**${label}**`);
    for (const item of items) {
      lines.push(`${icon} ${item}`);
    }
  }

  return lines.join('\n');
}

/**
 * Read raw feed JSON from file path (KIRO_FEED_FILE).
 */
function readFeedRaw(): string | undefined {
  const filePath = process.env.KIRO_FEED_FILE;
  if (!filePath) return undefined;
  try {
    return readFileSync(filePath, 'utf-8');
  } catch (err) {
    logger.warn('[feed] Failed to read KIRO_FEED_FILE:', err);
    return undefined;
  }
}

/**
 * Parse feed data and return non-hidden releases with changes.
 */
function parseReleases(): RustFeedRelease[] {
  const raw = readFeedRaw();
  if (!raw) return [];

  try {
    const feed: RustFeed = JSON.parse(raw);
    return feed.entries.filter(
      (e) => e.type === 'release' && !e.hidden && (e.changes?.length ?? 0) > 0
    );
  } catch (err) {
    logger.warn('[feed] Failed to parse feed data:', err);
    return [];
  }
}

/**
 * Return the most recent N releases as markdown-rendered notes (newest first).
 * Pass `options.icons` to control bullet icons (e.g. ASCII_ICONS for accessibility).
 */
export function getRecentReleases(
  limit?: number,
  options?: RenderOptions
): ReleaseNotes[] {
  const releases = parseReleases();
  const sliced = limit != null ? releases.slice(0, limit) : releases;
  return sliced.map((r) => ({
    version: r.version,
    date: r.date,
    content: releaseToContent(r, options),
  }));
}

/**
 * Get the content for the announcement (latest release).
 * Called at render time with current icon set.
 */
export function getAnnouncementContent(options?: RenderOptions): string | null {
  const releases = parseReleases();
  if (releases.length === 0) return null;
  return releaseToContent(releases[0]!, options);
}

/**
 * Parse feed and return announcement metadata (without pre-rendered content).
 */
function parseFeedFromEnv(): FeedEntry[] {
  const releases = parseReleases();
  if (releases.length === 0) return [];

  const latest = releases[0]!;
  return [
    {
      type: FeedEntryType.Announcement,
      id: `release-${latest.version}`,
      date: latest.date,
      version: latest.version,
      maxShowCount: 3,
      priority: 1,
      maxLines: 8,
    },
  ];
}

export const FEED_ENTRIES: FeedEntry[] = parseFeedFromEnv();

export function getAnnouncements(): AnnouncementEntry[] {
  return FEED_ENTRIES.filter(
    (e): e is AnnouncementEntry => e.type === FeedEntryType.Announcement
  );
}

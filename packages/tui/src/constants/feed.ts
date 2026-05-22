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
  content: string;
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
};

/** Stable display order for change types. */
const TYPE_ORDER = ['added', 'changed', 'fixed', 'security', 'deprecated'];

/**
 * Convert a Rust feed release entry into grouped markdown.
 * If `onlyTypes` is provided, only those groups are included.
 */
function releaseToContent(
  entry: RustFeedRelease,
  options?: { onlyTypes?: string[] }
): string {
  const lines: string[] = [`**✨ What's new in ${entry.version}**`];
  const changes = entry.changes ?? [];

  // Group by type
  const groups = new Map<string, string[]>();
  for (const change of changes) {
    if (options?.onlyTypes && !options.onlyTypes.includes(change.type))
      continue;
    const desc = change.description.replace(/ - \[#\d+\]\([^)]+\)/, '');
    const list = groups.get(change.type) ?? [];
    list.push(desc);
    groups.set(change.type, list);
  }

  for (const type of TYPE_ORDER) {
    const items = groups.get(type);
    if (!items) continue;
    const label = TYPE_LABELS[type] ?? type;
    lines.push('', `**${label}**`);
    for (const item of items) {
      lines.push(`- ${item}`);
    }
  }

  return lines.join('\n');
}

/**
 * Read raw feed JSON from file path (KIRO_FEED_FILE).
 * In production the Rust launcher writes feed.json to the data directory.
 * In dev/test mode, Knight Rider sets KIRO_FEED_FILE to the repo's feed.json.
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
 * Returns [] on missing/invalid data. Shared by announcement + /changelog.
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
 * No limit = all. Matches V1 `/changelog` when called with limit=2.
 */
export function getRecentReleases(limit?: number): ReleaseNotes[] {
  const releases = parseReleases();
  const sliced = limit != null ? releases.slice(0, limit) : releases;
  return sliced.map((r) => ({
    version: r.version,
    date: r.date,
    content: releaseToContent(r),
  }));
}

/**
 * Parse KIRO_FEED_JSON and convert the latest visible releases into
 * announcement entries. Returns at most 1 entry (the latest release).
 */
function parseFeedFromEnv(): FeedEntry[] {
  const releases = parseReleases();
  if (releases.length === 0) return [];

  // Take the latest release (first non-hidden with changes)
  const latest = releases[0]!;
  return [
    {
      type: FeedEntryType.Announcement,
      id: `release-${latest.version}`,
      date: latest.date,
      version: latest.version,
      content: releaseToContent(latest),
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

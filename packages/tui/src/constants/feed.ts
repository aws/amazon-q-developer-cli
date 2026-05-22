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
  /** Markdown body: `## What's new in X.Y.Z` + bullet list of changes. */
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

/**
 * Convert a Rust feed release entry into a markdown-formatted announcement.
 */
function releaseToContent(entry: RustFeedRelease): string {
  const lines: string[] = [`## What's new in ${entry.version}`];
  const changes = entry.changes ?? [];
  const sorted = [...changes].sort((a, b) => a.type.localeCompare(b.type));
  for (const change of sorted) {
    const label = TYPE_LABELS[change.type] ?? change.type;
    // Strip PR links like " - [#123](url)"
    const desc = change.description.replace(/ - \[#\d+\]\([^)]+\)/, '');
    lines.push(`- **${label}**: ${desc}`);
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

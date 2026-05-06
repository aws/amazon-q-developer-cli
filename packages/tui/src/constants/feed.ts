/**
 * Extensible feed system for the TUI. Reads release data from the Rust-side
 * feed.json (passed via KIRO_FEED_JSON env var) and converts the latest
 * releases into announcement entries for the greeting screen.
 */

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
 * Parse KIRO_FEED_JSON and convert the latest visible releases into
 * announcement entries. Returns at most 1 entry (the latest release).
 */
function parseFeedFromEnv(): FeedEntry[] {
  const raw = process.env.KIRO_FEED_JSON;
  if (!raw) return [];

  try {
    const feed: RustFeed = JSON.parse(raw);
    const releases = feed.entries.filter(
      (e) => e.type === 'release' && !e.hidden && (e.changes?.length ?? 0) > 0
    );
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
  } catch (err) {
    logger.warn('[feed] Failed to parse KIRO_FEED_JSON:', err);
    return [];
  }
}

export const FEED_ENTRIES: FeedEntry[] = parseFeedFromEnv();

export function getAnnouncements(): AnnouncementEntry[] {
  return FEED_ENTRIES.filter(
    (e): e is AnnouncementEntry => e.type === FeedEntryType.Announcement
  );
}

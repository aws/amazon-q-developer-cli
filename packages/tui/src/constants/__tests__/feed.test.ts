import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { writeFileSync, unlinkSync, mkdtempSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { FeedEntryType, type AnnouncementEntry } from '../../constants/feed.js';

let originalFeedFile: string | undefined;
let tmpDir: string;
let feedFile: string;

beforeEach(() => {
  originalFeedFile = process.env.KIRO_FEED_FILE;
  tmpDir = mkdtempSync(join(tmpdir(), 'feed-test-'));
  feedFile = join(tmpDir, 'feed.json');
});

afterEach(() => {
  if (originalFeedFile !== undefined) {
    process.env.KIRO_FEED_FILE = originalFeedFile;
  } else {
    delete process.env.KIRO_FEED_FILE;
  }
  try {
    unlinkSync(feedFile);
  } catch {
    // file may not exist
  }
});

/** Helper: write feed JSON to a temp file and re-import feed.ts */
async function loadFeed(json?: string) {
  if (json !== undefined) {
    writeFileSync(feedFile, json);
    process.env.KIRO_FEED_FILE = feedFile;
  } else {
    delete process.env.KIRO_FEED_FILE;
  }
  const cacheBuster = `?t=${Date.now()}-${Math.random()}`;
  const mod = await import(`../../constants/feed.js${cacheBuster}`);
  return mod;
}

const SAMPLE_FEED = JSON.stringify({
  entries: [
    {
      type: 'release',
      date: '2026-04-27',
      version: '2.2.0',
      title: 'Version 2.2.0',
      changes: [
        { type: 'added', description: 'Support adaptive thinking' },
        { type: 'fixed', description: 'Fix API key auth' },
      ],
    },
    {
      type: 'release',
      date: '2026-04-21',
      version: '2.1.0',
      title: 'Version 2.1.0',
      changes: [{ type: 'added', description: 'Device flow login' }],
    },
  ],
});

describe('feed', () => {
  it('returns empty entries when KIRO_FEED_FILE is not set', async () => {
    const { FEED_ENTRIES, getAnnouncements } = await loadFeed(undefined);
    expect(FEED_ENTRIES).toEqual([]);
    expect(getAnnouncements()).toEqual([]);
  });

  it('returns empty entries when feed file is invalid JSON', async () => {
    const { FEED_ENTRIES } = await loadFeed('not json!!!');
    expect(FEED_ENTRIES).toEqual([]);
  });

  it('parses latest release into an announcement entry', async () => {
    const { FEED_ENTRIES, getAnnouncements } = await loadFeed(SAMPLE_FEED);
    expect(FEED_ENTRIES).toHaveLength(1);

    const entry = FEED_ENTRIES[0] as AnnouncementEntry;
    expect(entry.type).toBe(FeedEntryType.Announcement);
    expect(entry.id).toBe('release-2.2.0');
    expect(entry.version).toBe('2.2.0');
    expect(entry.date).toBe('2026-04-27');
    expect(entry.maxShowCount).toBe(3);
    expect(entry.priority).toBe(1);
    expect(entry.content).toContain("What's new in 2.2.0");
    expect(entry.content).toContain('Support adaptive thinking');
    expect(entry.content).toContain('Fix API key auth');

    expect(getAnnouncements()).toHaveLength(1);
  });

  it('skips hidden releases', async () => {
    const feed = JSON.stringify({
      entries: [
        {
          type: 'release',
          date: '2999-01-01',
          version: '0.0.0',
          hidden: true,
          changes: [],
        },
        {
          type: 'release',
          date: '2026-04-27',
          version: '2.2.0',
          changes: [{ type: 'added', description: 'Feature' }],
        },
      ],
    });
    const { FEED_ENTRIES } = await loadFeed(feed);
    expect(FEED_ENTRIES).toHaveLength(1);
    expect(FEED_ENTRIES[0]!.version).toBe('2.2.0');
  });

  it('skips releases with no changes', async () => {
    const feed = JSON.stringify({
      entries: [
        { type: 'release', date: '2026-04-27', version: '2.2.0', changes: [] },
        {
          type: 'release',
          date: '2026-04-21',
          version: '2.1.0',
          changes: [{ type: 'added', description: 'Something' }],
        },
      ],
    });
    const { FEED_ENTRIES } = await loadFeed(feed);
    expect(FEED_ENTRIES).toHaveLength(1);
    expect(FEED_ENTRIES[0]!.version).toBe('2.1.0');
  });

  it('strips PR links from descriptions', async () => {
    const feed = JSON.stringify({
      entries: [
        {
          type: 'release',
          date: '2026-01-01',
          version: '1.0.0',
          changes: [
            {
              type: 'added',
              description:
                'A feature - [#123](https://github.com/aws/repo/pull/123)',
            },
          ],
        },
      ],
    });
    const { FEED_ENTRIES } = await loadFeed(feed);
    expect(FEED_ENTRIES[0]!.content).toContain('A feature');
    expect(FEED_ENTRIES[0]!.content).not.toContain('#123');
  });

  it('all entries have valid FeedEntryType enum values', async () => {
    const { FEED_ENTRIES } = await loadFeed(SAMPLE_FEED);
    const validTypes = Object.values(FeedEntryType);
    for (const entry of FEED_ENTRIES) {
      expect(validTypes).toContain(entry.type);
    }
  });

  it('all entries have non-empty id', async () => {
    const { FEED_ENTRIES } = await loadFeed(SAMPLE_FEED);
    for (const entry of FEED_ENTRIES) {
      expect(entry.id.length).toBeGreaterThan(0);
    }
  });

  it('announcement entries have positive maxShowCount', async () => {
    const { getAnnouncements } = await loadFeed(SAMPLE_FEED);
    for (const a of getAnnouncements()) {
      expect(a.maxShowCount).toBeGreaterThan(0);
    }
  });

  it('announcement entries have non-empty content', async () => {
    const { getAnnouncements } = await loadFeed(SAMPLE_FEED);
    for (const a of getAnnouncements()) {
      expect(a.content.length).toBeGreaterThan(0);
    }
  });
});

describe('getRecentReleases', () => {
  it('returns empty array when KIRO_FEED_FILE is not set', async () => {
    const { getRecentReleases } = await loadFeed(undefined);
    expect(getRecentReleases()).toEqual([]);
    expect(getRecentReleases(2)).toEqual([]);
  });

  it('returns empty array when feed file is invalid JSON', async () => {
    const { getRecentReleases } = await loadFeed('not json!!!');
    expect(getRecentReleases(2)).toEqual([]);
  });

  it('returns all visible releases when no limit is given', async () => {
    const { getRecentReleases } = await loadFeed(SAMPLE_FEED);
    const releases = getRecentReleases();
    expect(releases).toHaveLength(2);
    expect(releases[0].version).toBe('2.2.0');
    expect(releases[1].version).toBe('2.1.0');
  });

  it('truncates to the requested limit (matches V1 take(2))', async () => {
    const feed = JSON.stringify({
      entries: [
        {
          type: 'release',
          date: '2026-04-27',
          version: '2.2.0',
          changes: [{ type: 'added', description: 'A' }],
        },
        {
          type: 'release',
          date: '2026-04-21',
          version: '2.1.0',
          changes: [{ type: 'fixed', description: 'B' }],
        },
        {
          type: 'release',
          date: '2026-04-01',
          version: '2.0.0',
          changes: [{ type: 'added', description: 'C' }],
        },
      ],
    });
    const { getRecentReleases } = await loadFeed(feed);
    const releases = getRecentReleases(2);
    expect(releases).toHaveLength(2);
    expect(releases.map((r: { version: string }) => r.version)).toEqual([
      '2.2.0',
      '2.1.0',
    ]);
  });

  it("renders markdown content with What's new header and sorted change bullets", async () => {
    const { getRecentReleases } = await loadFeed(SAMPLE_FEED);
    const [latest] = getRecentReleases(1);
    expect(latest.version).toBe('2.2.0');
    expect(latest.date).toBe('2026-04-27');
    expect(latest.content).toContain("**✨ What's new in 2.2.0**");
    expect(latest.content).toContain('- Support adaptive thinking');
    expect(latest.content).toContain('- Fix API key auth');
    // Change groups are ordered: Added before Fixed
    const addedIdx = latest.content.indexOf('**Added**');
    const fixedIdx = latest.content.indexOf('**Fixed**');
    expect(addedIdx).toBeGreaterThan(0);
    expect(fixedIdx).toBeGreaterThan(addedIdx);
  });

  it('skips hidden releases and releases with no changes', async () => {
    const feed = JSON.stringify({
      entries: [
        {
          type: 'release',
          date: '2999-01-01',
          version: '0.0.0',
          hidden: true,
          changes: [{ type: 'added', description: 'x' }],
        },
        {
          type: 'release',
          date: '2026-04-27',
          version: '2.2.0',
          changes: [],
        },
        {
          type: 'release',
          date: '2026-04-21',
          version: '2.1.0',
          changes: [{ type: 'added', description: 'Real change' }],
        },
      ],
    });
    const { getRecentReleases } = await loadFeed(feed);
    const releases = getRecentReleases();
    expect(releases).toHaveLength(1);
    expect(releases[0].version).toBe('2.1.0');
  });

  it('strips PR links from change descriptions', async () => {
    const feed = JSON.stringify({
      entries: [
        {
          type: 'release',
          date: '2026-01-01',
          version: '1.0.0',
          changes: [
            {
              type: 'added',
              description:
                'A feature - [#123](https://github.com/aws/repo/pull/123)',
            },
          ],
        },
      ],
    });
    const { getRecentReleases } = await loadFeed(feed);
    const [entry] = getRecentReleases();
    expect(entry.content).toContain('A feature');
    expect(entry.content).not.toContain('#123');
  });
});

describe('file read errors', () => {
  it('returns empty when KIRO_FEED_FILE points to nonexistent file', async () => {
    process.env.KIRO_FEED_FILE = '/nonexistent/feed.json';
    const cacheBuster = `?t=${Date.now()}-${Math.random()}`;
    const { FEED_ENTRIES } = await import(
      `../../constants/feed.js${cacheBuster}`
    );
    expect(FEED_ENTRIES).toEqual([]);
  });
});

import { describe, it, expect } from 'bun:test';
import {
  FeedEntryType,
  FEED_ENTRIES,
  getAnnouncements,
  type AnnouncementEntry,
} from '../../constants/feed.js';

describe('feed', () => {
  it('all entries have valid FeedEntryType enum values', () => {
    const validTypes = Object.values(FeedEntryType);
    for (const entry of FEED_ENTRIES) {
      expect(validTypes).toContain(entry.type);
    }
  });

  it('all entries have non-empty id', () => {
    for (const entry of FEED_ENTRIES) {
      expect(entry.id.length).toBeGreaterThan(0);
    }
  });

  it('no duplicate IDs in FEED_ENTRIES', () => {
    const ids = FEED_ENTRIES.map((e) => e.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('getAnnouncements returns only Announcement type entries', () => {
    const announcements = getAnnouncements();
    for (const a of announcements) {
      expect(a.type).toBe(FeedEntryType.Announcement);
    }
  });

  it('announcement entries have non-empty content', () => {
    const announcements = getAnnouncements();
    for (const a of announcements) {
      expect(a.content.length).toBeGreaterThan(0);
    }
  });

  it('announcement entries have positive maxShowCount', () => {
    const announcements = getAnnouncements();
    for (const a of announcements) {
      expect(a.maxShowCount).toBeGreaterThan(0);
    }
  });

  it('type narrowing works via discriminated union', () => {
    for (const entry of FEED_ENTRIES) {
      switch (entry.type) {
        case FeedEntryType.Announcement: {
          // TypeScript narrows to AnnouncementEntry here
          const a: AnnouncementEntry = entry;
          expect(typeof a.content).toBe('string');
          expect(typeof a.maxShowCount).toBe('number');
          expect(typeof a.priority).toBe('number');
          expect(typeof a.maxLines).toBe('number');
          break;
        }
      }
    }
  });
});

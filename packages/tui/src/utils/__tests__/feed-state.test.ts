import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { mkdirSync, writeFileSync, rmSync, existsSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { FeedEntryType, type AnnouncementEntry } from '../../constants/feed.js';
import {
  getShowCounts,
  incrementShowCount,
  getActiveAnnouncement,
} from '../feed-state.js';

let testDir: string;
let originalHome: string | undefined;

function stateFilePath(): string {
  return join(testDir, '.kiro', 'settings', 'feed_state.json');
}

function writeState(data: Record<string, number>): void {
  const dir = join(testDir, '.kiro', 'settings');
  mkdirSync(dir, { recursive: true });
  writeFileSync(stateFilePath(), JSON.stringify(data), 'utf-8');
}

function makeAnnouncement(
  overrides: Partial<AnnouncementEntry> & { id: string }
): AnnouncementEntry {
  return {
    type: FeedEntryType.Announcement,
    date: '2026-04-09',
    version: '1.29.6',
    content: 'Test message',
    maxShowCount: 3,
    priority: 1,
    maxLines: 1,
    ...overrides,
  };
}

beforeEach(() => {
  testDir = join(
    tmpdir(),
    `feed-state-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
  mkdirSync(testDir, { recursive: true });
  originalHome = process.env.HOME;
  process.env.HOME = testDir;
});

afterEach(() => {
  process.env.HOME = originalHome;
  try {
    rmSync(testDir, { recursive: true, force: true });
  } catch {
    /* ignore */
  }
});

describe('feed-state', () => {
  describe('getShowCounts', () => {
    it('returns empty object when state file does not exist', () => {
      expect(getShowCounts()).toEqual({});
    });

    it('returns persisted counts', () => {
      writeState({ 'msg-1': 2, 'msg-2': 1 });
      expect(getShowCounts()).toEqual({ 'msg-1': 2, 'msg-2': 1 });
    });

    it('handles corrupt state file gracefully', () => {
      const dir = join(testDir, '.kiro', 'settings');
      mkdirSync(dir, { recursive: true });
      writeFileSync(stateFilePath(), 'not json!!!', 'utf-8');
      expect(getShowCounts()).toEqual({});
    });

    it('handles array state file gracefully', () => {
      const dir = join(testDir, '.kiro', 'settings');
      mkdirSync(dir, { recursive: true });
      writeFileSync(stateFilePath(), '[1,2,3]', 'utf-8');
      expect(getShowCounts()).toEqual({});
    });
  });

  describe('incrementShowCount', () => {
    it('creates state file on first increment', () => {
      incrementShowCount('msg-1');
      expect(getShowCounts()).toEqual({ 'msg-1': 1 });
      expect(existsSync(stateFilePath())).toBe(true);
    });

    it('increments existing count', () => {
      writeState({ 'msg-1': 2 });
      incrementShowCount('msg-1');
      expect(getShowCounts()).toEqual({ 'msg-1': 3 });
    });

    it('preserves other entries when incrementing', () => {
      writeState({ 'msg-1': 1, 'msg-2': 5 });
      incrementShowCount('msg-1');
      expect(getShowCounts()).toEqual({ 'msg-1': 2, 'msg-2': 5 });
    });
  });

  describe('getActiveAnnouncement', () => {
    it('returns highest-priority message when none have been seen', () => {
      const msgs = [
        makeAnnouncement({ id: 'low', priority: 10 }),
        makeAnnouncement({ id: 'high', priority: 1 }),
      ];
      expect(getActiveAnnouncement(msgs)?.id).toBe('high');
    });

    it('returns null when all messages are exhausted', () => {
      writeState({ 'msg-1': 3, 'msg-2': 3 });
      const msgs = [
        makeAnnouncement({ id: 'msg-1', maxShowCount: 3 }),
        makeAnnouncement({ id: 'msg-2', maxShowCount: 3 }),
      ];
      expect(getActiveAnnouncement(msgs)).toBeNull();
    });

    it('skips exhausted messages and returns next priority', () => {
      writeState({ high: 3 });
      // Same version — priority tiebreak: high is exhausted, low wins
      const msgs = [
        makeAnnouncement({ id: 'low', priority: 1, maxShowCount: 3 }),
        makeAnnouncement({ id: 'high', priority: 10, maxShowCount: 3 }),
      ];
      // Only the newest (same version, lowest priority) is considered
      expect(getActiveAnnouncement(msgs)?.id).toBe('low');
    });

    it('message IDs not in feed are ignored (stale state from old versions)', () => {
      writeState({ 'old-removed-msg': 5 });
      const msgs = [makeAnnouncement({ id: 'new-msg' })];
      expect(getActiveAnnouncement(msgs)?.id).toBe('new-msg');
    });

    it('newer version entry supersedes exhausted older entry', () => {
      writeState({ 'old-msg': 10 });
      process.env.KIRO_VERSION_OVERRIDE = '2.0.0';
      const msgs = [
        makeAnnouncement({ id: 'old-msg', version: '1.0.0', maxShowCount: 3 }),
        makeAnnouncement({ id: 'new-msg', version: '2.0.0', priority: 1 }),
      ];
      expect(getActiveAnnouncement(msgs)?.id).toBe('new-msg');
      delete process.env.KIRO_VERSION_OVERRIDE;
    });

    it('same message ID is NOT re-shown after simulated upgrade', () => {
      writeState({ 'msg-1': 3 });
      // Same message still in feed after version bump — should NOT show
      const msgs = [makeAnnouncement({ id: 'msg-1', maxShowCount: 3 })];
      expect(getActiveAnnouncement(msgs)).toBeNull();
    });

    it('maxShowCount of 1 — shown exactly once then never again', () => {
      const msgs = [makeAnnouncement({ id: 'once', maxShowCount: 1 })];

      // First time: active
      expect(getActiveAnnouncement(msgs)?.id).toBe('once');
      incrementShowCount('once');

      // Second time: exhausted
      expect(getActiveAnnouncement(msgs)).toBeNull();
    });

    it('maxShowCount of 0 — never shown', () => {
      const msgs = [makeAnnouncement({ id: 'never', maxShowCount: 0 })];
      expect(getActiveAnnouncement(msgs)).toBeNull();
    });

    it('multiple messages with same priority — deterministic selection (first in array)', () => {
      const msgs = [
        makeAnnouncement({ id: 'first', priority: 1 }),
        makeAnnouncement({ id: 'second', priority: 1 }),
      ];
      expect(getActiveAnnouncement(msgs)?.id).toBe('first');
    });

    it('returns null for empty message list', () => {
      expect(getActiveAnnouncement([])).toBeNull();
    });

    it('full lifecycle: show 3 times then stop', () => {
      const msgs = [makeAnnouncement({ id: 'lifecycle', maxShowCount: 3 })];

      for (let i = 0; i < 3; i++) {
        expect(getActiveAnnouncement(msgs)?.id).toBe('lifecycle');
        incrementShowCount('lifecycle');
      }
      expect(getActiveAnnouncement(msgs)).toBeNull();
    });

    it('upgrade scenario: old exhausted, new version appears', () => {
      // v1.29.6 ships with one message
      const v1msgs = [
        makeAnnouncement({
          id: 'v1-welcome',
          version: '1.29.6',
          maxShowCount: 2,
        }),
      ];
      incrementShowCount('v1-welcome');
      incrementShowCount('v1-welcome');
      expect(getActiveAnnouncement(v1msgs)).toBeNull();

      // v2.0.0 ships with new message — old one is superseded
      process.env.KIRO_VERSION_OVERRIDE = '2.0.0';
      const v2msgs = [
        makeAnnouncement({
          id: 'v1-welcome',
          version: '1.29.6',
          maxShowCount: 2,
        }),
        makeAnnouncement({
          id: 'v2-welcome',
          version: '2.0.0',
          priority: 1,
          maxShowCount: 3,
        }),
      ];
      expect(getActiveAnnouncement(v2msgs)?.id).toBe('v2-welcome');
      delete process.env.KIRO_VERSION_OVERRIDE;
    });

    it('upgrade scenario: same message ID persists across versions, not re-shown', () => {
      // User sees message 3 times on v1.29.6
      const msgs = [makeAnnouncement({ id: 'persistent', maxShowCount: 3 })];
      for (let i = 0; i < 3; i++) {
        incrementShowCount('persistent');
      }

      // User upgrades to v1.30.0 — same message still in feed
      // Should NOT re-show
      expect(getActiveAnnouncement(msgs)).toBeNull();
    });

    it('does not show announcement for a future version', () => {
      process.env.KIRO_VERSION_OVERRIDE = '1.29.6';
      const msgs = [makeAnnouncement({ id: 'future', version: '2.0.0' })];
      expect(getActiveAnnouncement(msgs)).toBeNull();
      delete process.env.KIRO_VERSION_OVERRIDE;
    });

    it('shows announcement when CLI version matches entry version', () => {
      process.env.KIRO_VERSION_OVERRIDE = '2.0.0';
      const msgs = [makeAnnouncement({ id: 'match', version: '2.0.0' })];
      expect(getActiveAnnouncement(msgs)?.id).toBe('match');
      delete process.env.KIRO_VERSION_OVERRIDE;
    });

    it('shows announcement when CLI version is newer than entry version', () => {
      process.env.KIRO_VERSION_OVERRIDE = '2.1.0';
      const msgs = [makeAnnouncement({ id: 'older', version: '2.0.0' })];
      expect(getActiveAnnouncement(msgs)?.id).toBe('older');
      delete process.env.KIRO_VERSION_OVERRIDE;
    });

    it('newer entry supersedes older even if older is eligible', () => {
      process.env.KIRO_VERSION_OVERRIDE = '1.29.6';
      const msgs = [
        makeAnnouncement({ id: 'current', version: '1.29.6', priority: 2 }),
        makeAnnouncement({ id: 'future', version: '2.0.0', priority: 1 }),
      ];
      // future supersedes current — but future isn't eligible on 1.29.6, so null
      expect(getActiveAnnouncement(msgs)).toBeNull();
      delete process.env.KIRO_VERSION_OVERRIDE;
    });

    it('version wildcard 2.X.X matches any 2.x.x', () => {
      process.env.KIRO_VERSION_OVERRIDE = '2.3.7';
      const msgs = [makeAnnouncement({ id: 'wild', version: '2.X.X' })];
      expect(getActiveAnnouncement(msgs)?.id).toBe('wild');
      delete process.env.KIRO_VERSION_OVERRIDE;
    });

    it('version wildcard 2.0.X matches any 2.0.x', () => {
      process.env.KIRO_VERSION_OVERRIDE = '2.0.5';
      const msgs = [makeAnnouncement({ id: 'wild', version: '2.0.X' })];
      expect(getActiveAnnouncement(msgs)?.id).toBe('wild');
      delete process.env.KIRO_VERSION_OVERRIDE;
    });

    it('version wildcard 2.0.X does not match 1.x.x', () => {
      process.env.KIRO_VERSION_OVERRIDE = '1.29.6';
      const msgs = [makeAnnouncement({ id: 'wild', version: '2.0.X' })];
      expect(getActiveAnnouncement(msgs)).toBeNull();
      delete process.env.KIRO_VERSION_OVERRIDE;
    });

    it('version wildcard 2.X.X does not match 1.x.x', () => {
      process.env.KIRO_VERSION_OVERRIDE = '1.99.99';
      const msgs = [makeAnnouncement({ id: 'wild', version: '2.X.X' })];
      expect(getActiveAnnouncement(msgs)).toBeNull();
      delete process.env.KIRO_VERSION_OVERRIDE;
    });

    it('version wildcard lowercase x also works', () => {
      process.env.KIRO_VERSION_OVERRIDE = '2.1.0';
      const msgs = [makeAnnouncement({ id: 'wild', version: '2.x.x' })];
      expect(getActiveAnnouncement(msgs)?.id).toBe('wild');
      delete process.env.KIRO_VERSION_OVERRIDE;
    });

    it('newer version entry takes priority over older non-exhausted one', () => {
      process.env.KIRO_VERSION_OVERRIDE = '2.1.0';
      const msgs = [
        makeAnnouncement({ id: 'old', version: '1.29.6', priority: 1 }),
        makeAnnouncement({ id: 'new', version: '2.0.0', priority: 10 }),
      ];
      // old has better priority (1) but new has newer version — new wins
      expect(getActiveAnnouncement(msgs)?.id).toBe('new');
      delete process.env.KIRO_VERSION_OVERRIDE;
    });

    it('same version falls back to priority', () => {
      process.env.KIRO_VERSION_OVERRIDE = '2.0.0';
      const msgs = [
        makeAnnouncement({ id: 'low-pri', version: '2.0.0', priority: 10 }),
        makeAnnouncement({ id: 'high-pri', version: '2.0.0', priority: 1 }),
      ];
      expect(getActiveAnnouncement(msgs)?.id).toBe('high-pri');
      delete process.env.KIRO_VERSION_OVERRIDE;
    });

    it('malformed CLI version does not match anything', () => {
      process.env.KIRO_VERSION_OVERRIDE = '2.0.0-beta.1';
      const msgs = [makeAnnouncement({ id: 'any', version: '2.X.X' })];
      expect(getActiveAnnouncement(msgs)).toBeNull();
      delete process.env.KIRO_VERSION_OVERRIDE;
    });

    it('malformed entry version does not match', () => {
      process.env.KIRO_VERSION_OVERRIDE = '2.0.0';
      const msgs = [makeAnnouncement({ id: 'bad', version: 'abc.def.ghi' })];
      expect(getActiveAnnouncement(msgs)).toBeNull();
      delete process.env.KIRO_VERSION_OVERRIDE;
    });
  });
});

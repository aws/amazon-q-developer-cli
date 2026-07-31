/**
 * Unit tests for status-line segment visibility.
 *
 * The behaviours worth pinning are the ones a user would notice going wrong: an
 * absent or malformed config must leave the bar exactly as it is today, a config
 * from a newer build must not wipe the segments this build does understand, and a
 * toggle back to the default must stop persisting an override rather than freezing
 * this build's default into the user's file.
 */
import { describe, it, expect, beforeEach, afterAll } from 'bun:test';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

const tempHome = mkdtempSync(join(tmpdir(), 'kiro-status-line-test-'));
const originalHome = process.env.KIRO_HOME;
// Set per test rather than here: a module-scope assignment outlives this file
// and would point other suites at these fixtures.
mkdirSync(join(tempHome, 'settings'), { recursive: true });

const {
  getStatusSegments,
  defaultStatusSegments,
  setStatusSegmentVisible,
  toggleStatusSegment,
  resetStatusSegments,
  statusSegmentsNeedClock,
  subscribeStatusLine,
  invalidateStatusSegments,
} = await import('../config.js');
const { STATUS_SEGMENT_IDS, CLOCK_SEGMENT_IDS, BILLING_SEGMENT_IDS } =
  await import('../segments.js');
const { STATUS_SEGMENT_LABELS } = await import('../labels.js');

const settingsFile = join(tempHome, 'settings', 'cli.json');

function writeSettings(value: Record<string, unknown>): void {
  writeFileSync(settingsFile, JSON.stringify(value));
  invalidateStatusSegments();
}

function readSaved(key: string): unknown {
  return JSON.parse(
    require('fs').readFileSync(settingsFile, 'utf-8') as string
  )[key];
}

beforeEach(() => {
  process.env.KIRO_HOME = tempHome;
  writeSettings({});
});

afterAll(() => {
  if (originalHome === undefined) delete process.env.KIRO_HOME;
  else process.env.KIRO_HOME = originalHome;
  rmSync(tempHome, { recursive: true, force: true });
});

describe('segment id lists', () => {
  it('pins which segments read a clock or the billing figures', () => {
    // These lists sit alongside the registry rather than being derived from it,
    // so a new segment that needs a clock or a fetch would silently never get
    // one. Failing here forces that decision to be made.
    expect([...CLOCK_SEGMENT_IDS]).toEqual(['date', 'time']);
    expect([...BILLING_SEGMENT_IDS]).toEqual(['usage', 'credits']);
  });

  it('lists only known ids', () => {
    for (const id of [...CLOCK_SEGMENT_IDS, ...BILLING_SEGMENT_IDS]) {
      expect(STATUS_SEGMENT_IDS).toContain(id);
    }
  });
});

describe('status-line visibility', () => {
  describe('defaults', () => {
    it('shows what each surface painted before this was configurable', () => {
      const tui = defaultStatusSegments('tui');
      expect(STATUS_SEGMENT_IDS.filter((id) => tui[id])).toEqual([
        'agent',
        'autonomous',
        'model',
        'effort',
        'context',
        'tangent',
        'codeIntel',
        'location',
        'branch',
        'goal',
      ]);

      // Lite has never painted a code-intelligence indicator.
      const lite = defaultStatusSegments('lite');
      expect(lite.codeIntel).toBe(false);
      expect(STATUS_SEGMENT_IDS.filter((id) => lite[id])).toEqual([
        'agent',
        'autonomous',
        'model',
        'effort',
        'context',
        'tangent',
        'location',
        'branch',
        'goal',
      ]);
    });

    it('leaves every new segment off so an existing bar is unchanged', () => {
      for (const surface of ['tui', 'lite'] as const) {
        const resolved = defaultStatusSegments(surface);
        for (const id of ['date', 'time', 'usage', 'credits'] as const) {
          expect(resolved[id]).toBe(false);
        }
      }
    });

    it('falls back to defaults when the key is absent', () => {
      expect(getStatusSegments('tui')).toEqual(defaultStatusSegments('tui'));
    });

    it('falls back to defaults when the value is not an object', () => {
      writeSettings({ 'chat.statusLine.tui': ['agent', 'model'] });
      expect(getStatusSegments('tui')).toEqual(defaultStatusSegments('tui'));
    });
  });

  describe('overrides', () => {
    it('applies only the keys present, leaving the rest at their default', () => {
      writeSettings({
        'chat.statusLine.tui': { effort: false, time: true },
      });
      const resolved = getStatusSegments('tui');
      expect(resolved.effort).toBe(false);
      expect(resolved.time).toBe(true);
      // Untouched keys keep the default.
      expect(resolved.agent).toBe(true);
      expect(resolved.credits).toBe(false);
    });

    it('keeps known segments when the config also carries unknown ids', () => {
      writeSettings({
        'chat.statusLine.tui': { weather: true, usage: true, modle: false },
      });
      const resolved = getStatusSegments('tui');
      expect(resolved.usage).toBe(true);
      expect(resolved.agent).toBe(true);
      expect('weather' in resolved).toBe(false);
    });

    it('ignores non-boolean values rather than coercing them', () => {
      writeSettings({
        'chat.statusLine.tui': { time: 'yes', usage: 1, date: true },
      });
      const resolved = getStatusSegments('tui');
      expect(resolved.time).toBe(false);
      expect(resolved.usage).toBe(false);
      expect(resolved.date).toBe(true);
    });

    it('keeps the two surfaces independent', () => {
      writeSettings({
        'chat.statusLine.tui': { time: true },
        'chat.statusLine.lite': { usage: true },
      });
      expect(getStatusSegments('tui').time).toBe(true);
      expect(getStatusSegments('tui').usage).toBe(false);
      expect(getStatusSegments('lite').usage).toBe(true);
      expect(getStatusSegments('lite').time).toBe(false);
    });
  });

  describe('persistence', () => {
    it('writes only the segments that differ from the default', async () => {
      await setStatusSegmentVisible('tui', 'time', true);
      expect(readSaved('chat.statusLine.tui')).toEqual({ time: true });
    });

    it('drops the override when a segment returns to its default', async () => {
      await setStatusSegmentVisible('tui', 'time', true);
      await setStatusSegmentVisible('tui', 'time', false);
      expect(readSaved('chat.statusLine.tui')).toEqual({});
    });

    it('round-trips a toggle', async () => {
      expect(getStatusSegments('tui').usage).toBe(false);
      await toggleStatusSegment('tui', 'usage');
      expect(getStatusSegments('tui').usage).toBe(true);
      await toggleStatusSegment('tui', 'usage');
      expect(getStatusSegments('tui').usage).toBe(false);
    });

    it('strips unknown ids already in the file when writing', async () => {
      writeSettings({ 'chat.statusLine.tui': { weather: true } });
      await setStatusSegmentVisible('tui', 'time', true);
      expect(readSaved('chat.statusLine.tui')).toEqual({ time: true });
    });

    it('reset clears every override', async () => {
      await setStatusSegmentVisible('tui', 'time', true);
      await setStatusSegmentVisible('tui', 'agent', false);
      await resetStatusSegments('tui');
      expect(getStatusSegments('tui')).toEqual(defaultStatusSegments('tui'));
    });

    it('notifies subscribers on write and stops once unsubscribed', async () => {
      let calls = 0;
      const unsubscribe = subscribeStatusLine(() => {
        calls += 1;
      });
      await setStatusSegmentVisible('tui', 'date', true);
      expect(calls).toBe(1);
      // A subscriber sees the new value, not the one it was mounted with.
      expect(getStatusSegments('tui').date).toBe(true);

      unsubscribe();
      await setStatusSegmentVisible('tui', 'date', false);
      expect(calls).toBe(1);
    });
  });

  describe('clock gating', () => {
    it('only asks for a clock when a clock segment is visible', () => {
      const off = defaultStatusSegments('tui');
      expect(statusSegmentsNeedClock(off)).toBe(false);
      expect(statusSegmentsNeedClock({ ...off, date: true })).toBe(true);
      expect(statusSegmentsNeedClock({ ...off, time: true })).toBe(true);
      // A non-clock segment must not start a timer.
      expect(statusSegmentsNeedClock({ ...off, usage: true })).toBe(false);
    });
  });

  it('labels every segment so the settings panel can list it', () => {
    for (const id of STATUS_SEGMENT_IDS) {
      expect(STATUS_SEGMENT_LABELS[id].label.length).toBeGreaterThan(0);
      expect(STATUS_SEGMENT_LABELS[id].description.length).toBeGreaterThan(0);
    }
  });
});

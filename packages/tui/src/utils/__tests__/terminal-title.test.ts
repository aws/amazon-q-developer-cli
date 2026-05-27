import { describe, it, expect, beforeEach, afterEach, spyOn } from 'bun:test';
import { mkdirSync, writeFileSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import {
  deriveTitle,
  emitCurrentTitle,
  getCurrentTitle,
  initTerminalTitle,
  isTerminalTitleEnabled,
  readPersistedSessionTitle,
  refreshFromSession,
  resetTerminalTitleState,
  setUserTitle,
  clearUserTitle,
  resetTerminalTitle,
} from '../terminal-title.js';

let testDir: string;
let savedEnv: NodeJS.ProcessEnv;

function writeSessionFile(
  sessionId: string,
  data: Record<string, unknown>
): void {
  writeFileSync(join(testDir, `${sessionId}.json`), JSON.stringify(data));
}

beforeEach(() => {
  testDir = join(
    tmpdir(),
    `terminal-title-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
  mkdirSync(testDir, { recursive: true });
  savedEnv = { ...process.env };
  process.env.KIRO_TEST_SESSIONS_DIR = testDir;
  resetTerminalTitleState();
});

afterEach(() => {
  process.env = savedEnv;
  resetTerminalTitleState();
  try {
    rmSync(testDir, { recursive: true, force: true });
  } catch {
    /* ignore */
  }
});

describe('terminal-title', () => {
  describe('deriveTitle', () => {
    it('uses override when provided', () => {
      const result = deriveTitle({
        cwd: '/home/user/project',
        sessionTitle: 'session topic',
        override: 'my override',
      });
      expect(result).toBe('kiro: my override');
    });

    it('uses sessionTitle when no override', () => {
      const result = deriveTitle({
        cwd: '/home/user/project',
        sessionTitle: 'session topic',
      });
      expect(result).toBe('kiro: session topic');
    });

    it('falls back to shortened cwd when no override or sessionTitle', () => {
      const result = deriveTitle({ cwd: '/home/user/project' });
      expect(result).toStartWith('kiro: ');
      expect(result).toContain('project');
    });

    it('truncates long titles to 60 characters', () => {
      const longTitle =
        'this is the user\'s very long title that well exceeds the max length allowed for a title. because it is so long, we expect this title to be truncated after the "x" in "max"';
      const result = deriveTitle({ cwd: '/x', override: longTitle });
      expect(result).toBe(
        "kiro: this is the user's very long title that well exceeds the max"
      );
    });

    it('skips whitespace-only override', () => {
      const result = deriveTitle({
        cwd: '/home/user/project',
        sessionTitle: 'real title',
        override: '   ',
      });
      expect(result).toBe('kiro: real title');
    });

    it('skips whitespace-only sessionTitle', () => {
      const result = deriveTitle({
        cwd: '/home/user/project',
        sessionTitle: '   ',
      });
      expect(result).toContain('project');
    });
  });

  describe('readPersistedSessionTitle', () => {
    it('returns undefined when file does not exist', async () => {
      expect(await readPersistedSessionTitle('nonexistent')).toBeUndefined();
    });

    it('returns undefined for malformed JSON', async () => {
      writeFileSync(join(testDir, 'bad.json'), 'not json{{{');
      expect(await readPersistedSessionTitle('bad')).toBeUndefined();
    });

    it('returns undefined when title field is missing', async () => {
      writeSessionFile('no-title', { session_id: 'no-title', cwd: '/tmp' });
      expect(await readPersistedSessionTitle('no-title')).toBeUndefined();
    });

    it('returns undefined when title is empty string', async () => {
      writeSessionFile('empty', { title: '' });
      expect(await readPersistedSessionTitle('empty')).toBeUndefined();
    });

    it('returns undefined when title is whitespace only', async () => {
      writeSessionFile('spaces', { title: '   ' });
      expect(await readPersistedSessionTitle('spaces')).toBeUndefined();
    });

    it('returns trimmed title when present', async () => {
      writeSessionFile('good', { title: '  WFP build fix  ' });
      expect(await readPersistedSessionTitle('good')).toBe('WFP build fix');
    });
  });

  describe('getCurrentTitle', () => {
    it('returns cwd-based title by default', () => {
      const title = getCurrentTitle();
      expect(title).toStartWith('kiro: ');
    });

    it('reflects session title after refreshFromSession', async () => {
      writeSessionFile('sess1', { title: 'my session' });
      await refreshFromSession('sess1');
      expect(getCurrentTitle()).toBe('kiro: my session');
    });
  });

  describe('refreshFromSession', () => {
    it('updates lastSessionTitle from persisted file', async () => {
      writeSessionFile('sess1', { title: 'persisted title' });
      await refreshFromSession('sess1');
      expect(getCurrentTitle()).toBe('kiro: persisted title');
    });

    it('clears lastSessionTitle when file has no title', async () => {
      writeSessionFile('sess1', { title: 'old title' });
      await refreshFromSession('sess1');
      expect(getCurrentTitle()).toBe('kiro: old title');

      writeSessionFile('sess2', { cwd: '/tmp' });
      await refreshFromSession('sess2');
      expect(getCurrentTitle()).not.toContain('old title');
    });

    it('does not overwrite userOverride when session title changes', async () => {
      initTerminalTitle({ isEnabled: () => true });
      setUserTitle('my override');

      writeSessionFile('sess1', { title: 'new session title' });
      await refreshFromSession('sess1');

      // userOverride takes precedence — getCurrentTitle still shows the override
      expect(getCurrentTitle()).toBe('kiro: my override');
    });
  });

  describe('initTerminalTitle', () => {
    it('sets the isEnabled getter', () => {
      expect(isTerminalTitleEnabled()).toBe(false);
      initTerminalTitle({ isEnabled: () => true });
      expect(isTerminalTitleEnabled()).toBe(true);
    });

    it('respects a dynamic getter that changes over time', () => {
      let enabled = false;
      initTerminalTitle({ isEnabled: () => enabled });
      expect(isTerminalTitleEnabled()).toBe(false);
      enabled = true;
      expect(isTerminalTitleEnabled()).toBe(true);
    });

    it('supports full enable→disable→re-enable cycle', () => {
      let enabled = true;
      initTerminalTitle({ isEnabled: () => enabled });

      // Enabled: setUserTitle works and emits
      const writes: string[] = [];
      const spy = spyOn(process.stdout, 'write').mockImplementation((data) => {
        writes.push(String(data));
        return true;
      });

      const result = setUserTitle('my title');
      expect(result).toEqual({ ok: true, title: 'kiro: my title' });
      expect(writes).toContain('\x1b]0;kiro: my title\x07');

      // Disable: setUserTitle is rejected, no writes
      enabled = false;
      writes.length = 0;
      const disabled = setUserTitle('ignored');
      expect(disabled).toEqual({ ok: false, reason: 'disabled' });
      expect(writes).toHaveLength(0);

      // Re-enable after clearing: emitCurrentTitle writes the title
      enabled = true;
      writes.length = 0;
      // Reset clears lastEmitted, so the next emit will write
      resetTerminalTitle();
      writes.length = 0;
      emitCurrentTitle();
      expect(writes).toContain('\x1b]0;kiro: my title\x07');

      spy.mockRestore();
    });
  });

  describe('setUserTitle', () => {
    it('returns disabled when feature is not enabled', () => {
      const result = setUserTitle('hello');
      expect(result).toEqual({ ok: false, reason: 'disabled' });
    });

    it('returns empty when input is whitespace-only', () => {
      initTerminalTitle({ isEnabled: () => true });
      const result = setUserTitle('   ');
      expect(result).toEqual({ ok: false, reason: 'empty' });
    });

    it('sets the title and returns ok when enabled', () => {
      initTerminalTitle({ isEnabled: () => true });
      const result = setUserTitle('my project');
      expect(result).toEqual({ ok: true, title: 'kiro: my project' });
      expect(getCurrentTitle()).toBe('kiro: my project');
    });

    it('does not emit duplicate writes for the same title', () => {
      initTerminalTitle({ isEnabled: () => true });
      setUserTitle('same title');

      const writes: string[] = [];
      const spy = spyOn(process.stdout, 'write').mockImplementation((data) => {
        writes.push(String(data));
        return true;
      });

      // Setting the same title again should be a no-op (dedup via lastEmitted)
      setUserTitle('same title');
      expect(writes).toHaveLength(0);

      spy.mockRestore();
    });

    it('sanitizes and truncates the title', () => {
      initTerminalTitle({ isEnabled: () => true });
      const longTitle = 'a'.repeat(100);
      const result = setUserTitle(longTitle);
      expect(result.ok).toBe(true);
      if (result.ok) {
        // 'kiro: ' (6 chars) + MAX_TITLE_LENGTH (60) = 66
        expect(result.title.length).toBe(66);
      }
    });

    it('strips escape sequences to prevent terminal injection', () => {
      initTerminalTitle({ isEnabled: () => true });
      const result = setUserTitle('hello\x1b]0;evil\x07world');
      expect(result.ok).toBe(true);
      if (result.ok) {
        // Control chars (\x1b, \x07) are stripped; printable chars remain
        expect(result.title).toBe('kiro: hello]0;evilworld');
        expect(result.title).not.toContain('\x1b');
        expect(result.title).not.toContain('\x07');
      }

      // Also strips \x9C (String Terminator) — alternative OSC terminator
      const st = setUserTitle('test\x9Cinjection');
      expect(st.ok).toBe(true);
      if (st.ok) {
        expect(st.title).toBe('kiro: testinjection');
      }
    });
  });

  describe('clearUserTitle', () => {
    it('returns disabled when feature is not enabled', () => {
      const result = clearUserTitle();
      expect(result).toEqual({ ok: false, reason: 'disabled' });
    });

    it('clears the override and reverts to auto title when enabled', async () => {
      initTerminalTitle({ isEnabled: () => true });
      setUserTitle('sticky title');
      expect(getCurrentTitle()).toBe('kiro: sticky title');

      const result = clearUserTitle();
      expect(result).toEqual({ ok: true });
      // Should revert to cwd-based title (no session title set)
      expect(getCurrentTitle()).not.toContain('sticky title');
    });

    it('reverts to session title after clearing override', async () => {
      initTerminalTitle({ isEnabled: () => true });
      writeSessionFile('sess1', { title: 'session topic' });
      await refreshFromSession('sess1');
      setUserTitle('override');
      expect(getCurrentTitle()).toBe('kiro: override');

      clearUserTitle();
      expect(getCurrentTitle()).toBe('kiro: session topic');
    });
  });

  describe('resetTerminalTitle', () => {
    it('is a no-op when no title was ever emitted', () => {
      const writes: string[] = [];
      const spy = spyOn(process.stdout, 'write').mockImplementation((data) => {
        writes.push(String(data));
        return true;
      });
      resetTerminalTitle();
      expect(writes).toHaveLength(0);
      spy.mockRestore();
    });

    it('clears the title even after the feature is disabled', () => {
      // Simulate: feature was enabled, title was written, then feature was disabled
      initTerminalTitle({ isEnabled: () => true });
      setUserTitle('active title');

      // Now disable the feature (simulates store update)
      initTerminalTitle({ isEnabled: () => false });

      const writes: string[] = [];
      const spy = spyOn(process.stdout, 'write').mockImplementation((data) => {
        writes.push(String(data));
        return true;
      });
      resetTerminalTitle();

      // Should write the empty OSC 0 sequence to clear the stale title
      expect(writes).toContain('\x1b]0;\x07');
      spy.mockRestore();
    });
  });
});

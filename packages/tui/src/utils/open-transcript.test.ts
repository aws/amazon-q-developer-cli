import { describe, test, expect, beforeEach, afterEach, mock } from 'bun:test';
import { openTranscriptInPager } from './open-transcript.js';

/**
 * Security regression for the Windows Notepad launch in
 * `openTranscriptInPager` (CWE-426 untrusted search path). With no PAGER set,
 * Notepad must be launched by its ABSOLUTE System32 path with `shell:false`
 * (a bare `notepad` via CreateProcess searches the CWD first).
 *
 * Effectful dependencies (spawnSync, fs, shell-escape) are injected directly so
 * the test is hermetic without `mock.module`, which is process-global in bun
 * and leaks across test files.
 */

const spawnSyncMock = mock(
  (_file: string, _args?: readonly string[], _opts?: unknown): unknown => ({
    error: undefined,
    status: 0,
  })
);

// Fake effectful deps: no real temp file, pager, or terminal mutation.
const deps = {
  spawnSync: spawnSyncMock,
  mkdtempSync: (p: string) => `${p}tmpdir`,
  writeFileSync: () => undefined,
  unlinkSync: () => undefined,
  executeShellEscapeTTY: () => ({ error: undefined }),
  restoreTerminalModes: () => undefined,
} as unknown as Parameters<typeof openTranscriptInPager>[3];

const origPlatform = process.platform;
const origEnv = process.env;

function setPlatform(platform: string): void {
  Object.defineProperty(process, 'platform', {
    value: platform,
    configurable: true,
  });
}

beforeEach(() => {
  spawnSyncMock.mockClear();
  process.env = { ...origEnv, SystemRoot: 'C:\\Windows' };
  delete process.env.PAGER;
});

afterEach(() => {
  Object.defineProperty(process, 'platform', {
    value: origPlatform,
    configurable: true,
  });
  process.env = origEnv;
});

describe('openTranscriptInPager (Windows, no PAGER)', () => {
  beforeEach(() => setPlatform('win32'));

  test('launches notepad.exe by absolute System32 path with shell:false', () => {
    openTranscriptInPager(
      [{ role: 'user', content: 'hi' }],
      'pre-rendered',
      'md',
      deps
    );

    expect(spawnSyncMock).toHaveBeenCalledTimes(1);
    const [file, args, opts] = spawnSyncMock.mock.calls[0] as [
      string,
      string[],
      { shell?: boolean },
    ];
    expect(file).toBe('C:\\Windows\\System32\\notepad.exe');
    expect(file).not.toBe('notepad');
    expect(Array.isArray(args)).toBe(true);
    expect(opts.shell).toBe(false);
  });
});

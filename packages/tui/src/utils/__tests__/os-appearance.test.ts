import { describe, it, expect, beforeEach, afterEach, mock } from 'bun:test';
import { getOSAppearance, type OSAppearanceDeps } from '../os-appearance.js';

/**
 * Single source of truth for getOSAppearance. Covers macOS/Windows/other
 * detection logic AND the Windows security hardening (CWE-426 untrusted search
 * path): on win32 the theme is read by invoking reg.exe via its ABSOLUTE
 * System32 path with `shell:false` and an args array, never a bare `reg`
 * through cmd.exe (which searches the CWD before PATH and could run a planted
 * `.\reg.exe` at pre-trust startup).
 *
 * Exec dependencies are injected directly. We deliberately do NOT use
// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, [
  'child_process',
]);

 * `mock.module('child_process', ...)` here: bun's module mocks are
 * process-global and leak into every other test file in the run.
 */

const execSyncMock = mock((): string => '');
const execFileSyncMock = mock(
  (_file: string, _args?: readonly string[], _opts?: unknown): string => ''
);

const deps: OSAppearanceDeps = {
  execSync: execSyncMock as unknown as OSAppearanceDeps['execSync'],
  execFileSync: execFileSyncMock as unknown as OSAppearanceDeps['execFileSync'],
};

const originalPlatform = process.platform;
const originalEnv = process.env;

function setPlatform(platform: string): void {
  Object.defineProperty(process, 'platform', {
    value: platform,
    configurable: true,
  });
}

beforeEach(() => {
  execSyncMock.mockClear();
  execFileSyncMock.mockClear();
  // mockClear keeps the default impl (returns '') so unset cases stay safe.
  execSyncMock.mockImplementation(() => '');
  execFileSyncMock.mockImplementation(() => '');
  process.env = { ...originalEnv };
});

afterEach(() => {
  setPlatform(originalPlatform);
  process.env = originalEnv;
});

describe('getOSAppearance', () => {
  describe('macOS (darwin)', () => {
    beforeEach(() => {
      setPlatform('darwin');
    });

    it('returns dark when defaults reports Dark', () => {
      execSyncMock.mockImplementation(() => 'Dark\n');
      expect(getOSAppearance(deps)).toBe('dark');
    });

    it('returns light when defaults returns empty string', () => {
      execSyncMock.mockImplementation(() => '');
      expect(getOSAppearance(deps)).toBe('light');
    });

    it('returns light when defaults returns Light', () => {
      execSyncMock.mockImplementation(() => 'Light');
      expect(getOSAppearance(deps)).toBe('light');
    });

    it('returns light when the command throws (macOS fallback)', () => {
      execSyncMock.mockImplementation(() => {
        throw new Error('command failed');
      });
      expect(getOSAppearance(deps)).toBe('light');
    });

    it('reads the appearance via the documented macOS command', () => {
      execSyncMock.mockImplementation(() => 'Dark');
      getOSAppearance(deps);
      expect(execSyncMock).toHaveBeenCalledWith(
        'defaults read -g AppleInterfaceStyle',
        expect.objectContaining({ encoding: 'utf8' })
      );
    });
  });

  describe('Windows (win32)', () => {
    beforeEach(() => {
      setPlatform('win32');
      process.env.SystemRoot = 'C:\\Windows';
    });

    // --- detection logic ---

    it('returns dark when the registry value is 0x0', () => {
      execFileSyncMock.mockImplementation(
        () =>
          'HKEY_CURRENT_USER\\...\\Personalize\n    AppsUseLightTheme    REG_DWORD    0x0\n'
      );
      expect(getOSAppearance(deps)).toBe('dark');
    });

    it('returns light when the registry value is 0x1', () => {
      execFileSyncMock.mockImplementation(
        () =>
          'HKEY_CURRENT_USER\\...\\Personalize\n    AppsUseLightTheme    REG_DWORD    0x1\n'
      );
      expect(getOSAppearance(deps)).toBe('light');
    });

    it('returns dark when the registry query throws', () => {
      execFileSyncMock.mockImplementation(() => {
        throw new Error('command failed');
      });
      expect(getOSAppearance(deps)).toBe('dark');
    });

    // --- security hardening (CWE-426) ---

    it('queries the registry via the absolute reg.exe path with shell:false', () => {
      execFileSyncMock.mockImplementation(
        () => '    AppsUseLightTheme    REG_DWORD    0x1\n'
      );

      expect(getOSAppearance(deps)).toBe('light');

      // execSync must never be used on the Windows path.
      expect(execSyncMock).not.toHaveBeenCalled();
      expect(execFileSyncMock).toHaveBeenCalledTimes(1);
      const [file, args, opts] = execFileSyncMock.mock.calls[0] as [
        string,
        string[],
        { shell?: boolean },
      ];
      expect(file).toBe('C:\\Windows\\System32\\reg.exe');
      expect(file).not.toBe('reg');
      expect(args[0]).toBe('query');
      expect(args).toContain('AppsUseLightTheme');
      expect(opts.shell).toBe(false);
    });

    it('falls back to the C:\\Windows System32 path when SystemRoot is unset', () => {
      delete process.env.SystemRoot;
      execFileSyncMock.mockImplementation(
        () => 'AppsUseLightTheme    REG_DWORD    0x1'
      );
      getOSAppearance(deps);
      const [file] = execFileSyncMock.mock.calls[0] as [string];
      expect(file).toBe('C:\\Windows\\System32\\reg.exe');
    });
  });

  describe('Linux and other platforms', () => {
    it('returns dark on linux without invoking any exec', () => {
      setPlatform('linux');
      expect(getOSAppearance(deps)).toBe('dark');
      expect(execSyncMock).not.toHaveBeenCalled();
      expect(execFileSyncMock).not.toHaveBeenCalled();
    });

    it('returns dark on freebsd without invoking any exec', () => {
      setPlatform('freebsd');
      expect(getOSAppearance(deps)).toBe('dark');
      expect(execSyncMock).not.toHaveBeenCalled();
      expect(execFileSyncMock).not.toHaveBeenCalled();
    });
  });
});

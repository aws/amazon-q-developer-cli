import { describe, it, expect, beforeEach, afterEach, mock } from 'bun:test';

const mockExecSync = mock((): string => '');

mock.module('child_process', () => ({
  execSync: mockExecSync,
}));

// Import AFTER mock.module
const { getOSAppearance } = await import('../os-appearance');

const originalPlatform = process.platform;

afterEach(() => {
  mockExecSync.mockReset();
  Object.defineProperty(process, 'platform', {
    value: originalPlatform,
    configurable: true,
  });
});

describe('getOSAppearance', () => {
  describe('macOS (darwin)', () => {
    beforeEach(() => {
      Object.defineProperty(process, 'platform', {
        value: 'darwin',
        configurable: true,
      });
    });

    it('returns dark when execSync returns Dark', () => {
      mockExecSync.mockImplementation(() => 'Dark\n');
      expect(getOSAppearance()).toBe('dark');
    });

    it('returns light when execSync returns empty string', () => {
      mockExecSync.mockImplementation(() => '');
      expect(getOSAppearance()).toBe('light');
    });

    it('returns light when execSync returns Light', () => {
      mockExecSync.mockImplementation(() => 'Light');
      expect(getOSAppearance()).toBe('light');
    });

    it('returns light when execSync throws (macOS fallback)', () => {
      mockExecSync.mockImplementation(() => {
        throw new Error('command failed');
      });
      expect(getOSAppearance()).toBe('light');
    });
  });

  describe('Windows (win32)', () => {
    beforeEach(() => {
      Object.defineProperty(process, 'platform', {
        value: 'win32',
        configurable: true,
      });
    });

    it('returns dark when registry result includes 0x0', () => {
      mockExecSync.mockImplementation(
        () =>
          'HKEY_CURRENT_USER\\...\\Personalize\n    AppsUseLightTheme    REG_DWORD    0x0\n'
      );
      expect(getOSAppearance()).toBe('dark');
    });

    it('returns light when registry result includes 0x1', () => {
      mockExecSync.mockImplementation(
        () =>
          'HKEY_CURRENT_USER\\...\\Personalize\n    AppsUseLightTheme    REG_DWORD    0x1\n'
      );
      expect(getOSAppearance()).toBe('light');
    });

    it('returns dark when execSync throws on Windows', () => {
      mockExecSync.mockImplementation(() => {
        throw new Error('command failed');
      });
      expect(getOSAppearance()).toBe('dark');
    });
  });

  describe('Linux and other platforms', () => {
    it('returns dark on linux without calling execSync', () => {
      Object.defineProperty(process, 'platform', {
        value: 'linux',
        configurable: true,
      });
      expect(getOSAppearance()).toBe('dark');
      expect(mockExecSync).not.toHaveBeenCalled();
    });

    it('returns dark on freebsd without calling execSync', () => {
      Object.defineProperty(process, 'platform', {
        value: 'freebsd',
        configurable: true,
      });
      expect(getOSAppearance()).toBe('dark');
      expect(mockExecSync).not.toHaveBeenCalled();
    });
  });

  it('calls execSync with correct macOS command', () => {
    Object.defineProperty(process, 'platform', {
      value: 'darwin',
      configurable: true,
    });
    mockExecSync.mockImplementation(() => 'Dark');
    getOSAppearance();
    expect(mockExecSync).toHaveBeenCalledWith(
      'defaults read -g AppleInterfaceStyle',
      expect.objectContaining({ encoding: 'utf8' })
    );
  });
});

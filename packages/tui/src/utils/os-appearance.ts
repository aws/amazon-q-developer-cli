import {
  execSync as realExecSync,
  execFileSync as realExecFileSync,
} from 'child_process';
import { system32Path } from './windows-paths.js';

/**
 * Injectable exec dependencies for {@link getOSAppearance}. Both default to the
 * real `child_process` implementation; tests inject fakes directly so no
 * process-global module mocking (which leaks across bun test files) is needed.
 */
export interface OSAppearanceDeps {
  execSync?: typeof realExecSync;
  execFileSync?: typeof realExecFileSync;
}

/**
 * Detects the OS appearance mode (dark or light).
 * Works on macOS and Windows. Defaults to 'dark' on other platforms or if detection fails.
 *
 * @returns 'dark' or 'light'
 */
export function getOSAppearance(deps: OSAppearanceDeps = {}): 'dark' | 'light' {
  const execSync = deps.execSync ?? realExecSync;
  const execFileSync = deps.execFileSync ?? realExecFileSync;
  try {
    if (process.platform === 'darwin') {
      // macOS: Check AppleInterfaceStyle preference
      // Note: This key only exists when dark mode is enabled
      // If the command fails, it means light mode is active
      const result = execSync('defaults read -g AppleInterfaceStyle', {
        encoding: 'utf8',
        stdio: ['pipe', 'pipe', 'ignore'], // Suppress stderr
      });
      return result.trim() === 'Dark' ? 'dark' : 'light';
    } else if (process.platform === 'win32') {
      // Windows: Check registry for AppsUseLightTheme.
      // Invoke reg.exe by its absolute System32 path with shell:false and an
      // args array so it is never resolved via cmd.exe (CWE-426); see
      // system32Path for the full rationale.
      const result = execFileSync(
        system32Path('reg.exe'),
        [
          'query',
          'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize',
          '/v',
          'AppsUseLightTheme',
        ],
        {
          shell: false,
          encoding: 'utf8',
          stdio: ['pipe', 'pipe', 'ignore'], // Suppress stderr
        }
      );
      // 0x0 = dark mode, 0x1 = light mode
      return result.includes('0x0') ? 'dark' : 'light';
    }
  } catch {
    // On macOS, if the command fails, it means light mode is active
    if (process.platform === 'darwin') {
      return 'light';
    }
    // For other platforms or errors, default to dark
  }

  // Default to dark for Linux or unknown platforms
  return 'dark';
}

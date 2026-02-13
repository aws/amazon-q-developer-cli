import { execSync } from 'child_process';

/**
 * Detects the OS appearance mode (dark or light).
 * Works on macOS and Windows. Defaults to 'dark' on other platforms or if detection fails.
 *
 * @returns 'dark' or 'light'
 */
export function getOSAppearance(): 'dark' | 'light' {
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
      // Windows: Check registry for AppsUseLightTheme
      const result = execSync(
        'reg query HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize /v AppsUseLightTheme',
        {
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

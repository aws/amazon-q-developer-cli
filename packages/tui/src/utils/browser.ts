/** Best-effort WSL detection via the Linux kernel osrelease string. */
function detectWsl(): boolean {
  if (process.platform !== 'linux') return false;
  try {
    const { readFileSync } = require('fs');
    const release = readFileSync(
      '/proc/sys/kernel/osrelease',
      'utf8'
    ).toLowerCase();
    return release.includes('microsoft') || release.includes('wsl');
  } catch {
    return false;
  }
}

/**
 * Build the argv to open a URL in the default browser, per platform. Windows
 * uses rundll32's URL handler (not cmd `start`, which mangles `&` and treats
 * the URL as a window title); WSL uses wslview to reach the Windows browser.
 * URL stays its own argv element — no shell. Pure for testability.
 */
export function browserOpenCommand(
  platform: NodeJS.Platform,
  url: string,
  isWsl = false
): { file: string; args: string[] } {
  if (platform === 'darwin') return { file: 'open', args: [url] };
  if (platform === 'win32')
    return { file: 'rundll32', args: ['url.dll,FileProtocolHandler', url] };
  if (isWsl) return { file: 'wslview', args: [url] };
  return { file: 'xdg-open', args: [url] };
}

/**
 * Open a URL in the user's default browser. Returns true on success, false if
 * the platform opener could not be spawned (caller should then surface the URL
 * for manual copy). Never throws.
 */
export function openUrlInBrowser(url: string): boolean {
  try {
    const { execFileSync } = require('child_process');
    const { file, args } = browserOpenCommand(
      process.platform,
      url,
      detectWsl()
    );
    execFileSync(file, args, { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

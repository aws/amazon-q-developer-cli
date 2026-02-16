import { execSync } from 'child_process';

/**
 * Queries the terminal background color using OSC 11 escape sequence.
 * Sends `\e]11;?\a` to the terminal and parses the response.
 *
 * Supported by: iTerm2, Terminal.app, Ghostty, kitty, WezTerm, Alacritty,
 * xterm, foot, most modern terminal emulators.
 *
 * Response format: `\e]11;rgb:RRRR/GGGG/BBBB\e\\` or `\e]11;rgb:RR/GG/BB\a`
 * where R/G/B are hex values (either 2 or 4 hex digits per channel).
 *
 * Uses /dev/tty directly to avoid interference with stdin/stdout which may
 * be redirected by Ink/React. This is a synchronous operation with a short
 * timeout to avoid hanging if the terminal doesn't support OSC 11.
 *
 * @returns The detected theme ('dark' or 'light'), or null if detection failed.
 */
export function queryTerminalBackground(): 'dark' | 'light' | null {
  // Only works on Unix-like systems with /dev/tty
  if (process.platform === 'win32') {
    return null;
  }

  // Don't attempt if not a TTY (e.g., piped input, CI, non-interactive)
  if (!process.stdin.isTTY) {
    return null;
  }

  try {
    // Use a shell one-liner that:
    // 1. Saves terminal state with stty
    // 2. Sets raw mode with a read timeout
    // 3. Sends OSC 11 query to /dev/tty
    // 4. Reads the response
    // 5. Restores terminal state
    //
    // All I/O goes through /dev/tty to avoid interfering with stdin/stdout
    // which Ink/React may have redirected.
    //
    // The `stty -echo raw min 0 time 2` sets:
    //   -echo: don't echo input
    //   raw: raw mode (no line buffering)
    //   min 0: don't wait for minimum chars
    //   time 2: 200ms timeout (in tenths of a second)
    const result = execSync(
      `old_state=$(stty -g < /dev/tty 2>/dev/null) && ` +
        `stty -echo raw min 0 time 2 < /dev/tty 2>/dev/null && ` +
        `printf '\\033]11;?\\007' > /dev/tty && ` +
        `dd bs=256 count=1 < /dev/tty 2>/dev/null; ` +
        `stty "$old_state" < /dev/tty 2>/dev/null`,
      {
        encoding: 'latin1', // Preserve raw bytes
        timeout: 1000,
        stdio: ['pipe', 'pipe', 'ignore'],
        shell: '/bin/sh',
      }
    );

    return parseOsc11Response(result);
  } catch {
    // Terminal doesn't support OSC 11, /dev/tty not available, or timeout
    return null;
  }
}

/**
 * Parses an OSC 11 response to extract the background color and determine
 * if it's dark or light.
 *
 * Response formats:
 * - `\e]11;rgb:RRRR/GGGG/BBBB\e\\` (4 hex digits per channel)
 * - `\e]11;rgb:RR/GG/BB\a` (2 hex digits per channel)
 * - `\e]11;rgb:R/G/B\a` (1 hex digit per channel, rare)
 *
 * @param response - Raw response string from the terminal
 * @returns 'dark' or 'light', or null if parsing failed
 */
export function parseOsc11Response(response: string): 'dark' | 'light' | null {
  // Match the rgb color in the response
  // The response contains: \e]11;rgb:RRRR/GGGG/BBBB followed by BEL or ST
  const match = response.match(
    /rgb:([0-9a-fA-F]+)\/([0-9a-fA-F]+)\/([0-9a-fA-F]+)/
  );
  if (!match) {
    return null;
  }

  const [, rHex, gHex, bHex] = match;
  if (!rHex || !gHex || !bHex) {
    return null;
  }

  // Normalize to 0-255 range regardless of hex digit count
  const normalize = (hex: string): number => {
    const val = parseInt(hex, 16);
    switch (hex.length) {
      case 1:
        return val * 17; // 0xF -> 255
      case 2:
        return val; // 0xFF -> 255
      case 4:
        return val >> 8; // 0xFFFF -> 255
      default:
        return val;
    }
  };

  const r = normalize(rHex);
  const g = normalize(gHex);
  const b = normalize(bHex);

  // Calculate perceived luminance using the sRGB luminance formula
  // Values > 128 are considered "light" backgrounds
  const luminance = 0.299 * r + 0.587 * g + 0.114 * b;

  return luminance > 128 ? 'light' : 'dark';
}

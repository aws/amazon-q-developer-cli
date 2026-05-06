/**
 * Resolve the root directory for user-level Kiro config data.
 *
 * Honors the `KIRO_HOME` environment variable when set; otherwise falls back
 * to `$HOME/.kiro` (or `%USERPROFILE%\.kiro` on Windows). Anything that
 * previously hard-coded `~/.kiro` should go through this helper so users can
 * relocate their config with `KIRO_HOME`.
 */

import { homedir } from 'os';
import { join } from 'path';

/** Path to the Kiro home directory, honoring the `KIRO_HOME` env var. */
export function kiroHomeDir(): string {
  const override = process.env.KIRO_HOME;
  if (override && override.length > 0) {
    return override;
  }
  const home = process.env.HOME || process.env.USERPROFILE || homedir();
  return join(home, '.kiro');
}

/** Join subpaths to the Kiro home directory. */
export function kiroHomePath(...segments: string[]): string {
  return join(kiroHomeDir(), ...segments);
}

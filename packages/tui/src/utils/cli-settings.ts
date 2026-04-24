import { join } from 'path';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { homedir } from 'os';
import { logger } from './logger.js';

/**
 * Shared synchronous access to the user's global kiro-cli settings at
 * `~/.kiro/settings/cli.json` (mirrors the path used by `chat-cli`'s
 * Rust settings loader).
 *
 * Why a file read instead of going through ACP?
 *
 *   Some settings are needed before the ACP backend is initialized and
 *   before React mounts — e.g. `chat.disableWrap` which must be passed
 *   to `render()` so twinki can track physical rows from the first frame.
 *   For runtime settings that are safe to read after ACP is up, prefer
 *   the regular ACP/session-info pathways.
 */

function settingsPath(): string {
  const home = process.env.HOME || process.env.USERPROFILE || homedir();
  return join(home, '.kiro', 'settings', 'cli.json');
}

/** Returns the parsed cli.json object, or `{}` on any error. */
export function readCliSettings(): Record<string, unknown> {
  try {
    const p = settingsPath();
    if (!existsSync(p)) return {};
    const raw = JSON.parse(readFileSync(p, 'utf-8'));
    if (raw && typeof raw === 'object' && !Array.isArray(raw)) {
      return raw as Record<string, unknown>;
    }
  } catch (err) {
    logger.warn('[cli-settings] failed to read cli.json:', err);
  }
  return {};
}

/** Overwrites cli.json with the provided settings object. */
export function writeCliSettings(settings: Record<string, unknown>): void {
  const p = settingsPath();
  const dir = join(p, '..');
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }
  writeFileSync(p, JSON.stringify(settings, null, 2), 'utf-8');
}

/** Read a boolean setting with a fallback when the key is missing or malformed. */
export function readBoolSetting(key: string, fallback = false): boolean {
  const val = readCliSettings()[key];
  return typeof val === 'boolean' ? val : fallback;
}

import { join } from 'path';
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  existsSync,
  renameSync,
  unlinkSync,
} from 'fs';
import { logger } from './logger.js';
import { kiroHomePath } from './kiro-home.js';

/**
 * Shared synchronous access to the user's global kiro-cli settings at
 * `~/.kiro/settings/cli.json` (or `$KIRO_HOME/settings/cli.json`, mirroring
 * the path used by `chat-cli`'s Rust settings loader).
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
  return kiroHomePath('settings', 'cli.json');
}

/**
 * Returns the parsed cli.json object, or `{}` if the file is missing.
 * Throws on parse/read errors to allow callers to distinguish
 * "no settings file" from "corrupt/unreadable file".
 */
function readCliSettingsStrict(): Record<string, unknown> {
  const p = settingsPath();
  if (!existsSync(p)) return {};
  const raw = JSON.parse(readFileSync(p, 'utf-8'));
  if (raw && typeof raw === 'object' && !Array.isArray(raw)) {
    return raw as Record<string, unknown>;
  }
  return {};
}

/** Returns the parsed cli.json object, or `{}` on any error. */
export function readCliSettings(): Record<string, unknown> {
  try {
    return readCliSettingsStrict();
  } catch (err) {
    logger.warn('[cli-settings] failed to read cli.json:', err);
  }
  return {};
}

/**
 * In-process write queue ensuring concurrent async callers within the same
 * process don't interleave read-modify-write cycles.
 */
let writeQueue: Promise<void> = Promise.resolve();

/**
 * Overwrites cli.json with the provided settings object.
 * Uses atomic write (temp + rename) to prevent corruption if the process
 * crashes mid-write. NOT cross-process safe; concurrent Rust/KAS writers
 * to the same file may still result in last-write-wins.
 */
export function writeCliSettings(settings: Record<string, unknown>): void {
  const p = settingsPath();
  const dir = join(p, '..');
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }
  const tmp = `${p}.${process.pid}.${Date.now()}.tmp`;
  try {
    writeFileSync(tmp, JSON.stringify(settings, null, 2), 'utf-8');
    renameSync(tmp, p);
  } catch (err) {
    // Clean up orphaned temp file on failure
    try {
      if (existsSync(tmp)) unlinkSync(tmp);
    } catch {
      /* best-effort */
    }
    throw err;
  }
}

/**
 * Async read-modify-write helper that serializes updates within the process
 * and uses atomic file replacement. Prefer this over manual
 * readCliSettings + writeCliSettings when updating a single key.
 *
 * Refuses to overwrite if the settings file exists but is corrupt/unreadable
 * (prevents wiping valid settings due to transient I/O errors).
 */
export async function updateCliSetting(
  key: string,
  value: unknown
): Promise<void> {
  const next = writeQueue.then(() => {
    const settings = readCliSettingsStrict();
    settings[key] = value;
    writeCliSettings(settings);
  });
  // Prevent queue poisoning: chain always resolves so future calls proceed
  writeQueue = next.catch(() => {});
  await next;
}

/**
 * Async serialized read-MODIFY-write helper. Unlike {@link updateCliSetting}
 * (which writes a precomputed value), this reads the CURRENT value of `key`
 * FRESH inside the write queue, passes it to `fn`, and writes the result back —
 * all within the same serialized critical section. Use this whenever the new
 * value depends on the existing one (e.g. deep-merging a nested object), so a
 * concurrent in-process writer to the same key cannot cause a lost update by
 * mutating it between an out-of-band read and the write.
 *
 * Like updateCliSetting, refuses to overwrite when the file exists but is
 * corrupt/unreadable (readCliSettingsStrict throws → chain rejects, settings
 * untouched).
 */
export async function updateCliSettingWith(
  key: string,
  fn: (prev: unknown) => unknown
): Promise<void> {
  const next = writeQueue.then(() => {
    const settings = readCliSettingsStrict();
    settings[key] = fn(settings[key]);
    writeCliSettings(settings);
  });
  // Prevent queue poisoning: chain always resolves so future calls proceed
  writeQueue = next.catch(() => {});
  await next;
}

/** Read a boolean setting with a fallback when the key is missing or malformed. */
export function readBoolSetting(key: string, fallback = false): boolean {
  const val = readCliSettings()[key];
  return typeof val === 'boolean' ? val : fallback;
}

/** Read a string setting with a fallback when the key is missing or malformed. */
export function readStringSetting(key: string, fallback: string): string {
  const val = readCliSettings()[key];
  return typeof val === 'string' ? val : fallback;
}

/** Read a string setting, returning undefined when absent or empty. */
export function readOptionalStringSetting(key: string): string | undefined {
  const val = readCliSettings()[key];
  return typeof val === 'string' && val !== '' ? val : undefined;
}

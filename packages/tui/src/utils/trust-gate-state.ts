import { join } from 'path';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { homedir } from 'os';
import { logger } from './logger.js';

const SETTING_KEY = 'chat.disableTrustAllConfirmation';

function cliSettingsPath(): string {
  const home = process.env.HOME || process.env.USERPROFILE || homedir();
  return join(home, '.kiro', 'settings', 'cli.json');
}

function readCliSettings(): Record<string, unknown> {
  try {
    const p = cliSettingsPath();
    if (existsSync(p)) {
      const raw = JSON.parse(readFileSync(p, 'utf-8'));
      if (raw && typeof raw === 'object' && !Array.isArray(raw)) {
        return raw as Record<string, unknown>;
      }
    }
  } catch {
    logger.warn('[trust-gate] Failed to read cli settings');
  }
  return {};
}

function writeCliSettings(settings: Record<string, unknown>): void {
  const p = cliSettingsPath();
  const dir = join(p, '..');
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }
  writeFileSync(p, JSON.stringify(settings, null, 2), 'utf-8');
}

/** Returns true if the user previously chose "don't ask again" for the trust-all-tools gate. */
export function isTrustGateAccepted(): boolean {
  return readCliSettings()[SETTING_KEY] === true;
}

/** Persist the user's choice to skip the trust-all-tools gate in future sessions. */
export function saveTrustGateAccepted(): void {
  try {
    const settings = readCliSettings();
    settings[SETTING_KEY] = true;
    writeCliSettings(settings);
  } catch (err) {
    logger.warn('[trust-gate] Failed to save trust gate preference:', err);
  }
}

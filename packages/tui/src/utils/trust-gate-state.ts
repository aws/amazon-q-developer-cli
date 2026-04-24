import { logger } from './logger.js';
import { readCliSettings, writeCliSettings } from './cli-settings.js';
import { Settings } from '../constants/settings.js';
import type { Kiro } from '../kiro.js';

const SETTING_KEY = Settings.CHAT_DISABLE_TRUST_ALL_CONFIRMATION;

/** Returns true if the user previously chose "don't ask again" for the trust-all-tools gate. */
export function isTrustGateAccepted(): boolean {
  return readCliSettings()[SETTING_KEY] === true;
}

/**
 * Persist the user's choice to skip the trust-all-tools gate in future sessions.
 *
 * When a {@link Kiro} instance is provided the write is routed through the ACP
 * backend which performs a locked read-modify-write on the settings file,
 * avoiding race conditions with the Rust process.  Falls back to a direct
 * file write when ACP is unavailable (should not happen in normal flow since
 * the trust gate is shown after ACP init).
 */
export function saveTrustGateAccepted(kiro?: Kiro): void {
  if (kiro) {
    // Fire-and-forget — the UI doesn't need to wait for the write to complete.
    kiro.setSetting(SETTING_KEY, true).catch((err) => {
      logger.warn(
        '[trust-gate] ACP settings/set failed, falling back to direct write:',
        err
      );
      directWriteTrustGate();
    });
  } else {
    directWriteTrustGate();
  }
}

/** Direct file write fallback (no cross-process locking). */
function directWriteTrustGate(): void {
  try {
    const settings = readCliSettings();
    settings[SETTING_KEY] = true;
    writeCliSettings(settings);
  } catch (err) {
    logger.warn('[trust-gate] Failed to save trust gate preference:', err);
  }
}

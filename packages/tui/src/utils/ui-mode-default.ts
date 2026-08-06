import type { Kiro } from '../kiro.js';
import type { UiMode } from '../types/ui-mode.js';
import { Settings } from '../constants/settings.js';
import { readCliSettings, writeCliSettings } from './cli-settings.js';
import { logger } from './logger.js';

/** Normalize the persisted chat.ui.mode setting for telemetry payloads. */
function normalizeUiModeForTelemetry(raw: string): 'lite' | 'tui' | 'unset' {
  return raw === 'lite' || raw === 'tui' ? raw : 'unset';
}

/** Persist the default through local settings and ACP without blocking a live switch. */
export function persistUiModeDefault(mode: UiMode, kiro: Kiro): void {
  const settings = readCliSettings();
  const previous = normalizeUiModeForTelemetry(
    String(settings[Settings.CHAT_UI_MODE] ?? '')
  );
  settings[Settings.CHAT_UI_MODE] = mode;
  try {
    writeCliSettings(settings);
  } catch (err) {
    logger.warn(
      '[ui-mode-default] failed to persist chat.ui.mode to cli.json:',
      err
    );
  }
  try {
    void kiro.setSetting(Settings.CHAT_UI_MODE, mode)?.catch(() => {});
  } catch {
    // The session client may not be attached yet.
  }
  if (previous !== mode) {
    kiro.sendUiModeDefaultChanged?.({
      from: previous,
      to: mode,
      sessionId: kiro.sessionId,
    });
  }
}

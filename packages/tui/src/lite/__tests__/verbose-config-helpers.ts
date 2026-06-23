import {
  setVerboseConfig,
  resetVerboseCache,
  DEFAULT_DISPLAY,
  type VerboseDisplayConfig,
} from '../verbose.js';

/**
 * All-flags-on display baseline: every toggle from DEFAULT_DISPLAY with a fresh
 * subagent clone, output uncapped, and an 80-char arg cap. Tests patch only the
 * cap(s) under test on top of this so each case reads as "this knob, this
 * expectation".
 */
export const BASE_DISPLAY: VerboseDisplayConfig = {
  ...DEFAULT_DISPLAY,
  subagent: { ...DEFAULT_DISPLAY.subagent },
  outputMaxLines: null,
  argsMaxChars: 80,
};

/** Apply a partial display on top of {@link BASE_DISPLAY}. */
export function setDisplay(overrides: Partial<VerboseDisplayConfig>): void {
  setVerboseConfig({ display: { ...BASE_DISPLAY, ...overrides } });
}

/**
 * Restore the all-flags-on display (empty filters + {@link BASE_DISPLAY}) so a
 * suite that mutated the global verbose config doesn't leak into the next file.
 */
export function restoreFullDefaults(): void {
  setVerboseConfig({ filters: [], display: BASE_DISPLAY });
  resetVerboseCache();
}

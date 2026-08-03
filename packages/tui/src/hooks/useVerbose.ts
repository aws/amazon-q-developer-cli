import { createContext, useContext, useSyncExternalStore } from 'react';
import {
  subscribeVerbose,
  cacheByVerboseVersion,
  getVerboseDisplay,
  getVerboseFilters,
  getTuiVerboseDisplay,
  getTuiVerboseFilters,
  shouldShowToolOutput,
  resolveThinkingDisplay,
  type VerboseDisplayConfig,
} from '../lite/verbose.js';
import { useAppStoreOptional } from '../stores/app-store.js';
import { readCliSettings } from '../utils/cli-settings.js';
import { Settings } from '../constants/settings.js';

const getDisplaySnapshot = cacheByVerboseVersion(getVerboseDisplay);
const getFiltersSnapshot = cacheByVerboseVersion(getVerboseFilters);
const getTuiDisplaySnapshot = cacheByVerboseVersion(getTuiVerboseDisplay);
const getTuiFiltersSnapshot = cacheByVerboseVersion(getTuiVerboseFilters);
const getThinkingSnapshot = cacheByVerboseVersion(
  () => getVerboseDisplay().thinkingDisplay
);
const getTuiThinkingSnapshot = cacheByVerboseVersion(
  () => getTuiVerboseDisplay().thinkingDisplay
);
const getLegacyThinkingSnapshot = cacheByVerboseVersion(() =>
  resolveThinkingDisplay(readCliSettings()[Settings.CHAT_SHOW_THINKING])
);

export const VerbosityOverrideContext = createContext<{
  display?: VerboseDisplayConfig;
  filters?: readonly string[];
} | null>(null);

function useSurfaceValue<T>(tui: () => T, lite: () => T): T {
  const surface = useAppStoreOptional((state) => state.uiMode, 'tui');
  return useSyncExternalStore(subscribeVerbose, surface === 'tui' ? tui : lite);
}

export function useVerboseDisplay(): VerboseDisplayConfig {
  const override = useContext(VerbosityOverrideContext);
  const display = useSurfaceValue(getTuiDisplaySnapshot, getDisplaySnapshot);
  return override?.display ?? display;
}

export function useThinkingDisplay(): VerboseDisplayConfig['thinkingDisplay'] {
  const override = useContext(VerbosityOverrideContext);
  const surface = useAppStoreOptional((state) => state.uiMode, 'tui');
  const rolloutEnabled = process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';
  const snapshot = rolloutEnabled
    ? surface === 'tui'
      ? getTuiThinkingSnapshot
      : getThinkingSnapshot
    : getLegacyThinkingSnapshot;
  const value = useSyncExternalStore(subscribeVerbose, snapshot);
  if (!rolloutEnabled) return value;
  return override?.display?.thinkingDisplay ?? value;
}

export function useShouldShowToolOutput(
  toolName: string,
  isMcp = false
): boolean {
  const override = useContext(VerbosityOverrideContext);
  const surfaceFilters = useSurfaceValue(
    getTuiFiltersSnapshot,
    getFiltersSnapshot
  );
  return shouldShowToolOutput(
    toolName,
    override?.filters ?? surfaceFilters,
    isMcp
  );
}

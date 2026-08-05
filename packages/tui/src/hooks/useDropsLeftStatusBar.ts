import { useTheme } from './useThemeContext.js';
import { useAppStoreOptional } from '../stores/app-store.js';

/**
 * Whether the current surface drops the left status bar (the full-height
 * accent-bar gutter) from message chrome, rendering content flush to column 0.
 *
 * Defaults to TUI chrome (bar present) without an AppStoreContext — Storybook
 * and snapshot tests.
 */
export function useDropsLeftStatusBar(): boolean {
  const { wrapDisabled } = useTheme();
  // Live read so a /tui swap restores chrome on later rows.
  const isLiteUi = useAppStoreOptional((s) => s.uiMode === 'lite', false);
  return wrapDisabled || isLiteUi;
}

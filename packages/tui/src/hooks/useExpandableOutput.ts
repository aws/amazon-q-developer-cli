import { useEffect, useContext } from 'react';
import { useStore } from 'zustand';
import { AppStoreContext } from '../stores/app-store.js';
import { useStatusBar } from '../components/chat/status-bar/StatusBar.js';

export interface UseExpandableOutputOptions {
  /** Total number of items */
  totalItems: number;
  /** Number of items to show in preview */
  previewCount: number;
  /** Whether this is a static/past turn (no expandable output) */
  isStatic?: boolean;
  /** Unit label for hidden items (e.g., "lines", "files", "entries"). Defaults to "more" */
  unit?: string;
}

export interface UseExpandableOutputResult {
  /** Whether the output is currently expanded */
  expanded: boolean;
  /** Whether there are more items than the preview count */
  hasExpandableContent: boolean;
  /** Number of items hidden in collapsed view */
  hiddenCount: number;
  /** Hint text to show for expansion (e.g., "...+5 items (^O to expand)") */
  expandHint: string;
}

/**
 * Hook for managing expandable/collapsible output in tool components.
 * 
 * Handles:
 * - Reading expanded state from app store
 * - Registering expandable content with the store
 * - Requesting remeasure when expanded state changes
 * - Calculating hidden item counts
 */
export function useExpandableOutput({
  totalItems,
  previewCount,
  isStatic = false,
  unit = 'more',
}: UseExpandableOutputOptions): UseExpandableOutputResult {
  // Try to get StatusBar context for remeasure
  let statusBarContext: ReturnType<typeof useStatusBar> | null = null;
  try {
    statusBarContext = useStatusBar();
  } catch {
    // Not within a StatusBar context
  }

  const { requestRemeasure } = statusBarContext ?? {
    requestRemeasure: () => {},
  };

  // Safe store access - returns defaults when no store context
  const store = useContext(AppStoreContext);
  const expanded = store ? useStore(store, (state) => state.toolOutputsExpanded) : false;
  const setHasExpandableToolOutputs = store
    ? useStore(store, (state) => state.setHasExpandableToolOutputs)
    : () => {};

  const hasExpandableContent = totalItems > previewCount;
  const hiddenCount = Math.max(0, totalItems - previewCount);

  // Register that we have expandable output (only for active/non-static)
  useEffect(() => {
    if (hasExpandableContent && !isStatic) {
      setHasExpandableToolOutputs(true);
    }
  }, [hasExpandableContent, isStatic, setHasExpandableToolOutputs]);

  // Request remeasure when expanded state changes (only for active)
  useEffect(() => {
    if (!isStatic) {
      requestRemeasure();
    }
  }, [expanded, isStatic, requestRemeasure]);

  const expandHint = hiddenCount > 0 ? `...+${hiddenCount} ${unit} (ctrl+o to toggle)` : '';

  return {
    expanded,
    hasExpandableContent,
    hiddenCount,
    expandHint,
  };
}

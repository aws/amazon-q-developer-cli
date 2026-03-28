import { useEffect, useContext, useRef } from 'react';
import { useStore, createStore } from 'zustand';
import { AppStoreContext } from '../stores/app-store.js';
import { useStatusBar } from '../components/chat/status-bar/StatusBar.js';

// Fallback store when AppStoreContext is null (e.g. storybook, tests).
// Called unconditionally so hook count is stable.
const createNoopStore = () =>
  createStore(() => ({
    toolOutputsExpanded: false as boolean,
    setHasExpandableToolOutputs: (() => {}) as (v: boolean) => void,
  }));

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
 * - Preserving expand/collapse state when transitioning to static (history)
 */
export function useExpandableOutput({
  totalItems,
  previewCount,
  isStatic = false,
  unit = 'more',
}: UseExpandableOutputOptions): UseExpandableOutputResult {
  const statusBarContext = useStatusBar();
  const { requestRemeasure } = statusBarContext ?? {
    requestRemeasure: () => {},
  };

  const store = useContext(AppStoreContext);
  const NOOP_STORE = useRef(createNoopStore()).current;
  const activeStore = store ?? NOOP_STORE;
  const storeExpanded = useStore(
    activeStore,
    (state) => state.toolOutputsExpanded
  );
  const setHasExpandableToolOutputs = useStore(
    activeStore,
    (state) => state.setHasExpandableToolOutputs
  );

  // Snapshot the expanded state so it's preserved when transitioning to static.
  // While active, the ref tracks the live store value.
  // Once isStatic flips to true, the ref retains the last active value.
  const frozenExpanded = useRef(storeExpanded);
  if (!isStatic) {
    frozenExpanded.current = storeExpanded;
  }

  const expanded = isStatic ? frozenExpanded.current : storeExpanded;

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

  const expandHint =
    hiddenCount > 0 && !isStatic
      ? `...+${hiddenCount} ${unit} (ctrl+o to toggle)`
      : '';

  return {
    expanded,
    hasExpandableContent,
    hiddenCount,
    expandHint,
  };
}

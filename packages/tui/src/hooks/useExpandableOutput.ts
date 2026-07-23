import { useEffect, useContext, useRef } from 'react';
import { useStore, createStore } from 'zustand';
import { AppStoreContext } from '../stores/app-store.js';
import { useStatusBar } from '../components/chat/status-bar/StatusBar.js';
import { Settings } from '../constants/settings.js';
import { useVerboseDisplay } from './useVerbose.js';

// Fallback store when AppStoreContext is null (e.g. storybook, tests).
// Called unconditionally so hook count is stable.
const createNoopStore = () =>
  createStore(() => ({
    toolOutputsExpanded: false as boolean,
    setHasExpandableToolOutputs: (() => {}) as (v: boolean) => void,
    settings: null as Record<string, unknown> | null,
  }));

const expandableRegistrations = new WeakMap<object, number>();

function useActiveExpandableStore() {
  const store = useContext(AppStoreContext);
  const noopStore = useRef(createNoopStore()).current;
  return store ?? noopStore;
}

/** Keep the global Ctrl+O handler active while at least one mounted body needs it. */
export function useExpandableRegistration(active: boolean): void {
  const activeStore = useActiveExpandableStore();

  useEffect(() => {
    if (!active) return;
    expandableRegistrations.set(
      activeStore,
      (expandableRegistrations.get(activeStore) ?? 0) + 1
    );
    activeStore.getState().setHasExpandableToolOutputs(true);
    return () => {
      const remaining = Math.max(
        0,
        (expandableRegistrations.get(activeStore) ?? 1) - 1
      );
      if (remaining === 0) {
        expandableRegistrations.delete(activeStore);
        activeStore.getState().setHasExpandableToolOutputs(false);
      } else {
        expandableRegistrations.set(activeStore, remaining);
      }
    };
  }, [active, activeStore]);
}

export interface UseExpandableOutputOptions {
  /** Total number of items */
  totalItems: number;
  /** Number of items to show in preview */
  previewCount: number;
  /** Widest output item, used to detect character-only truncation. */
  maxContentWidth?: number;
  /** Whether this is a static/past turn (no expandable output) */
  isStatic?: boolean;
  /** Unit label for hidden items (e.g., "lines", "files", "entries"). Defaults to "more" */
  unit?: string;
  /**
   * Force the output fully expanded (like `autoExpand`): always open and not
   * registered as Ctrl+O-toggleable. Used by thinking's `expanded` mode.
   */
  forceExpanded?: boolean;
  /**
   * Opt in to the verbosity `outputMaxLines` cap overriding `previewCount`.
   * Only tool OUTPUT bodies want this; consumers like thinking (previewCount 0)
   * and the collapsed-tool entry (previewCount 1) must keep their own preview
   * size, so they leave this off and effectivePreviewCount === previewCount.
   */
  applyVerbosityOutputCap?: boolean;
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
  /** Collapsed preview size after the outputMaxLines override; slice by this. */
  effectivePreviewCount: number;
  /** Collapsed per-line char cap; null while expanded. */
  outputMaxChars: number | null;
  /** Whether finished tool output should stay in scrollback after the turn ends
   *  (verbosity `persistOutput`). Components read this in their `isStatic`
   *  branch: true → keep the (capped) body; false → collapse to a summary. */
  persistOutput: boolean;
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
  maxContentWidth = 0,
  isStatic = false,
  unit = 'more',
  forceExpanded = false,
  applyVerbosityOutputCap = false,
}: UseExpandableOutputOptions): UseExpandableOutputResult {
  const statusBarContext = useStatusBar();
  const { requestRemeasure } = statusBarContext ?? {
    requestRemeasure: () => {},
  };

  const activeStore = useActiveExpandableStore();
  const storeExpanded = useStore(
    activeStore,
    (state) => state.toolOutputsExpanded
  );

  // When the auto-expand setting is on, always show full output inline —
  // no truncation, no ctrl+o hints, no alternate read-only view.
  const autoExpandSetting = useStore(
    activeStore,
    (state) =>
      (
        state as Record<string, unknown> & {
          settings?: Record<string, unknown> | null;
        }
      ).settings?.[Settings.CHAT_AUTO_EXPAND_TOOL_OUTPUT] === true
  );
  // `forceExpanded` (thinking's `expanded` mode) behaves identically.
  const autoExpand = autoExpandSetting || forceExpanded;

  // When opted in, outputMaxLines fully defines the collapsed size. `null`
  // means unbounded, so legacy per-tool preview defaults must not re-cap it.
  const display = useVerboseDisplay();
  const verbosityCapActive =
    applyVerbosityOutputCap && process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';
  const effectivePreviewCount = verbosityCapActive
    ? (display.outputMaxLines ?? totalItems)
    : previewCount;
  // Snapshot the expanded state so it's preserved when transitioning to static.
  // While active, the ref tracks the live store value.
  // Once isStatic flips to true, the ref retains the last active value.
  const frozenExpanded = useRef(storeExpanded);
  if (!isStatic) {
    frozenExpanded.current = storeExpanded;
  }

  const expanded =
    autoExpand || (isStatic ? frozenExpanded.current : storeExpanded);
  const configuredMaxChars = verbosityCapActive ? display.outputMaxChars : null;
  const hasCharacterTruncation =
    configuredMaxChars != null &&
    configuredMaxChars > 0 &&
    maxContentWidth > configuredMaxChars;
  const outputMaxChars = expanded ? null : configuredMaxChars;
  const persistOutput = display.persistOutput;

  const hasLineTruncation = totalItems > effectivePreviewCount;
  const hasExpandableContent = hasLineTruncation || hasCharacterTruncation;
  const hiddenCount = Math.max(0, totalItems - effectivePreviewCount);

  useExpandableRegistration(hasExpandableContent && !isStatic && !autoExpand);

  // Request remeasure when expanded state changes (only for active)
  useEffect(() => {
    if (!isStatic) {
      requestRemeasure();
    }
  }, [expanded, isStatic, requestRemeasure]);

  let expandHint = '';
  if (!expanded && !autoExpand) {
    if (hiddenCount > 0) {
      if (!isStatic || verbosityCapActive) {
        expandHint = `...+${hiddenCount} ${unit}${isStatic ? '' : ' (ctrl+o to toggle)'}`;
      }
    } else if (hasCharacterTruncation && !isStatic) {
      expandHint = '... (ctrl+o to toggle)';
    }
  }

  return {
    expanded,
    hasExpandableContent,
    hiddenCount,
    expandHint,
    effectivePreviewCount,
    outputMaxChars,
    persistOutput,
  };
}

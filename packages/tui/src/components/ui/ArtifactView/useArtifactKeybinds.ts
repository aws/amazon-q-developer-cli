import { useKeypress } from '../../../hooks/useKeypress.js';
import { useUIActions, useUIState } from '../../../stores/selectors.js';
import { useAppStore } from '../../../stores/app-store.js';
import type { ArtifactKind } from '../../../stores/app-store.js';

/** Map keystroke → artifact kind for fast doc switching. */
const SWITCH_KEY_TO_ARTIFACT: Record<string, ArtifactKind> = {
  r: 'requirements',
  d: 'design',
  t: 'tasks',
};

/**
 * Mode-conditional keybinding handler for the ArtifactView panel.
 *
 * Only active when:
 *   - The panel is open
 *   - There is no pending approval prompt
 *   - There is no active slash-command (selection menu) consuming input
 *
 * The `q` key flows through to the chat prompt as text whenever the panel
 * is closed, because we gate the entire `useKeypress` listener on
 * `view.open !== null`.
 */
export function useArtifactKeybinds(): void {
  const { artifactViewOpen } = useUIState();
  const {
    closeArtifactView,
    moveArtifactCursor,
    toggleArtifactExpand,
    enterArtifactDetail,
  } = useUIActions();

  // `openArtifactView` isn't on `useUIActions` (it's surfaced through the
  // command-context for slash-command effects). Read it directly from the
  // store so we can swap docs in-place without prop-drilling or extending
  // the selector — same pattern this hook already uses for
  // `pendingApproval` and `activeCommand`.
  const openArtifactView = useAppStore((s) => s.openArtifactView);

  // Cross-cutting state that should defer to other input owners. Reading
  // these from the store directly avoids prop-drilling.
  const pendingApproval = useAppStore((s) => s.pendingApproval);
  const activeCommand = useAppStore((s) => s.activeCommand);

  const isActive =
    artifactViewOpen !== null && !pendingApproval && !activeCommand;

  useKeypress(
    (input, key) => {
      if (!artifactViewOpen) return;

      // Close handler: 'q' in summary mode, escape from detail (handled below).
      if (input === 'q' && !key.ctrl && !key.meta) {
        closeArtifactView();
        return;
      }

      if (artifactViewOpen.mode === 'summary') {
        // Doc-switch shortcuts: r / d / t swap the panel between
        // requirements / design / tasks for the currently-open feature.
        // No-op when the requested kind is already shown. The store's
        // `openArtifactView` handles missing-file errors gracefully.
        const switchTo = SWITCH_KEY_TO_ARTIFACT[input];
        if (switchTo && !key.ctrl && !key.meta) {
          if (switchTo !== artifactViewOpen.artifact) {
            void openArtifactView(artifactViewOpen.featureName, switchTo);
          }
          return;
        }

        if (key.upArrow) {
          moveArtifactCursor('prev');
          return;
        }
        if (key.downArrow) {
          moveArtifactCursor('next');
          return;
        }
        if (key.rightArrow) {
          // Tasks-only meaningfully expand; for other kinds this is a
          // no-op at the slice level.
          toggleArtifactExpand(artifactViewOpen.cursor);
          return;
        }
        if (key.leftArrow) {
          // Collapse: store-level toggle handles "currently expanded → false";
          // when already collapsed it becomes true, so we explicitly check
          // the expanded map and only toggle when expanded.
          if (artifactViewOpen.expanded[artifactViewOpen.cursor]) {
            toggleArtifactExpand(artifactViewOpen.cursor);
          }
          return;
        }
        if (key.return) {
          enterArtifactDetail();
          return;
        }
      } else {
        // Detail mode: Esc/back is owned by Panel.onClose (which we wire
        // mode-aware in ArtifactView), so we explicitly do nothing here.
        // Up/down/left/right are intentionally ignored per spec.
      }
    },
    { isActive }
  );
}

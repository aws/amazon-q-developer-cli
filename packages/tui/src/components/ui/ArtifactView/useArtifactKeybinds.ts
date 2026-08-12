import { useContext } from 'react';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { useUIActions, useUIState } from '../../../stores/selectors.js';
import {
  AppStoreContext,
  commentsForDocument,
  useAppStore,
} from '../../../stores/app-store.js';
import type { AppState, ArtifactKind } from '../../../stores/app-store.js';
import { findSpecFeature } from '../../../utils/spec-workspace.js';
import {
  resumeSpecFeature,
  sendSpecRevision,
} from '../../../commands/effects.js';
import { logger } from '../../../utils/logger.js';
import { decideSend } from '../../../utils/spec-review/send-decision.js';
import { workflowStages } from '../../../utils/spec-workflow.js';
import { detailBodyAt } from '../../../utils/spec-artifact-parser/index.js';
import type { ReviewAction } from '../../../utils/spec-review/review-actions.js';

/** Stable empty list, so the staged-comments selector doesn't churn renders. */
const NO_STAGED: ReviewAction[] = [];

/** Map keystroke → artifact kind for fast doc switching. */
export const SWITCH_KEY_TO_ARTIFACT: Record<string, ArtifactKind> = {
  r: 'requirements',
  d: 'design',
  t: 'tasks',
  b: 'bugfix',
};

/** The key that switches to `artifact`, for advertising it in the footer. */
export function switchKeyFor(artifact: ArtifactKind): string {
  const entry = Object.entries(SWITCH_KEY_TO_ARTIFACT).find(
    ([, kind]) => kind === artifact
  );
  return (entry?.[0] ?? '').toUpperCase();
}

/**
 * Whether a message sent now would be queued rather than delivered as written.
 *
 * A queued send keeps only the text the transcript shows, so a revision request
 * would lose its quotes and its instruction while still looking sent.
 */
function agentIsBusy(state: AppState): boolean {
  return (
    !state.isInitialized ||
    state.isProcessing ||
    state.isCompacting ||
    !!state.loadingMessage
  );
}

/**
 * Keybinding handler for the ArtifactView panel.
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
  const { closeArtifactView, moveArtifactCursor, toggleArtifactExpand } =
    useUIActions();

  // `openArtifactView` isn't on `useUIActions` (it's surfaced through the
  // command-context for slash-command effects). Read it directly from the
  // store so we can swap docs in-place without prop-drilling or extending
  // the selector — same pattern this hook already uses for
  // `pendingApproval` and `activeCommand`.
  const openArtifactView = useAppStore((s) => s.openArtifactView);
  const openSpecReview = useAppStore((s) => s.openSpecReview);
  const clearSpecReview = useAppStore((s) => s.clearSpecReview);
  const staged = useAppStore((s) =>
    artifactViewOpen
      ? commentsForDocument(
          s,
          artifactViewOpen.featureName,
          artifactViewOpen.artifact
        )
      : NO_STAGED
  );
  // Read live rather than through a selector: the busy check has to hold at the
  // moment of sending, and the send awaits a mode switch first.
  const storeApi = useContext(AppStoreContext)!;

  // Cross-cutting state that should defer to other input owners. Reading
  // these from the store directly avoids prop-drilling.
  const pendingApproval = useAppStore((s) => s.pendingApproval);
  const activeCommand = useAppStore((s) => s.activeCommand);

  // Dependencies for the `c` resume keybind. Bound here (not inside the
  // handler) so the closure stays stable; the handler itself is held in
  // `useKeypress`'s ref so identity churn doesn't matter, but reading
  // through `useAppStore` selectors keeps it React-friendly.
  const kiro = useAppStore((s) => s.kiro);
  const setCurrentAgent = useAppStore((s) => s.setCurrentAgent);
  const sendMessage = useAppStore((s) => s.sendMessage);
  const showTransientAlert = useAppStore((s) => s.showTransientAlert);

  const isActive =
    artifactViewOpen !== null && !pendingApproval && !activeCommand;

  useKeypress(
    (input, key) => {
      if (!artifactViewOpen) return;

      // 'q' closes the panel from anywhere in it.
      if (input === 'q' && !key.ctrl && !key.meta) {
        closeArtifactView();
        return;
      }

      // Doc-switch shortcuts swap the panel between this feature's documents.
      // Only the ones its workflow writes: an unadvertised key is a no-op rather
      // than a load that misses and leaves the panel in an error state.
      const switchTo = SWITCH_KEY_TO_ARTIFACT[input];
      if (switchTo && !key.ctrl && !key.meta) {
        const stages = workflowStages(
          artifactViewOpen.workflow.workflowType,
          artifactViewOpen.workflow.specType
        );
        if (
          switchTo !== artifactViewOpen.artifact &&
          stages.includes(switchTo)
        ) {
          void openArtifactView(artifactViewOpen.featureName, switchTo);
        }
        return;
      }

      // `c` — Continue work on this spec. Switches to spec mode and
      // sends the same "Continue working on …" prompt the legacy
      // `/spec <name>` resume path used. We close the panel first so
      // the chat output is visible, then dispatch.
      if (input === 'c' && !key.ctrl && !key.meta) {
        const featureName = artifactViewOpen.featureName;
        const feature = findSpecFeature(process.cwd(), featureName);
        if (!feature) {
          // Race: feature deleted while panel was open. Surface a
          // transient and don't close — let the user dismiss with Q.
          showTransientAlert({
            message: `No spec found at .kiro/specs/${featureName}/`,
            status: 'error',
            autoHideMs: 5000,
          });
          return;
        }
        closeArtifactView();
        void resumeSpecFeature(
          {
            kiro,
            setCurrentAgent,
            sendMessage,
            showAlert: (message, status, autoHideMs) =>
              showTransientAlert({ message, status, autoHideMs }),
          },
          feature
        ).catch((err) => {
          logger.error('[artifact-view] resume threw', {
            err: err instanceof Error ? err.message : String(err),
          });
        });
        return;
      }

      // `s` — send the comments staged against this document.
      if (input === 's' && !key.ctrl && !key.meta && staged.length > 0) {
        const busy = agentIsBusy(storeApi.getState());
        const decision = decideSend(artifactViewOpen.artifact, staged, {
          busy,
        });
        if (decision.kind === 'nothing') return;
        if (decision.kind === 'refuse') {
          showTransientAlert({
            message: decision.message,
            status: 'warning',
            autoHideMs: 5000,
          });
          return;
        }
        const { featureName } = artifactViewOpen;
        const document = decision.document;
        closeArtifactView();
        void sendSpecRevision(
          {
            kiro,
            setCurrentAgent,
            sendMessage,
            isBusy: () => agentIsBusy(storeApi.getState()),
            showAlert: (message, status, autoHideMs) =>
              showTransientAlert({ message, status, autoHideMs }),
          },
          decision.request,
          decision.summary,
          () => clearSpecReview(featureName, document)
        ).catch((err) => {
          logger.error('[artifact-view] sending the revision threw', {
            err: err instanceof Error ? err.message : String(err),
          });
        });
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
        // Only kinds whose rows have children expand; for the rest the
        // store-level toggle is a no-op.
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
        // Open the document itself, landing on the item under the cursor. The
        // surface covers this panel rather than replacing it, so its Esc brings
        // the summary back — and staged comments stay parked per document, so
        // opening another one never discards them.
        void openSpecReview(
          artifactViewOpen.featureName,
          artifactViewOpen.artifact,
          detailBodyAt(artifactViewOpen.summary, artifactViewOpen.cursor)
        ).catch((err) => {
          logger.error('[artifact-view] opening the review threw', {
            err: err instanceof Error ? err.message : String(err),
          });
        });
        return;
      }
    },
    { isActive }
  );
}

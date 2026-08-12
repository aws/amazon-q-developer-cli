import React from 'react';
import { Panel } from '../panel/Panel.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { SummaryView } from './SummaryView.js';
import { ErrorState } from './ErrorState.js';
import { StageBar } from './StageBar.js';
import { useArtifactKeybinds, switchKeyFor } from './useArtifactKeybinds.js';
import { workflowStages } from '../../../utils/spec-workflow.js';
import { useUIState, useUIActions } from '../../../stores/selectors.js';
import { commentsForDocument, useAppStore } from '../../../stores/app-store.js';
import { commentCount } from '../../../utils/spec-review/review-actions.js';
import type { OpenArtifactView } from '../../../stores/app-store.js';

/**
 * Top-level Spec Artifact view panel.
 *
 * Renders nothing when `artifactViewOpen` is null — the panel must be
 * conditionally mounted by the parent layout, but the component is
 * resilient to being rendered with no state (returns null).
 */
export const ArtifactView: React.FC = () => {
  const { artifactViewOpen } = useUIState();
  const { closeArtifactView } = useUIActions();
  // Wire up keybindings unconditionally; the hook gates on `view.open`.
  useArtifactKeybinds();

  if (!artifactViewOpen) return null;

  const title = `/spec view ${artifactViewOpen.featureName} ${artifactViewOpen.artifact}`;

  // Panel owns Esc (via `useInput` keyed on `closeMenu`) and it closes the
  // panel. Returning from the review surface to this summary is a level above:
  // the surface covers the panel, so its own Esc puts the panel back on screen
  // without this handler ever seeing the keystroke.
  return (
    <Panel
      title={title}
      onClose={closeArtifactView}
      hideTitleDivider={false}
      closeHintLabel="close"
      footerLeft={<ArtifactFooterHints view={artifactViewOpen} />}
    >
      <StageBar
        workflow={artifactViewOpen.workflow}
        current={artifactViewOpen.artifact}
        featureName={artifactViewOpen.featureName}
      />
      {artifactViewOpen.error ? (
        <ErrorState message={artifactViewOpen.error.message} />
      ) : (
        <SummaryView view={artifactViewOpen} />
      )}
    </Panel>
  );
};

const ArtifactFooterHints: React.FC<{ view: OpenArtifactView }> = ({
  view,
}) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const dim = getColor('secondary');
  const primary = getColor('primary');
  const success = getColor('success');
  const staged = useAppStore(
    (s) => commentsForDocument(s, view.featureName, view.artifact).length
  );
  // Error mode: Esc/Q close. Panel renders the Esc hint; nothing extra
  // for us to surface here.
  if (view.error) {
    return null;
  }
  // Rows that can hold children advertise the key that reveals them; the arrow
  // marker on those rows is otherwise a promise with nothing behind it.
  const expandable =
    view.summary.kind === 'tasks' || view.summary.kind === 'bugfix';
  // Only the documents this spec's workflow actually writes: a bugfix spec has
  // no requirements.md to switch to, and a feature spec has no bugfix.md.
  const switchKeys = workflowStages(
    view.workflow.workflowType,
    view.workflow.specType
  )
    .map(switchKeyFor)
    .join('/');
  return (
    <Text>
      {primary(`${glyphs.arrowUp}${glyphs.arrowDown}`)}{' '}
      {dim(`move ${glyphs.smallDot} `)}
      {primary('Enter')} {dim('open')}
      {expandable ? (
        <>
          {dim(` ${glyphs.smallDot} `)}
          {primary(`${glyphs.arrow} ${glyphs.arrowLeft}`)} {dim('expand')}
        </>
      ) : null}
      {dim(` ${glyphs.smallDot} `)}
      {primary(switchKeys)} {dim('switch')}
      {dim(` ${glyphs.smallDot} `)}
      {primary('C')} {dim('continue')}
      {staged > 0 && (
        <>
          {dim(` ${glyphs.smallDot} `)}
          {primary('S')} {success(`send ${commentCount(staged)}`)}
        </>
      )}
    </Text>
  );
};

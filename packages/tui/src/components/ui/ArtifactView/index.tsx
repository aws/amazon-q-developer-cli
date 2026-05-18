import React from 'react';
import { Box } from './../../../renderer.js';
import { Panel } from '../panel/Panel.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { SummaryView } from './SummaryView.js';
import { DetailView } from './DetailView.js';
import { ErrorState } from './ErrorState.js';
import { StageBar } from './StageBar.js';
import { useArtifactKeybinds } from './useArtifactKeybinds.js';
import { useUIState, useUIActions } from '../../../stores/selectors.js';
import type { OpenArtifactView } from '../../../stores/app-store.js';

const ARTIFACT_LABELS: Record<OpenArtifactView['artifact'], string> = {
  requirements: 'Requirements',
  design: 'Design',
  tasks: 'Tasks',
};

/**
 * Top-level Spec Artifact view panel.
 *
 * Renders nothing when `artifactViewOpen` is null — the panel must be
 * conditionally mounted by the parent layout, but the component is
 * resilient to being rendered with no state (returns null).
 */
export const ArtifactView: React.FC = () => {
  const { artifactViewOpen } = useUIState();
  const { closeArtifactView, leaveArtifactDetail } = useUIActions();
  // Wire up keybindings unconditionally; the hook gates on `view.open`.
  useArtifactKeybinds();

  if (!artifactViewOpen) return null;

  const title = `/spec view ${artifactViewOpen.featureName} ${artifactViewOpen.artifact}`;

  // Panel listens for Esc internally (via `useInput` keyed on `closeMenu`).
  // We make that one keystroke do the right thing for the current mode:
  // in detail mode it returns to the summary; in summary or error mode
  // it closes the whole panel. This is the *only* Esc handler — the hook
  // intentionally no longer listens for Esc, to avoid the dual-handler
  // race that previously closed the panel before the mode-change took
  // effect.
  const inDetail =
    artifactViewOpen.mode === 'detail' && !artifactViewOpen.error;
  const handleClose = inDetail ? leaveArtifactDetail : closeArtifactView;
  const closeHintLabel = inDetail ? 'back' : 'close';

  return (
    <Panel
      title={title}
      onClose={handleClose}
      hideTitleDivider={false}
      closeHintLabel={closeHintLabel}
      footerLeft={<ArtifactFooterHints view={artifactViewOpen} />}
    >
      <StageBar
        workflow={artifactViewOpen.workflow}
        current={artifactViewOpen.artifact}
      />
      {artifactViewOpen.error ? (
        <ErrorState message={artifactViewOpen.error.message} />
      ) : artifactViewOpen.mode === 'summary' ? (
        <SummaryView view={artifactViewOpen} />
      ) : (
        <DetailModeFrame view={artifactViewOpen} />
      )}
    </Panel>
  );
};

const ArtifactFooterHints: React.FC<{ view: OpenArtifactView }> = ({
  view,
}) => {
  const { getColor } = useTheme();
  const dim = getColor('secondary');
  const primary = getColor('primary');
  // Error mode: Esc/Q close. Panel renders the Esc hint; nothing extra
  // for us to surface here.
  if (view.error) {
    return null;
  }
  if (view.mode === 'detail') {
    // Esc-back is rendered by Panel (closeHintLabel='back'). We add a
    // Q-close shortcut so users have a one-press exit from any mode.
    return (
      <Text>
        {primary('Q')} {dim('close')}
      </Text>
    );
  }
  // summary
  const includeRightLeft = view.summary.kind === 'tasks';
  return (
    <Text>
      {primary('↑↓')} {dim('move · ')}
      {primary('Enter')} {dim('detail')}
      {includeRightLeft ? (
        <>
          {dim(' · ')}
          {primary('→ ←')} {dim('expand')}
        </>
      ) : null}
      {dim(' · ')}
      {primary('R/D/T')} {dim('switch')}
      {dim(' · ')}
      {primary('C')} {dim('continue')}
      {dim(' · ')}
      {primary('Q')} {dim('close')}
    </Text>
  );
};

const DetailModeFrame: React.FC<{ view: OpenArtifactView }> = ({ view }) => {
  const { getColor } = useTheme();
  const dim = getColor('secondary');
  const detailLabel = (() => {
    const summary = view.summary;
    if (summary.kind === 'requirements') {
      const item = summary.items[view.cursor];
      return item ? `${ARTIFACT_LABELS.requirements} ${item.number}` : '';
    }
    if (summary.kind === 'design') {
      const section = summary.sections[view.cursor];
      return section ? section.title : '';
    }
    const task = summary.items[view.cursor];
    return task ? (task.number ? `${task.number}. ${task.title}` : task.title) : '';
  })();

  return (
    <Box flexDirection="column">
      {detailLabel.length > 0 && (
        <Box marginBottom={1}>
          <Text>{dim('Detail · ')}</Text>
          <Text>{detailLabel}</Text>
        </Box>
      )}
      <DetailView view={view} />
    </Box>
  );
};

import React from 'react';
import { Box } from './../../../renderer.js';
import { Panel } from '../panel/Panel.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { SummaryView } from './SummaryView.js';
import { DetailView } from './DetailView.js';
import { ErrorState } from './ErrorState.js';
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
  const { closeArtifactView } = useUIActions();
  // Wire up keybindings unconditionally; the hook gates on `view.open`.
  useArtifactKeybinds();

  if (!artifactViewOpen) return null;

  const title = `/spec view ${artifactViewOpen.featureName} ${artifactViewOpen.artifact}`;

  return (
    <Panel
      title={title}
      onClose={closeArtifactView}
      hideTitleDivider={false}
      footerLeft={
        <ArtifactFooterHints view={artifactViewOpen} />
      }
    >
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
  if (view.error) {
    return <Text>{dim('Press Q to close')}</Text>;
  }
  if (view.mode === 'detail') {
    return (
      <Text>
        {primary('Esc')} {dim('back · ')}
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

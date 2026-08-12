import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { workflowStages } from '../../../utils/spec-workflow.js';
import { commentsForDocument, useAppStore } from '../../../stores/app-store.js';
import type { ArtifactKind } from '../../../utils/spec-artifact-loader.js';
import type { SpecConfig } from '../../../utils/spec-config.js';

interface Props {
  workflow: SpecConfig;
  current: ArtifactKind;
  featureName: string;
}

const STAGE_LABELS: Record<ArtifactKind, string> = {
  requirements: 'Requirements',
  design: 'Design',
  tasks: 'Tasks',
  bugfix: 'Bug analysis',
};

/**
 * Thin one-row bar showing the artifact pipeline for the current
 * feature, with the active stage highlighted. The order reflects the
 * feature's `.config.kiro` `workflowType` (defaults to
 * `requirements-first`).
 *
 * A stage also carries the number of comments staged against its document, so
 * comments parked on a document the user has navigated away from stay visible —
 * sending is per-document, and this is what says which document to go back to.
 *
 * Pure presentation: clicking is not supported. Stage swaps happen via the
 * document-switch keybinds wired in `useArtifactKeybinds`.
 */
export const StageBar: React.FC<Props> = ({
  workflow,
  current,
  featureName,
}) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const accent = getColor('accent');
  const dim = getColor('secondary');
  const success = getColor('success');
  const staged = useAppStore((s) => s.specReviewComments);

  const stages = workflowStages(workflow.workflowType, workflow.specType);
  // `arrowRight` is a glyph used elsewhere in the TUI (task expansion
  // collapse indicator). Reusing it keeps Unicode/ASCII fallback
  // behaviour consistent.
  const sep = ` ${glyphs.arrowRight} `;

  return (
    <Box paddingX={1}>
      {stages.map((stage, i) => {
        const isCurrent = stage === current;
        const label = STAGE_LABELS[stage];
        const count = commentsForDocument(
          { specReviewComments: staged },
          featureName,
          stage
        ).length;
        const decorated = isCurrent
          ? accent.bold(`[${label}`)
          : dim(`[${label}`);
        return (
          <React.Fragment key={stage}>
            {i > 0 ? <Text>{dim(sep)}</Text> : null}
            <Text>{decorated}</Text>
            {count > 0 ? (
              <Text>{success(` ${glyphs.smallDot}${count}`)}</Text>
            ) : null}
            <Text>{isCurrent ? accent.bold(']') : dim(']')}</Text>
          </React.Fragment>
        );
      })}
    </Box>
  );
};

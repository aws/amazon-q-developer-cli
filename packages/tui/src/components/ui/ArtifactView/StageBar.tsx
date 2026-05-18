import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { workflowStages } from '../../../utils/spec-workflow.js';
import type { ArtifactKind } from '../../../utils/spec-artifact-loader.js';
import type { SpecConfig } from '../../../utils/spec-config.js';

interface Props {
  workflow: SpecConfig;
  current: ArtifactKind;
}

const STAGE_LABELS: Record<ArtifactKind, string> = {
  requirements: 'Requirements',
  design: 'Design',
  tasks: 'Tasks',
};

/**
 * Thin one-row bar showing the artifact pipeline for the current
 * feature, with the active stage highlighted. The order reflects the
 * feature's `.config.kiro` `workflowType` (defaults to
 * `requirements-first`).
 *
 * Pure presentation: clicking is not supported. Stage swaps happen via
 * the `r` / `d` / `t` keybinds wired in `useArtifactKeybinds`.
 */
export const StageBar: React.FC<Props> = ({ workflow, current }) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const accent = getColor('accent');
  const dim = getColor('secondary');

  const stages = workflowStages(workflow.workflowType);
  // `arrowRight` is a glyph used elsewhere in the TUI (task expansion
  // collapse indicator). Reusing it keeps Unicode/ASCII fallback
  // behaviour consistent.
  const sep = ` ${glyphs.arrowRight} `;

  return (
    <Box paddingX={1}>
      {stages.map((stage, i) => {
        const isCurrent = stage === current;
        const label = STAGE_LABELS[stage];
        const decorated = isCurrent
          ? accent.bold(`[${label}]`)
          : dim(`[${label}]`);
        return (
          <React.Fragment key={stage}>
            {i > 0 ? <Text>{dim(sep)}</Text> : null}
            <Text>{decorated}</Text>
          </React.Fragment>
        );
      })}
    </Box>
  );
};

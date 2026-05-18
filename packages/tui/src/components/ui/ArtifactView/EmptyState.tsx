import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import type { ArtifactKind } from '../../../utils/spec-artifact-loader.js';

interface Props {
  kind: ArtifactKind;
}

export const EmptyState: React.FC<Props> = ({ kind }) => {
  const { getColor } = useTheme();
  const dim = getColor('secondary');
  const messages: Record<ArtifactKind, string> = {
    requirements:
      'No requirements found. Look for blocks beginning with "### Requirement N:".',
    design:
      'No design sections found. Add H2 headings (## Title) to populate this view.',
    tasks: 'No tasks found. Add checkbox items like "- [ ] 1. Title".',
  };
  return (
    <Box marginTop={1} marginBottom={1}>
      <Text>{dim(messages[kind])}</Text>
    </Box>
  );
};

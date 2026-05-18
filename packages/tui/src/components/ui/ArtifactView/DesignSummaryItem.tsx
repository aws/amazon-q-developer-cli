import React from 'react';
import { Box, CURSOR_MARKER } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import type { DesignSection } from '../../../utils/spec-artifact-parser/index.js';

interface Props {
  section: DesignSection;
  selected: boolean;
}

export const DesignSummaryItem: React.FC<Props> = ({ section, selected }) => {
  const { getColor } = useTheme();
  const primary = getColor('primary');
  const accent = getColor('accent');

  const cursor = selected ? CURSOR_MARKER : ' ';
  return (
    <Box>
      <Text>{cursor}</Text>
      <Text>
        {selected ? accent.bold(section.title) : primary(section.title)}
      </Text>
    </Box>
  );
};

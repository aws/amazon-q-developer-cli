import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { cursorColumn } from './cursorColumn.js';
import type { DesignSection } from '../../../utils/spec-artifact-parser/index.js';

interface Props {
  section: DesignSection;
  selected: boolean;
}

export const DesignSummaryItem: React.FC<Props> = ({ section, selected }) => {
  const { getColor } = useTheme();
  const primary = getColor('primary');
  const accent = getColor('accent');

  const cursor = cursorColumn(selected);
  return (
    <Box>
      <Text>{cursor}</Text>
      <Text>
        {selected ? accent.bold(section.title) : primary(section.title)}
      </Text>
    </Box>
  );
};

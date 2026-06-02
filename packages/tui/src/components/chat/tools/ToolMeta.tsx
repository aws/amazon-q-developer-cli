import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';

const LEFT_MARGIN = 2;

export interface ToolMetaProps {
  params: string[] | null;
}

/** Renders params below a tool's StatusInfo header, inline with wrapping */
export const ToolMeta = React.memo(function ToolMeta({
  params,
}: ToolMetaProps) {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();

  if (!params || params.length === 0) return null;

  const color = getColor('muted');

  return (
    <Box marginLeft={LEFT_MARGIN}>
      <Text wrap="wrap">
        {color(`${glyphs.cornerBottomLeftRound} `)}
        {color(params.join(', '))}
      </Text>
    </Box>
  );
});

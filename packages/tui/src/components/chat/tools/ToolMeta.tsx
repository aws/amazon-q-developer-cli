import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

/** Prefix icon (downward-and-rightward pipe) */
const PREFIX = '╰ ';
const LEFT_MARGIN = 2;

export interface ToolMetaProps {
  params: string[] | null;
}

/** Renders params below a tool's StatusInfo header, inline with wrapping */
export const ToolMeta = React.memo(function ToolMeta({
  params,
}: ToolMetaProps) {
  const { getColor } = useTheme();

  if (!params || params.length === 0) return null;

  const color = getColor('muted');

  return (
    <Box marginLeft={LEFT_MARGIN}>
      <Text wrap="wrap">
        {color(PREFIX)}
        {color(params.join(', '))}
      </Text>
    </Box>
  );
});

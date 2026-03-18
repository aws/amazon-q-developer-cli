import React, { useMemo } from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

/** Prefix icon (downward-and-rightward pipe) */
const PREFIX = '╰ ';
const SEP = ', ';
const LEFT_MARGIN = 2;

export interface ToolMetaProps {
  params: string[] | null;
}

/** Renders params below a tool's StatusInfo header, inline with wrapping */
export const ToolMeta = React.memo(function ToolMeta({
  params,
}: ToolMetaProps) {
  const { getColor } = useTheme();

  const termWidth = process.stdout.columns || 80;
  const availableWidth = termWidth - LEFT_MARGIN - 4;

  // Group params into lines that fit within terminal width
  const lines = useMemo(() => {
    if (!params || params.length === 0) return [];

    const result: string[][] = [];
    let currentLine: string[] = [];
    // First line has the prefix, subsequent lines are indented to align
    let currentWidth = PREFIX.length;

    for (const param of params) {
      if (currentLine.length === 0) {
        currentLine.push(param);
        currentWidth += param.length;
      } else {
        const widthWithSep = currentWidth + SEP.length + param.length;
        if (widthWithSep <= availableWidth) {
          currentLine.push(param);
          currentWidth = widthWithSep;
        } else {
          result.push(currentLine);
          currentLine = [param];
          currentWidth = PREFIX.length + param.length;
        }
      }
    }
    if (currentLine.length > 0) {
      result.push(currentLine);
    }
    return result;
  }, [params, availableWidth]);

  if (lines.length === 0) return null;

  const color = getColor('muted');

  return (
    <>
      {lines.map((lineParams, i) => (
        <Box key={i} marginLeft={LEFT_MARGIN}>
          <Text>
            {color(i === 0 ? PREFIX : '  ')}
            {color(lineParams.join(SEP))}
          </Text>
        </Box>
      ))}
    </>
  );
});

import React from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import type { ToolDenial } from '../../utils/tool-denial.js';

export interface ToolDenialDetailsProps {
  denial: ToolDenial;
}

/**
 * Inline "Blocked by …" detail for a gated tool call, rendered under the tool
 * card. Parity with the IDE's SafetyDenialDetails / PolicyDenialDetails cards:
 * a "Blocked by <source>" header plus Rule and (when known) Tool rows.
 *
 * The distinction between an infra-safety block and a permission-policy denial
 * is already resolved in {@link deriveToolDenial}; this component only renders
 * the normalized {@link ToolDenial}.
 */
export const ToolDenialDetails = React.memo(function ToolDenialDetails({
  denial,
}: ToolDenialDetailsProps) {
  const { getColor } = useTheme();
  const error = getColor('error');
  const secondary = getColor('secondary');

  return (
    <Box flexDirection="column" marginTop={1}>
      <Text>
        {error('Blocked')} by {denial.source}
      </Text>
      <Box marginTop={1}>
        <Text>{secondary('Rule')} </Text>
        <Text>{denial.rule}</Text>
      </Box>
      {denial.tool && (
        <Box>
          <Text>{secondary('Tool')} </Text>
          <Text>{denial.tool}</Text>
        </Box>
      )}
    </Box>
  );
});

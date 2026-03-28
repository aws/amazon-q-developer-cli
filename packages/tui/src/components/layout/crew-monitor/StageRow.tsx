import React from 'react';
import { Box, Text } from '../../../renderer.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { getAgentColor } from '../../../utils/agentColors.js';
import { SpinnerIcon } from './SpinnerIcon.js';
import type { Stage } from './types.js';
import { truncate } from './types.js';

export const StageRow = React.memo(function StageRow({
  stage,
  index,
  isSelected,
  hasPendingApproval,
  nameW,
  agentNameW,
  depLabel,
  depW,
}: {
  stage: Stage;
  index: number;
  isSelected: boolean;
  hasPendingApproval: boolean;
  nameW: number;
  agentNameW: number;
  depLabel: string;
  depW: number;
}) {
  const { getColor } = useTheme();
  const agentColor = getAgentColor(stage.agentName, getColor);

  const statusText = stage.activeStatus ?? '';
  const statusColor = hasPendingApproval
    ? 'yellow'
    : stage.state === 'Completed'
      ? getColor('success').hex
      : stage.state === 'Failed'
        ? getColor('error').hex
        : 'gray';

  // Dependency arrow column: "←2,3" or empty
  const depCol = depLabel
    ? `←${depLabel}`.padEnd(depW + 2)
    : ' '.repeat(depW + 2);

  return (
    <Box
      paddingX={1}
      overflow="hidden"
      backgroundColor={isSelected ? 'gray' : undefined}
      gap={2}
    >
      <Box flexDirection="row" gap={1} flexShrink={0}>
        <Text color="gray">{index.toString().padStart(2)}</Text>
        <SpinnerIcon state={stage.state} />
      </Box>
      <Box width={nameW} flexShrink={0}>
        <Text bold={isSelected} underline={isSelected} wrap="truncate">
          {getColor('primary')(truncate(stage.name, nameW))}
        </Text>
      </Box>
      <Box width={agentNameW} flexShrink={0}>
        <Text wrap="truncate">
          {agentColor(truncate(stage.agentName, agentNameW))}
        </Text>
      </Box>
      {depW > 0 && (
        <Box width={depW + 2} flexShrink={0}>
          <Text color="gray">{depCol}</Text>
        </Box>
      )}
      <Text color={statusColor}>{statusText}</Text>
    </Box>
  );
});

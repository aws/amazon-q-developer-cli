import React from 'react';
import { Box, Text } from '../../../renderer.js';

export const CrewFooter = React.memo(function CrewFooter({
  hasExecutingSelected,
}: {
  hasExecutingSelected: boolean;
}) {
  return (
    <Box paddingX={1}>
      {hasExecutingSelected && <Text color="gray">^x kill session · </Text>}
      <Text color="gray">q/^g back</Text>
    </Box>
  );
});

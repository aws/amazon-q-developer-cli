import React from 'react';
import { Box, Text } from '../../../renderer.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';

export const CrewFooter = React.memo(function CrewFooter({
  hasExecutingSelected,
}: {
  hasExecutingSelected: boolean;
}) {
  const glyphs = useGlyphs();
  return (
    <Box paddingX={1}>
      {hasExecutingSelected && (
        <Text color="gray">^x kill session {glyphs.smallDot} </Text>
      )}
      <Text color="gray">q/^g back</Text>
    </Box>
  );
});

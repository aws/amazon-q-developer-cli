import React from 'react';
import { useStore } from 'zustand';
import { Box, Text } from '../../../renderer.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { workflowStore } from '../../../stores/workflow-store.js';

export const CrewFooter = React.memo(function CrewFooter({
  hasExecutingSelected,
  canKill = true,
}: {
  hasExecutingSelected: boolean;
  canKill?: boolean;
}) {
  const glyphs = useGlyphs();
  const hasWorkflow = useStore(
    workflowStore,
    (state) => state.workflows.size > 0
  );
  return (
    <Box paddingX={1}>
      {hasExecutingSelected && canKill && (
        <Text color="gray">ctrl+x kill session {glyphs.smallDot} </Text>
      )}
      <Text color="gray">
        {hasWorkflow ? `Tab workflows ${glyphs.smallDot} ` : ''}q/ctrl+g back
      </Text>
    </Box>
  );
});

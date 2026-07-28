import React from 'react';
import { Box } from '../../renderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { useAppStore } from '../../stores/app-store.js';
import type { SpecCheckpointPhase } from '../../types/agent-events.js';

const PHASE_LABEL: Record<SpecCheckpointPhase, string> = {
  requirements: 'Requirements',
  design: 'Design',
  tasks: 'Tasks',
};

/**
 * Marks the spec phase whose document the agent just finished.
 *
 * Shown only alongside the agent's check-in question: the notification arrives
 * when the document is first written, but the phase isn't settled until the
 * agent stops and asks (requirements, for instance, are still being refined by
 * detailer sub-agents after that first write).
 */
export const SpecCheckpointChip: React.FC<{
  /** Whether the check-in question's panel is on screen, decided by the layout
   *  that renders it — the marker must appear and vanish with it. */
  questionVisible: boolean;
}> = ({ questionVisible }) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const checkpoint = useAppStore((s) => s.specPhaseCheckpoint);
  if (!checkpoint || !questionVisible) return null;

  const phase = PHASE_LABEL[checkpoint.phase];

  return (
    <Box marginTop={1}>
      <Text>
        {getColor('success')(`${glyphs.checkmark} ${phase} complete`)}
      </Text>
    </Box>
  );
};

import React from 'react';
import { Box } from '../../renderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import { commentsForCheckpoint, useAppStore } from '../../stores/app-store.js';
import { commentCount } from '../../utils/spec-review/review-actions.js';
import { logger } from '../../utils/logger.js';
import type { SpecCheckpointPhase } from '../../types/agent-events.js';

const PHASE_LABEL: Record<SpecCheckpointPhase, string> = {
  requirements: 'Requirements',
  design: 'Design',
  tasks: 'Tasks',
  // Not "Bugfix", which reads as the bug being fixed rather than described.
  bugfix: 'Bug analysis',
};

/**
 * Marks the spec phase whose document the agent just finished, and offers the
 * way in to review it.
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
  const staged = useAppStore((s) => commentsForCheckpoint(s).length);
  const openReview = useAppStore((s) => s.openSpecReview);

  useKeypress((input, key) => {
    // Only while the question is on screen: the marker and its shortcut are
    // part of that question, not of the session.
    if (!checkpoint || !questionVisible) return;
    if (key.ctrl && input === 'x') {
      void openReview(checkpoint.featureName, checkpoint.phase).catch((err) => {
        logger.error('[spec-checkpoint] opening the review threw', {
          err: err instanceof Error ? err.message : String(err),
        });
      });
    }
  });

  if (!checkpoint || !questionVisible) return null;

  const phase = PHASE_LABEL[checkpoint.phase];
  const success = getColor('success');
  const secondary = getColor('secondary');
  const primary = getColor('primary');

  return (
    <Box marginTop={1}>
      <Text>
        {success(`${glyphs.checkmark} ${phase} complete`)}
        {secondary(` ${glyphs.smallDot} `)}
        {primary('ctrl+X')} {secondary('to review')}
        {staged > 0 &&
          success(` ${glyphs.smallDot} ${commentCount(staged)} staged`)}
      </Text>
    </Box>
  );
};

import React from 'react';
import { Box, Text } from '../../../renderer.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { getAgentColor } from '../../../utils/agentColors.js';
import { SessionOutput } from '../../multi-agent/SessionOutput.js';
import { ScrollableBox } from '../../ui/ScrollableBox.js';
import { useAppStore } from '../../../stores/app-store.js';
import type { Stage } from './types.js';
import { truncate } from './types.js';

export const WorkerOutputPanel = React.memo(function WorkerOutputPanel({
  selectedStage,
  workerOutputH,
  width,
  title = 'SUBAGENT OUTPUT',
  scrollActive = true,
  scrollToEndKey,
}: {
  selectedStage: Stage | undefined;
  workerOutputH: number;
  width: number;
  title?: string;
  /**
   * When false, the output pane's j/k/page scroll keys are suspended so they
   * don't fire while another surface (e.g. the workflow monitor's steer/respond
   * composer or an approval panel) owns keyboard input. Mouse-wheel scrolling
   * stays active. Defaults to true for the crew monitor, which has no composer.
   */
  scrollActive?: boolean;
  /**
   * #13: when this key changes, jump the output to its end so the actionable
   * tail (e.g. a paused step's question) is immediately visible instead of
   * clipped below the fold. The workflow monitor derives it from the selected
   * session + its status.
   */
  scrollToEndKey?: string | number;
}) {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();

  const sessionId = selectedStage?.sessionId;
  const selectedSession = useAppStore((state) =>
    sessionId ? state.sessions.get(sessionId) : undefined
  );

  return (
    <>
      <Box paddingX={1} marginTop={1}>
        <Text bold color="white">
          {title}
        </Text>
        {selectedStage && (
          <Text color={getAgentColor(selectedStage.name, getColor).hex}>
            {' '}
            [{truncate(selectedStage.name, 30)}]
          </Text>
        )}
        {scrollActive && (
          <Text color="gray"> j/k scroll {glyphs.smallDot} ^d/^u page</Text>
        )}
      </Box>
      <ScrollableBox
        height={workerOutputH}
        autoScroll={true}
        isActive={scrollActive}
        scrollToEndKey={scrollToEndKey}
        selectionScope
      >
        {selectedSession ? (
          <SessionOutput
            sessionId={selectedStage!.sessionId}
            session={selectedSession}
            width={width - 2}
            height={workerOutputH}
          />
        ) : (
          <Box paddingX={1}>
            <Text color="gray">Select a stage to view output</Text>
          </Box>
        )}
      </ScrollableBox>
    </>
  );
});

import React from 'react';
import { Box, Text } from '../../../renderer.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { getAgentColor } from '../../../utils/agentColors.js';
import { SessionOutput } from '../../multi-agent/SessionOutput.js';
import { ScrollableBox } from '../../ui/ScrollableBox.js';
import { useAppStore } from '../../../stores/app-store.js';
import type { Stage } from './types.js';
import { truncate, EMPTY_INBOX } from './types.js';

export const WorkerOutputPanel = React.memo(function WorkerOutputPanel({
  selectedStage,
  workerOutputH,
  width,
}: {
  selectedStage: Stage | undefined;
  workerOutputH: number;
  width: number;
}) {
  const { getColor } = useTheme();

  const sessionId = selectedStage?.sessionId;
  const selectedSession = useAppStore((state) =>
    sessionId ? state.sessions.get(sessionId) : undefined
  );
  const selectedMessages = useAppStore((state) =>
    sessionId
      ? (state.sessionMessages.get(sessionId) ?? EMPTY_INBOX)
      : EMPTY_INBOX
  );

  return (
    <>
      <Box paddingX={1} marginTop={1}>
        <Text bold color="white">
          SUBAGENT OUTPUT
        </Text>
        {selectedStage && (
          <Text color={getAgentColor(selectedStage.name, getColor).hex}>
            {' '}
            [{truncate(selectedStage.name, 30)}]
          </Text>
        )}
        <Text color="gray"> j/k scroll · ^d/^u page</Text>
      </Box>
      <ScrollableBox height={workerOutputH} autoScroll={true}>
        {selectedSession ? (
          <SessionOutput
            sessionId={selectedStage!.sessionId}
            session={selectedSession}
            messages={selectedMessages}
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

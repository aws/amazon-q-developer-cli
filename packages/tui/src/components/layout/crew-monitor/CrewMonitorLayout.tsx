import React, { useState, useMemo } from 'react';
import { Box, Text } from '../../../renderer.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { ProgressChip } from '../../ui/chip/ProgressChip.js';
import { MessageRole, useAppStore } from '../../../stores/app-store.js';
import { sessionConversationsStore } from '../../../stores/session-conversations.js';
import { useShallow } from 'zustand/react/shallow';
import type { Stage } from './types.js';
import { DagVisualization } from './DagVisualization.js';
import { WorkerOutputPanel } from './WorkerOutputPanel.js';
import { ApprovalPanel } from './ApprovalPanel.js';
import { CrewFooter } from './CrewFooter.js';

export const CrewMonitorLayout = React.memo(function CrewMonitorLayout({
  stages,
  selectedIndex,
  onSelect,
  elapsed,
  width,
  height,
}: {
  stages: Stage[];
  selectedIndex: number;
  onSelect: (i: number) => void;
  elapsed: string;
  width: number;
  height: number;
}) {
  const approvalQueue = useAppStore((state) => state.approvalQueue);
  const { hasExpandableToolOutputs, toggleToolOutputsExpanded } = useAppStore(
    useShallow((state) => ({
      hasExpandableToolOutputs: state.hasExpandableToolOutputs,
      toggleToolOutputsExpanded: state.toggleToolOutputsExpanded,
    }))
  );

  // Handle Ctrl+O to toggle tool output expansion
  useKeypress(
    (input, key) => {
      if (key.ctrl && input.toLowerCase() === 'o') {
        toggleToolOutputsExpanded();
      }
    },
    { isActive: hasExpandableToolOutputs }
  );

  const completed = useMemo(
    () => stages.filter((s) => s.state === 'Completed').length,
    [stages]
  );

  const selectedStage = stages[selectedIndex];

  const kiro = useAppStore((state) => state.kiro);
  const updateSession = useAppStore((state) => state.updateSession);
  const cleanupTerminatedSession = useAppStore(
    (state) => state.cleanupTerminatedSession
  );

  const sessionsWithApproval = useMemo(
    () =>
      new Set(
        approvalQueue.map((a) => a.sessionId).filter((id): id is string => !!id)
      ),
    [approvalQueue]
  );

  const selectedApproval = useMemo(
    () =>
      selectedStage
        ? approvalQueue.find((a) => a.sessionId === selectedStage.sessionId)
        : undefined,
    [approvalQueue, selectedStage?.sessionId]
  );

  const crewH = 1 + stages.length + 1;
  const approvalPanelH = selectedApproval ? 7 : 0;
  const workerOutputH = Math.max(
    3,
    height - 3 - crewH - approvalPanelH - 1 - 1 - 2
  );

  const [killTarget, setKillTarget] = useState<string | null>(null);
  const killTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null);

  useKeypress((input, key) => {
    if (key.ctrl && input === 'x') {
      if (!selectedStage || selectedStage.state !== 'Executing') return;
      if (killTarget === selectedStage.sessionId) {
        if (killTimerRef.current) clearTimeout(killTimerRef.current);
        setKillTarget(null);
        updateSession(selectedStage.sessionId, { status: 'terminated' });
        kiro.terminateSession(selectedStage.sessionId).catch(() => {});
        cleanupTerminatedSession(selectedStage.sessionId);

        // Mark in-flight tool calls as finished in session conversation store
        const convStore = sessionConversationsStore.getState();
        const msgs = convStore.conversations.get(selectedStage.sessionId);
        if (
          msgs?.some((m) => m.role === MessageRole.ToolUse && !m.isFinished)
        ) {
          sessionConversationsStore.setState((s) => {
            const m = new Map(s.conversations);
            m.set(
              selectedStage.sessionId,
              msgs.map((msg) =>
                msg.role === MessageRole.ToolUse && !msg.isFinished
                  ? { ...msg, isFinished: true }
                  : msg
              )
            );
            return { conversations: m };
          });
        }
      } else {
        if (killTimerRef.current) clearTimeout(killTimerRef.current);
        setKillTarget(selectedStage.sessionId);
        killTimerRef.current = setTimeout(() => setKillTarget(null), 2000);
      }
      return;
    }
    if (input === '[' || key.leftArrow) {
      onSelect(Math.max(0, selectedIndex - 1));
      return;
    }
    if (input === ']' || key.rightArrow) {
      onSelect(Math.min(stages.length - 1, selectedIndex + 1));
      return;
    }
    const num = parseInt(input);
    if (!isNaN(num) && num >= 1 && num <= stages.length) {
      onSelect(num - 1);
      return;
    }
  });

  const groupStages = useMemo(() => {
    const group = stages.find((s) => s.group)?.group;
    return group ? stages.filter((s) => s.group === group) : stages;
  }, [stages]);

  return (
    <Box flexDirection="column" width={width} height={height}>
      {/* Header */}
      <Box paddingX={2}>
        <Text bold color="white">
          AGENT MONITOR
        </Text>
        <Text>
          {' '.repeat(Math.max(1, width - 4 - 13 - 8 - elapsed.length))}
        </Text>
        <Text color="gray">elapsed {elapsed}</Text>
      </Box>
      <Box paddingX={2}>
        <ProgressChip
          value={stages.length > 0 ? (completed / stages.length) * 100 : 0}
          showPercentage={false}
          label={`${completed}/${stages.length}`}
        />
      </Box>

      <DagVisualization
        stages={stages}
        groupStages={groupStages}
        selectedIndex={selectedIndex}
        sessionsWithApproval={sessionsWithApproval}
      />

      <WorkerOutputPanel
        selectedStage={selectedStage}
        workerOutputH={workerOutputH}
        width={width}
      />

      {selectedApproval && <ApprovalPanel key={selectedApproval.toolCall.toolCallId} approval={selectedApproval} />}

      {killTarget && (
        <Box paddingX={1}>
          <Text color="magenta">Press ctrl+x again to kill session</Text>
        </Box>
      )}

      <Box flexGrow={1} />

      <CrewFooter hasExecutingSelected={selectedStage?.state === 'Executing'} />
    </Box>
  );
});

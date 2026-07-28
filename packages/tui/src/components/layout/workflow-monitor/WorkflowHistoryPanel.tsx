import React, { useRef, useState } from 'react';
import { useStore } from 'zustand';
import { Box } from '../../../renderer.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useAppStore } from '../../../stores/app-store.js';
import {
  workflowStore,
  type WorkflowStoreState,
} from '../../../stores/workflow-store.js';
import { buildHistoricalWorkflowRun } from '../../../stores/workflow-view-model.js';
import type { WorkflowRunSummary } from '../../../types/workflow-history.js';
import { Panel } from '../../ui/panel/index.js';
import { Text } from '../../ui/text/Text.js';
import { RUN_STATUS_COLOR_TOKEN, runStatusGlyph } from './run-status-style.js';
import { truncateToWidth } from '../../../utils/text-width.js';
import {
  workflowControlShortcut,
  type WorkflowControlShortcut,
} from './workflow-control-shortcut.js';

interface WorkflowHistoryPanelProps {
  onClose: () => void;
}

type PendingWorkflowAction = {
  workflowId: string;
  kind: 'load' | WorkflowControlShortcut;
};

const PENDING_ACTION_LABEL: Record<PendingWorkflowAction['kind'], string> = {
  load: 'loading',
  pause: 'pausing',
  resume: 'resuming',
};

function runDuration(run: WorkflowRunSummary): string {
  if (!run.startedAt || !run.endedAt) return '';
  const durationSeconds = Math.floor(
    (Date.parse(run.endedAt) - Date.parse(run.startedAt)) / 1000
  );
  if (durationSeconds < 0) return '';
  if (durationSeconds < 60) return `${durationSeconds}s`;
  const minutes = Math.floor(durationSeconds / 60);
  const seconds = durationSeconds % 60;
  return `${minutes}m ${seconds.toString().padStart(2, '0')}s`;
}

export const WorkflowHistoryPanel = React.memo(function WorkflowHistoryPanel({
  onClose,
}: WorkflowHistoryPanelProps) {
  const { width } = useTerminalSize();
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const kiro = useAppStore((state) => state.kiro);
  const setMode = useAppStore((state) => state.setMode);
  const runs = useStore(
    workflowStore,
    (state: WorkflowStoreState) => state.history.runs
  );
  const openHistoricalWorkflow = useStore(
    workflowStore,
    (state) => state.openHistoricalWorkflow
  );
  const setHistoryRunStatus = useStore(
    workflowStore,
    (state) => state.setHistoryRunStatus
  );
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [pendingAction, setPendingAction] =
    useState<PendingWorkflowAction | null>(null);
  const pendingActionRef = useRef<PendingWorkflowAction | null>(null);
  const [error, setError] = useState<string | null>(null);
  const selectedRun = runs[selectedIndex];

  const beginAction = (action: PendingWorkflowAction): boolean => {
    if (pendingActionRef.current) return false;
    pendingActionRef.current = action;
    setPendingAction(action);
    setError(null);
    return true;
  };

  const finishAction = (action: PendingWorkflowAction): void => {
    if (pendingActionRef.current !== action) return;
    pendingActionRef.current = null;
    setPendingAction(null);
  };

  const controlRun = async (
    run: WorkflowRunSummary,
    actionKind: WorkflowControlShortcut
  ): Promise<void> => {
    if (
      (actionKind === 'pause' && run.status !== 'running') ||
      (actionKind === 'resume' && run.status !== 'paused')
    ) {
      return;
    }
    const action: PendingWorkflowAction = {
      workflowId: run.workflowId,
      kind: actionKind,
    };
    if (!beginAction(action)) return;
    try {
      if (actionKind === 'pause') {
        const response = await kiro.pauseWorkflow(run.workflowId);
        if (!response.paused) {
          throw new Error(`Could not pause "${run.name}".`);
        }
        setHistoryRunStatus(run.workflowId, 'paused');
      } else {
        const response = await kiro.resumeWorkflow(run.workflowId);
        setHistoryRunStatus(run.workflowId, response.status);
      }
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : `Could not ${actionKind} "${run.name}".`
      );
    } finally {
      finishAction(action);
    }
  };

  const openRun = async (run: WorkflowRunSummary): Promise<void> => {
    const action: PendingWorkflowAction = {
      workflowId: run.workflowId,
      kind: 'load',
    };
    if (!beginAction(action)) return;
    try {
      const inspected = await kiro.inspectWorkflow(run.workflowId);
      openHistoricalWorkflow(buildHistoricalWorkflowRun(run, inspected));
      onClose();
      process.stdout.write('\x1b[?1049h');
      setMode('workflow-monitor');
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : `Could not load "${run.name}".`
      );
    } finally {
      finishAction(action);
    }
  };

  const controlHint =
    !pendingAction && selectedRun?.status === 'running'
      ? `${glyphs.smallDot} p pause`
      : !pendingAction && selectedRun?.status === 'paused'
        ? `${glyphs.smallDot} r resume`
        : '';

  useKeypress((input, key) => {
    if (pendingActionRef.current) return;
    if (key.upArrow || input === 'k') {
      setSelectedIndex((index) => Math.max(0, index - 1));
    } else if (key.downArrow || input === 'j') {
      setSelectedIndex((index) =>
        Math.min(Math.max(0, runs.length - 1), index + 1)
      );
    } else if (key.return || key.rightArrow) {
      if (selectedRun) void openRun(selectedRun);
    } else {
      const control = workflowControlShortcut(input, key);
      if (selectedRun && control) void controlRun(selectedRun, control);
    }
  });

  return (
    <Panel
      title="WORKFLOWS"
      onClose={onClose}
      footerExtra={
        <Text>
          {getColor('secondary')(
            `${glyphs.arrowUp}${glyphs.arrowDown} move ${glyphs.smallDot} Enter view${controlHint ? ` ${controlHint}` : ''}`
          )}
        </Text>
      }
    >
      <Box marginBottom={1}>
        <Text>{getColor('secondary')('this session')}</Text>
      </Box>
      {error && <Text>{getColor('error')(error)}</Text>}
      <Box flexDirection="column">
        {runs.map((run, index) => {
          const selected = index === selectedIndex;
          const duration = runDuration(run);
          const tail = `${run.status}${duration ? ` ${glyphs.smallDot} ${duration}` : ''}`;
          const nameWidth = Math.max(8, width - tail.length - 10);
          const statusColor = getColor(RUN_STATUS_COLOR_TOKEN[run.status]);
          const pendingLabel =
            pendingAction?.workflowId === run.workflowId
              ? PENDING_ACTION_LABEL[pendingAction.kind]
              : null;
          return (
            <Box key={run.workflowId}>
              <Text>
                {selected ? `${glyphs.chevron} ` : '  '}
                {statusColor(`${runStatusGlyph(run.status, glyphs)} `)}
                {selected
                  ? getColor('primary').bold(
                      truncateToWidth(run.name, nameWidth, '...')
                    )
                  : getColor('secondary')(
                      truncateToWidth(run.name, nameWidth, '...')
                    )}
                {getColor('secondary')(`  ${tail}`)}
                {pendingLabel ? getColor('info')(`  ${pendingLabel}`) : ''}
              </Text>
            </Box>
          );
        })}
      </Box>
    </Panel>
  );
});

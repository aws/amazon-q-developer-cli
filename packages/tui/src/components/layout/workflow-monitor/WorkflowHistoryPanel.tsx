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
import { truncateToWidth, visibleWidth } from '../../../utils/text-width.js';
import { enterAltScreen } from '../../../utils/alt-screen';
import {
  workflowControlShortcut,
  type WorkflowControlShortcut,
} from './workflow-control-shortcut.js';
import {
  isLiveWorkflowStatus,
  isRetryableWorkflowStatus,
  isTerminalWorkflowStatus,
} from '../../../types/workflow-status.js';

interface WorkflowHistoryPanelProps {
  onClose: () => void;
}

type WorkflowRunControl = WorkflowControlShortcut | 'cancel';

type PendingWorkflowAction = {
  workflowId: string;
  kind: 'load' | WorkflowRunControl;
};

const PENDING_ACTION_LABEL: Record<PendingWorkflowAction['kind'], string> = {
  load: 'loading...',
  pause: 'pausing...',
  resume: 'resuming...',
  retry: 'retrying...',
  cancel: 'cancelling...',
};

function fitFooterHint(
  maxWidth: number,
  controls: string,
  navigation: string,
  compactNavigation: string,
  separator: string
): string {
  const candidates = controls
    ? [
        `${controls}${separator}${navigation}`,
        `${controls}${separator}${compactNavigation}`,
        controls,
      ]
    : [navigation, compactNavigation];
  return (
    candidates.find((candidate) => visibleWidth(candidate) <= maxWidth) ??
    truncateToWidth(candidates.at(-1)!, maxWidth, '')
  );
}

// Splits a hint string built from `key label` chunks (joined by `separator`)
// back into {key, label} pairs so each half can be colored independently,
// matching Panel's own footer (primary key, secondary label).
function splitFooterHint(
  hint: string,
  separator: string
): { key: string; label: string }[] {
  if (!hint) return [];
  return hint.split(separator).map((chunk) => {
    const spaceIndex = chunk.indexOf(' ');
    return spaceIndex === -1
      ? { key: chunk, label: '' }
      : { key: chunk.slice(0, spaceIndex), label: chunk.slice(spaceIndex + 1) };
  });
}

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
  const [cancelConfirmationWorkflowId, setCancelConfirmationWorkflowId] =
    useState<string | null>(null);
  const cancelConfirmationWorkflowIdRef = useRef<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const selectedRun = runs[selectedIndex];

  const setCancelConfirmation = (workflowId: string | null): void => {
    cancelConfirmationWorkflowIdRef.current = workflowId;
    setCancelConfirmationWorkflowId(workflowId);
  };

  const beginAction = (action: PendingWorkflowAction): boolean => {
    if (pendingActionRef.current) return false;
    setCancelConfirmation(null);
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
    actionKind: WorkflowRunControl
  ): Promise<void> => {
    if (
      (actionKind === 'pause' && run.status !== 'running') ||
      (actionKind === 'resume' && run.status !== 'paused') ||
      (actionKind === 'retry' && !isRetryableWorkflowStatus(run.status)) ||
      (actionKind === 'cancel' && !isLiveWorkflowStatus(run.status))
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
        if (actionKind === 'resume') {
          const response = await kiro.resumeWorkflow(run.workflowId);
          setHistoryRunStatus(run.workflowId, response.status);
        } else if (actionKind === 'retry') {
          const response = await kiro.retryWorkflow(run.workflowId);
          setHistoryRunStatus(run.workflowId, response.status);
        } else {
          const response = await kiro.cancelWorkflow(run.workflowId, 'aborted');
          setHistoryRunStatus(
            run.workflowId,
            isTerminalWorkflowStatus(response.previousStatus)
              ? response.previousStatus
              : 'aborted'
          );
        }
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
      enterAltScreen();
      setMode('workflow-monitor');
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : `Could not load "${run.name}".`
      );
    } finally {
      finishAction(action);
    }
  };

  const cancelConfirmationArmed =
    selectedRun !== undefined &&
    cancelConfirmationWorkflowId === selectedRun.workflowId;
  const controlHint = cancelConfirmationArmed
    ? 'x confirm cancel'
    : !pendingAction && selectedRun?.status === 'running'
      ? `p pause ${glyphs.smallDot} x cancel`
      : !pendingAction && selectedRun?.status === 'paused'
        ? `r resume ${glyphs.smallDot} x cancel`
        : !pendingAction && isRetryableWorkflowStatus(selectedRun?.status)
          ? 'r retry'
          : pendingAction
            ? PENDING_ACTION_LABEL[pendingAction.kind]
            : '';
  const footerWidth = Math.max(1, width - 20);
  const footerHint = fitFooterHint(
    footerWidth,
    controlHint,
    `${glyphs.arrowUp}${glyphs.arrowDown} navigate ${glyphs.smallDot} enter view`,
    `${glyphs.arrowUp}${glyphs.arrowDown} ${glyphs.smallDot} enter`,
    ` ${glyphs.smallDot} `
  );
  const footerHints = splitFooterHint(footerHint, ` ${glyphs.smallDot} `);

  useKeypress((input, key) => {
    if (pendingActionRef.current) return;
    const confirmingWorkflowId = cancelConfirmationWorkflowIdRef.current;
    if (confirmingWorkflowId !== null) {
      if (key.escape) {
        setCancelConfirmation(null);
      } else if (
        selectedRun?.workflowId === confirmingWorkflowId &&
        !key.ctrl &&
        !key.meta &&
        input === 'x'
      ) {
        void controlRun(selectedRun, 'cancel');
      }
      return;
    }
    if (key.upArrow || input === 'k') {
      setSelectedIndex((index) => Math.max(0, index - 1));
    } else if (key.downArrow || input === 'j') {
      setSelectedIndex((index) =>
        Math.min(Math.max(0, runs.length - 1), index + 1)
      );
    } else if (key.return || key.rightArrow) {
      if (selectedRun) void openRun(selectedRun);
    } else if (
      selectedRun &&
      isLiveWorkflowStatus(selectedRun.status) &&
      !key.ctrl &&
      !key.meta &&
      input === 'x'
    ) {
      setCancelConfirmation(selectedRun.workflowId);
    } else {
      const control = workflowControlShortcut(input, key, selectedRun?.status);
      if (selectedRun && control) void controlRun(selectedRun, control);
    }
  });

  return (
    <Panel
      title="WORKFLOWS"
      onClose={onClose}
      closeHintLabel={cancelConfirmationArmed ? 'keep running' : 'close'}
      footerLeft={
        footerHints.length > 0 ? (
          <Text>
            {footerHints.map((hint, index) => (
              <React.Fragment key={`${hint.key}-${hint.label}`}>
                {index > 0 && getColor('secondary')(` ${glyphs.smallDot} `)}
                {getColor('primary')(hint.key)}
                {hint.label ? ' ' : ''}
                {getColor('secondary')(hint.label)}
              </React.Fragment>
            ))}
          </Text>
        ) : undefined
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
          const pendingLabel =
            pendingAction?.workflowId === run.workflowId
              ? PENDING_ACTION_LABEL[pendingAction.kind]
              : null;
          const tail = pendingLabel
            ? pendingLabel
            : `${run.status}${duration ? ` ${glyphs.smallDot} ${duration}` : ''}`;
          const prefix = `${selected ? glyphs.chevron : ' '} ${runStatusGlyph(run.status, glyphs)} `;
          const suffix = `  ${tail}`;
          const nameWidth = Math.max(
            1,
            width - 2 - visibleWidth(prefix) - visibleWidth(suffix)
          );
          const statusColor = getColor(RUN_STATUS_COLOR_TOKEN[run.status]);
          const tailColor = pendingLabel
            ? getColor('info')
            : getColor('secondary');
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
                {tailColor(suffix)}
              </Text>
            </Box>
          );
        })}
      </Box>
    </Panel>
  );
});

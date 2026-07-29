import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useStore, type StoreApi } from 'zustand';
import { Box, Split, Tabs, Text, useFullscreen } from '../../../renderer.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useAllowIcons, useGlyphs } from '../../../hooks/useGlyphs.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useAppStore } from '../../../stores/app-store.js';
import {
  selectActiveWorkflow,
  selectWorkflowNodeIndex,
  workflowStore,
  type WorkflowStoreState,
} from '../../../stores/workflow-store.js';
import {
  buildWorkflowNodeConversation,
  workflowProgress,
} from '../../../stores/workflow-view-model.js';
import { WorkflowDagView } from './WorkflowDagView.js';
import { WorkerOutputPanel } from '../crew-monitor/WorkerOutputPanel.js';
import { ApprovalPanel } from '../crew-monitor/ApprovalPanel.js';
import { CrewMonitorContent } from '../crew-monitor/CrewMonitorScreen.js';
import type { Stage } from '../crew-monitor/types.js';
import { classifyInputKey } from './classify-input-key.js';
import {
  MONITOR_RESIZE_STEP,
  monitorPaneDimensions,
  monitorSplitDirection,
  resizeMonitorRatio,
} from './monitor-layout.js';
import {
  buildMonitorFooterHints,
  buildWorkflowNavigationHints,
} from './monitor-footer-hints.js';
import {
  RUN_STATUS_COLOR_TOKEN,
  runStatusGlyph,
  runStatusLabel,
} from './run-status-style.js';
import {
  adjacentWorkflowId,
  buildWorkflowTabs,
  workflowDigitToIndex,
} from './workflow-tabs.js';
import { classifyWorkflowStopKey } from './workflow-stop-confirmation.js';
import { workflowControlShortcut } from './workflow-control-shortcut.js';
import { isTerminalWorkflowStatus } from '../../../types/workflow-status.js';
import type { WorkflowMonitorNode } from '../../../types/workflow-monitor.js';
import type { WorkflowNodeSessionTarget } from '../../../types/workflow.js';
import { setMouseCaptureEnabled } from '../../../utils/mouse-capture.js';
import {
  WORKFLOW_MESSAGE_COMPOSER_HEIGHT,
  WorkflowMessageComposer,
  type WorkflowMessageMode,
} from './WorkflowMessageComposer.js';

type InputMode = 'none' | WorkflowMessageMode;
const WORKFLOW_APPROVAL_HEIGHT = 8;

export interface WorkflowMonitorScreenProps {
  store?: StoreApi<WorkflowStoreState>;
  now?: () => number;
}

function elapsedLabel(startedAt: number | null, now: number): string {
  if (startedAt === null) return '0s';
  const totalSeconds = Math.max(0, Math.floor((now - startedAt) / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return minutes > 0
    ? `${minutes}m ${seconds.toString().padStart(2, '0')}s`
    : `${seconds}s`;
}

function messageModeForNode(
  node: WorkflowMonitorNode | null | undefined
): WorkflowMessageMode | null {
  if (node?.status === 'running') return 'steer';
  if (node?.status === 'paused' && node.completionSignal === 'need_input') {
    return 'respond';
  }
  if (node?.status === 'completed') return 'message';
  return null;
}

function messageDraftKey(target: WorkflowNodeSessionTarget): string {
  return `${target.workflowId}\u0000${target.sessionId}`;
}

export const WorkflowMonitorScreen = React.memo(function WorkflowMonitorScreen({
  store = workflowStore,
  now = Date.now,
}: WorkflowMonitorScreenProps) {
  useFullscreen();
  const { width, height } = useTerminalSize();
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();
  const workflow = useStore(store, selectActiveWorkflow);
  const selectedIndex = useStore(store, selectWorkflowNodeIndex);
  const workflows = useStore(store, (state) => state.workflows);
  const activeWorkflowId = useStore(store, (state) => state.activeWorkflowId);
  const monitorLayout = useStore(store, (state) => state.monitorLayout);
  const monitorSplitRatios = useStore(
    store,
    (state) => state.monitorSplitRatios
  );
  const pauseRequestedWorkflowIds = useStore(
    store,
    (state) => state.pauseRequestedWorkflowIds
  );
  const setActiveWorkflow = useStore(store, (state) => state.setActiveWorkflow);
  const setSelectedNode = useStore(store, (state) => state.setSelectedNode);
  const setWorkflowSurfaceOpen = useStore(
    store,
    (state) => state.setWorkflowSurfaceOpen
  );
  const setPauseRequested = useStore(store, (state) => state.setPauseRequested);
  const setInputState = useStore(store, (state) => state.setInputState);
  const toggleMonitorLayout = useStore(
    store,
    (state) => state.toggleMonitorLayout
  );
  const setMonitorSplitRatio = useStore(
    store,
    (state) => state.setMonitorSplitRatio
  );

  const kiro = useAppStore((state) => state.kiro);
  const setMode = useAppStore((state) => state.setMode);
  const approvalQueue = useAppStore((state) => state.approvalQueue);
  const queuedMessageCount = useAppStore(
    (state) =>
      state.queuedMessages.length +
      (state.pendingSteerContent
        ? state.pendingSteerContent.split('\n\n').length
        : 0)
  );
  const showTransientAlert = useAppStore((state) => state.showTransientAlert);

  const [inputMode, setInputMode] = useState<InputMode>('none');
  const [inputText, setInputTextState] = useState('');
  const [inputError, setInputError] = useState<string | null>(null);
  const [mouseModeEnabled, setMouseModeEnabled] = useState(false);
  const [activeView, setActiveView] = useState<'workflows' | 'agents'>(
    'workflows'
  );
  const [stopConfirmationWorkflowId, setStopConfirmationWorkflowId] = useState<
    string | null
  >(null);
  const composerRevisionRef = useRef(0);
  const inputTextRef = useRef('');
  const messageDraftsRef = useRef(new Map<string, string>());
  const [, setClock] = useState(0);

  useEffect(() => {
    setWorkflowSurfaceOpen('monitor', true);
    return () => {
      composerRevisionRef.current += 1;
      setMouseCaptureEnabled(false);
      setInputState(false);
      setWorkflowSurfaceOpen('monitor', false);
    };
  }, [setInputState, setWorkflowSurfaceOpen]);

  useEffect(() => {
    const timer = setInterval(() => setClock((value) => value + 1), 1000);
    return () => clearInterval(timer);
  }, []);

  const workflowList = useMemo(() => [...workflows.values()], [workflows]);
  const tabs = useMemo(
    () =>
      buildWorkflowTabs(workflowList, glyphs, (token) => getColor(token).hex),
    [getColor, glyphs, workflowList]
  );
  const workflowIds = useMemo(
    () => workflowList.map((item) => item.workflowId),
    [workflowList]
  );
  const selectedNode = workflow?.nodes[selectedIndex] ?? null;
  const selectedConversation = useMemo(
    () =>
      workflow && selectedNode
        ? buildWorkflowNodeConversation(workflow, selectedNode)
        : null,
    [selectedNode, workflow]
  );
  const selectedApproval = selectedNode?.sessionId
    ? approvalQueue.find(
        (approval) =>
          approval.sessionId === selectedNode.sessionId ||
          approval.originSessionId === selectedNode.sessionId
      )
    : undefined;
  const approvalSessionIds = useMemo(
    () =>
      new Set(
        approvalQueue.flatMap((approval) =>
          approval.sessionId ? [approval.sessionId] : []
        )
      ),
    [approvalQueue]
  );
  const selectedStage: Stage | undefined = selectedNode?.sessionId
    ? {
        name: selectedNode.label,
        agentName: selectedNode.agentName ?? selectedNode.label,
        state:
          selectedNode.status === 'running'
            ? 'Executing'
            : selectedNode.status === 'completed'
              ? 'Completed'
              : selectedNode.status === 'failed' ||
                  selectedNode.status === 'aborted'
                ? 'Failed'
                : 'Pending',
        description: '',
        events: 0,
        role: '',
        sessionId: selectedNode.sessionId,
      }
    : undefined;
  const pauseRequested =
    !!workflow && pauseRequestedWorkflowIds.has(workflow.workflowId);
  const stopConfirmationArmed =
    !!workflow &&
    stopConfirmationWorkflowId === workflow.workflowId &&
    !isTerminalWorkflowStatus(workflow.status);

  const replaceInputText = (value: string) => {
    inputTextRef.current = value;
    setInputTextState(value);
  };

  const clearInput = () => {
    setInputMode('none');
    replaceInputText('');
    setInputError(null);
    setInputState(false);
  };

  const closeInput = () => {
    composerRevisionRef.current += 1;
    clearInput();
  };

  const showError = (error: unknown, fallback: string) => {
    const message = error instanceof Error ? error.message : fallback;
    showTransientAlert({ message, status: 'error', autoHideMs: 5000 });
  };

  const inputModeForSelection = (): WorkflowMessageMode | null => {
    return selectedConversation ? messageModeForNode(selectedNode) : null;
  };

  const openInput = () => {
    composerRevisionRef.current += 1;
    const nextMode = inputModeForSelection();
    if (!nextMode || !selectedConversation) {
      setInputError('This workflow step cannot receive a message.');
      return;
    }
    setInputMode(nextMode);
    replaceInputText(
      messageDraftsRef.current.get(
        messageDraftKey(selectedConversation.target)
      ) ?? ''
    );
    setInputError(null);
    setInputState(true);
  };

  const persistMessageDraft = (
    target: WorkflowNodeSessionTarget,
    value: string
  ) => {
    const key = messageDraftKey(target);
    if (value) messageDraftsRef.current.set(key, value);
    else messageDraftsRef.current.delete(key);
  };

  const updateInputText = (update: (value: string) => string) => {
    const next = update(inputTextRef.current);
    inputTextRef.current = next;
    if (selectedConversation) {
      persistMessageDraft(selectedConversation.target, next);
    }
    setInputTextState(next);
  };

  const moveComposerSelection = (offset: -1 | 1) => {
    if (!workflow) return;
    const nextIndex = Math.min(
      Math.max(0, selectedIndex + offset),
      Math.max(0, workflow.nodes.length - 1)
    );
    if (nextIndex === selectedIndex) return;

    const nextNode = workflow.nodes[nextIndex] ?? null;
    const nextConversation = nextNode
      ? buildWorkflowNodeConversation(workflow, nextNode)
      : null;
    const nextMode = messageModeForNode(nextNode);
    if (selectedConversation) {
      persistMessageDraft(selectedConversation.target, inputTextRef.current);
    }
    composerRevisionRef.current += 1;
    setInputState(false);
    setSelectedNode(nextIndex);

    if (!nextConversation || !nextMode) {
      setInputMode('none');
      replaceInputText('');
      setInputError('This workflow step cannot receive a message.');
      return;
    }

    setInputMode(nextMode);
    replaceInputText(
      messageDraftsRef.current.get(messageDraftKey(nextConversation.target)) ??
        ''
    );
    setInputError(null);
    setInputState(true);
  };

  const submitInput = () => {
    const content = inputTextRef.current.trim();
    if (!selectedConversation || !content || inputMode === 'none') return;
    const submittedTarget = selectedConversation.target;
    const submittedDraftKey = messageDraftKey(submittedTarget);
    const submissionRevision = ++composerRevisionRef.current;
    messageDraftsRef.current.delete(submittedDraftKey);
    void kiro
      .messageWorkflowNode(submittedTarget, content)
      .catch((error: unknown) => {
        showError(error, 'Could not message workflow step');
        const state = store.getState();
        const currentWorkflow = selectActiveWorkflow(state);
        const currentNode =
          currentWorkflow?.nodes[selectWorkflowNodeIndex(state)];
        const currentTarget =
          currentWorkflow && currentNode
            ? buildWorkflowNodeConversation(currentWorkflow, currentNode)
                ?.target
            : undefined;
        const retryMode = currentTarget
          ? messageModeForNode(currentNode)
          : null;
        if (
          composerRevisionRef.current !== submissionRevision ||
          currentTarget?.workflowId !== submittedTarget.workflowId ||
          currentTarget.sessionId !== submittedTarget.sessionId ||
          retryMode === null
        ) {
          return;
        }
        messageDraftsRef.current.set(submittedDraftKey, content);
        setInputMode(retryMode);
        replaceInputText(content);
        setInputError(null);
        setInputState(true);
      });
    clearInput();
  };

  const pauseWorkflow = () => {
    if (!workflow || workflow.status !== 'running' || pauseRequested) return;
    setPauseRequested(workflow.workflowId, true);
    void kiro.pauseWorkflow(workflow.workflowId).catch((error: unknown) => {
      setPauseRequested(workflow.workflowId, false);
      showError(error, 'Could not pause workflow');
    });
  };

  const resumeWorkflow = () => {
    if (!workflow || workflow.status !== 'paused') return;
    void kiro
      .resumeWorkflow(workflow.workflowId)
      .catch((error: unknown) => showError(error, 'Could not resume workflow'));
  };

  const stopWorkflow = () => {
    if (!workflow) return;
    const workflowId = workflow.workflowId;
    setStopConfirmationWorkflowId(null);
    void kiro
      .cancelWorkflow(workflowId, 'aborted')
      .catch((error: unknown) => showError(error, 'Could not stop workflow'));
  };

  useKeypress((input, key) => {
    if (activeView === 'agents') {
      if (key.tab) setActiveView('workflows');
      else if (key.escape || (!key.ctrl && !key.meta && input === 'q')) {
        setMode('inline');
      }
      return;
    }

    if (inputMode !== 'none') {
      if (key.upArrow) {
        moveComposerSelection(-1);
        return;
      }
      if (key.downArrow) {
        moveComposerSelection(1);
        return;
      }
      const action = classifyInputKey(input, key);
      if (action === 'cancel') closeInput();
      else if (action === 'submit') submitInput();
      else if (action === 'delete') {
        updateInputText((value) => value.slice(0, -1));
      } else if (typeof action === 'object') {
        updateInputText((value) => value + action.append);
      }
      return;
    }

    const stopAction = classifyWorkflowStopKey(
      input,
      key,
      stopConfirmationArmed,
      !!workflow && !isTerminalWorkflowStatus(workflow.status)
    );
    if (stopAction === 'arm') {
      setStopConfirmationWorkflowId(workflow?.workflowId ?? null);
      return;
    }
    if (stopAction === 'confirm') {
      stopWorkflow();
      return;
    }
    if (stopAction === 'dismiss') {
      setStopConfirmationWorkflowId(null);
      return;
    }
    if (stopAction === 'block') return;

    if (selectedApproval) {
      if (key.escape) {
        setMouseCaptureEnabled(false);
        setMode('inline');
      }
      return;
    }
    if (key.escape || (!key.ctrl && !key.meta && input === 'q')) {
      setMouseCaptureEnabled(false);
      setMode('inline');
      return;
    }
    if (key.tab) {
      setMouseCaptureEnabled(false);
      setMouseModeEnabled(false);
      setActiveView('agents');
      return;
    }
    if (key.upArrow) {
      setSelectedNode(selectedIndex - 1);
      return;
    }
    if (key.downArrow) {
      setSelectedNode(selectedIndex + 1);
      return;
    }
    if (key.leftArrow || key.rightArrow) {
      const adjacent = adjacentWorkflowId(
        workflowIds,
        activeWorkflowId,
        key.rightArrow ? 1 : -1
      );
      if (adjacent) setActiveWorkflow(adjacent);
      return;
    }
    const digitIndex = workflowDigitToIndex(input, workflowIds.length);
    if (!key.ctrl && !key.meta && digitIndex !== null) {
      const workflowId = workflowIds[digitIndex];
      if (workflowId) setActiveWorkflow(workflowId);
      return;
    }
    if (!key.ctrl && !key.meta && input === 's') {
      openInput();
      return;
    }
    const controlShortcut = workflowControlShortcut(input, key);
    if (controlShortcut === 'pause') {
      pauseWorkflow();
      return;
    }
    if (controlShortcut === 'resume') {
      resumeWorkflow();
      return;
    }
    if (!key.ctrl && !key.meta && input === 'l') {
      toggleMonitorLayout();
      return;
    }
    if (!key.ctrl && !key.meta && (input === '[' || input === ']')) {
      setMonitorSplitRatio(
        monitorLayout,
        resizeMonitorRatio(
          monitorSplitRatios[monitorLayout],
          input === '[' ? -MONITOR_RESIZE_STEP : MONITOR_RESIZE_STEP
        )
      );
      return;
    }
    if (!key.ctrl && !key.meta && input === 'm') {
      setMouseModeEnabled((enabled) => {
        const next = !enabled;
        setMouseCaptureEnabled(next);
        return next;
      });
    }
  });

  if (activeView === 'agents') {
    return <CrewMonitorContent />;
  }

  if (!workflow) {
    return (
      <Box flexDirection="column" width={width} height={height}>
        <Text>{getColor('brand').bold('WORKFLOWS')}</Text>
        <Text>
          {getColor('secondary')('No active workflows. Esc to return.')}
        </Text>
      </Box>
    );
  }

  const headerRows =
    2 + Number(workflowList.length > 1) + Number(Boolean(workflow.pauseReason));
  const ruleRows = 2;
  const footerRows = inputMode === 'none' ? 2 : 0;
  const contentHeight = Math.max(
    4,
    height - headerRows - ruleRows - footerRows
  );
  const ratio = monitorSplitRatios[monitorLayout];
  const dimensions = monitorPaneDimensions(
    monitorLayout,
    width,
    contentHeight,
    ratio
  );
  const statusColor = getColor(RUN_STATUS_COLOR_TOKEN[workflow.status]);
  const progress = workflowProgress(workflow.nodes);
  const outputAccessoryHeight = selectedApproval
    ? WORKFLOW_APPROVAL_HEIGHT
    : inputMode === 'none'
      ? inputError
        ? 1
        : 0
      : WORKFLOW_MESSAGE_COMPOSER_HEIGHT;
  const horizontalRule = glyphs.lineHorizontalHeavy.repeat(
    Math.max(0, width - 2)
  );
  const navigationHints = buildWorkflowNavigationHints(workflowList.length);
  const footer = buildMonitorFooterHints({
    selectedNode,
    status: workflow.status,
    monitorLayout,
    mouseModeEnabled,
    stopConfirmationArmed,
    inputOpen: inputMode !== 'none',
  });

  return (
    <Box flexDirection="column" width={width} height={height}>
      <Box width={width} paddingX={1}>
        <Box flexGrow={1}>
          <Text>
            {getColor('brand').bold('WORKFLOWS')}
            {workflowList.length > 1
              ? getColor('secondary')(
                  `  ${glyphs.lineVertical}  ${glyphs.arrowLeft}${glyphs.arrow} workflows ${glyphs.smallDot} 1-9 jump`
                )
              : ''}
          </Text>
        </Box>
        {mouseModeEnabled && (
          <Text>
            {getColor('info').bold(
              `${allowIcons ? `${glyphs.dotFilled} ` : ''}MOUSE ON`
            )}
          </Text>
        )}
      </Box>

      {workflowList.length > 1 && (
        <Tabs
          tabs={tabs}
          activeId={activeWorkflowId ?? workflow.workflowId}
          onActivate={setActiveWorkflow}
          showIndexes
          width={width}
          activeColor={getColor('brand').hex}
          inactiveColor={getColor('secondary').hex}
          borderColor={getColor('secondary').hex}
        />
      )}

      <Box width={width} paddingX={1}>
        <Text wrap="truncate">
          {getColor('secondary')(`${glyphs.lineVertical} `)}
          {statusColor(
            `${allowIcons ? `${runStatusGlyph(workflow.status, glyphs)} ` : ''}${workflow.name}`
          )}
          {getColor('secondary')(
            ` - ${runStatusLabel(workflow.status, pauseRequested)} ${glyphs.lineVertical} ${progress.completed}/${progress.total} ${glyphs.lineVertical} ${elapsedLabel(workflow.startedAt, now())} ${glyphs.lineVertical} Tab agents`
          )}
          {queuedMessageCount > 0
            ? getColor('warning')(
                ` ${glyphs.lineVertical} ${queuedMessageCount} messages queued`
              )
            : ''}
        </Text>
      </Box>

      {workflow.pauseReason && (
        <Box paddingX={1}>
          <Text>{getColor('warning')(workflow.pauseReason)}</Text>
        </Box>
      )}

      <Box paddingX={1} height={1}>
        <Text wrap="truncate">{getColor('secondary')(horizontalRule)}</Text>
      </Box>

      <Split
        direction={monitorSplitDirection(monitorLayout)}
        ratio={ratio}
        width={width}
        height={contentHeight}
        showPaneBorders={false}
        activeColor={getColor('brand').hex}
        inactiveColor={getColor('secondary').hex}
        onResize={(nextRatio) =>
          mouseModeEnabled && setMonitorSplitRatio(monitorLayout, nextRatio)
        }
      >
        <Box
          flexDirection="column"
          width={dimensions.dagWidth}
          height={dimensions.dagHeight}
          paddingX={1}
        >
          <Box height={1}>
            <Text>{getColor('secondary')('Steps')}</Text>
          </Box>
          <WorkflowDagView
            nodes={workflow.nodes}
            selectedIndex={selectedIndex}
            width={Math.max(1, dimensions.dagWidth - 2)}
            height={Math.max(1, dimensions.dagHeight - 1)}
            approvalSessionIds={approvalSessionIds}
            onSelectNode={mouseModeEnabled ? setSelectedNode : undefined}
          />
        </Box>
        <Box
          flexDirection="column"
          width={dimensions.outputWidth}
          height={dimensions.outputHeight}
        >
          <Box height={1} paddingX={1}>
            <Text>{getColor('secondary')('Output')}</Text>
          </Box>
          <WorkerOutputPanel
            selectedStage={selectedStage}
            workerOutputH={Math.max(
              2,
              dimensions.outputHeight - 1 - 2 - outputAccessoryHeight
            )}
            width={dimensions.outputWidth}
            title="WORKFLOW OUTPUT"
          />
          {selectedApproval ? (
            <Box
              width={dimensions.outputWidth}
              height={WORKFLOW_APPROVAL_HEIGHT}
              overflow="hidden"
            >
              <ApprovalPanel
                approval={selectedApproval}
                width={dimensions.outputWidth}
              />
            </Box>
          ) : inputMode !== 'none' ? (
            <WorkflowMessageComposer
              mode={inputMode}
              targetLabel={
                selectedConversation?.label ?? selectedNode?.label ?? 'step'
              }
              value={inputText}
              width={dimensions.outputWidth}
            />
          ) : inputError ? (
            <Box paddingX={1}>
              <Text wrap="truncate">{getColor('error')(inputError)}</Text>
            </Box>
          ) : null}
        </Box>
      </Split>

      <Box paddingX={1} height={1}>
        <Text wrap="truncate">{getColor('secondary')(horizontalRule)}</Text>
      </Box>

      {footerRows > 0 && (
        <Box paddingX={1} height={footerRows}>
          <Text wrap="wrap">
            {getColor('secondary')(
              navigationHints ? `${navigationHints} | ${footer}` : footer
            )}
          </Text>
        </Box>
      )}
    </Box>
  );
});

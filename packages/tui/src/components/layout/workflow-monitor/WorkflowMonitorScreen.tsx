import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useStore, type StoreApi } from 'zustand';
import {
  Box,
  Split,
  Tabs,
  Text,
  useFullscreen,
  useSelectionCopy,
} from '../../../renderer.js';
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
import { classifyInputKey } from './classify-input-key.js';
import { classifyWorkflowStopKey } from './workflow-stop-confirmation.js';
import { runStopNotice } from './run-stop-notice.js';
import { messageModeForNode } from './workflow-message-mode.js';
import { retryIsStepScoped } from './workflow-retry-scope.js';
import { workflowControlShortcut } from './workflow-control-shortcut.js';
import {
  isRetryableWorkflowStatus,
  isTerminalWorkflowStatus,
} from '../../../types/workflow-status.js';
import type { WorkflowNodeSessionTarget } from '../../../types/workflow.js';
import { setMouseCaptureEnabled } from '../../../utils/mouse-capture.js';
import {
  readBoolSetting,
  updateCliSetting,
} from '../../../utils/cli-settings.js';
import { Settings } from '../../../constants/settings.js';
import {
  WORKFLOW_MESSAGE_COMPOSER_HEIGHT,
  WorkflowMessageComposer,
  type WorkflowMessageMode,
} from './WorkflowMessageComposer.js';

type InputMode = 'none' | WorkflowMessageMode;
const WORKFLOW_APPROVAL_HEIGHT = 8;
// #13: reserved rows for the non-clipped paused-question banner. A fixed height
// keeps the output-pane height math deterministic while wrapping long questions
// across up to this many rows (instead of truncating to "...if").
const QUESTION_BANNER_ROWS = 3;

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

function messageDraftKey(target: WorkflowNodeSessionTarget): string {
  return `${target.workflowId}\u0000${target.sessionId}`;
}

// LINT-DEBT(complexity): pre-existing at gate adoption; Function 'WorkflowMonitorScreen' has a complexity of 52. Maximum allowed is 30.; refactor before extending
// LINT-DEBT(sonarjs/cognitive-complexity): pre-existing at gate adoption; Refactor this function to reduce its Cognitive Complexity from 39 to the 30 allowed.; refactor before extending
// eslint-disable-next-line complexity, sonarjs/cognitive-complexity
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
  // #2/#15: mouse capture defaults ON but is persisted, so users who rely on
  // native terminal text-selection can turn it off with `m` and have that stick.
  const [mouseModeEnabled, setMouseModeEnabled] = useState(() =>
    readBoolSetting(Settings.WORKFLOW_MONITOR_MOUSE, true)
  );
  const [copiedFlash, setCopiedFlash] = useState(false);
  const copiedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useSelectionCopy(
    (text) => {
      if (!text.replace(/\n+$/g, '')) return;
      setCopiedFlash(true);
      if (copiedTimerRef.current) clearTimeout(copiedTimerRef.current);
      copiedTimerRef.current = setTimeout(() => setCopiedFlash(false), 1600);
    },
    { isActive: mouseModeEnabled }
  );
  useEffect(
    () => () => {
      if (copiedTimerRef.current) clearTimeout(copiedTimerRef.current);
    },
    []
  );
  const [activeView, setActiveView] = useState<'workflows' | 'agents'>(
    'workflows'
  );
  const [stopConfirmationWorkflowId, setStopConfirmationWorkflowId] = useState<
    string | null
  >(null);
  const composerRevisionRef = useRef(0);
  const inputTextRef = useRef('');
  const messageDraftsRef = useRef(new Map<string, string>());
  const pendingRetryWorkflowIdsRef = useRef(new Set<string>());
  const [, setClock] = useState(0);

  useEffect(() => {
    setWorkflowSurfaceOpen('monitor', true);
    // Honor the persisted mouse preference on open (default ON, #2/#15).
    setMouseCaptureEnabled(mouseModeEnabled);
    return () => {
      composerRevisionRef.current += 1;
      setMouseCaptureEnabled(false);
      setInputState(false);
      setWorkflowSurfaceOpen('monitor', false);
    };
    // Only run on mount/unmount; live toggles go through the `m` handler.
    // LINT-DEBT(react-hooks/exhaustive-deps): pre-existing suppression accepted at gate adoption; mount lifecycle intentionally captures the initial mouse preference
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
    // Chatting no longer rehydrates the run, so name retry or it looks stuck.
    // Only on success, and named at the scope `retryWorkflow` will really use, so
    // the alert can't promise a landed send that was rejected or a step-scoped
    // retry that reruns the whole run.
    const retryHint =
      selectedNode?.status === 'failed'
        ? retryIsStepScoped(selectedNode)
          ? 'Sent. Press r to retry this step — chat alone won’t resume the run.'
          : 'Sent. Press r to retry the run — chat alone won’t resume it, and a step inside a loop can’t be retried on its own.'
        : null;
    void kiro
      .messageWorkflowNode(submittedTarget, content)
      .then(() => {
        if (retryHint === null) return;
        showTransientAlert({
          message: retryHint,
          status: 'info',
          autoHideMs: 6000,
        });
      })
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

  const retryWorkflow = () => {
    if (!workflow || !isRetryableWorkflowStatus(workflow.status)) return;
    const workflowId = workflow.workflowId;
    if (pendingRetryWorkflowIdsRef.current.has(workflowId)) return;
    pendingRetryWorkflowIdsRef.current.add(workflowId);
    const nodeId = retryIsStepScoped(selectedNode)
      ? selectedNode?.id
      : undefined;
    void kiro
      .retryWorkflow(workflowId, nodeId)
      .catch((error: unknown) => showError(error, 'Could not retry workflow'))
      .finally(() => pendingRetryWorkflowIdsRef.current.delete(workflowId));
  };

  const stopWorkflow = () => {
    if (!workflow) return;
    const workflowId = workflow.workflowId;
    setStopConfirmationWorkflowId(null);
    void kiro
      .cancelWorkflow(workflowId, 'aborted')
      .catch((error: unknown) => showError(error, 'Could not stop workflow'));
  };

  // LINT-DEBT(complexity): pre-existing at gate adoption; Arrow function has a complexity of 56. Maximum allowed is 30.; refactor before extending
  // LINT-DEBT(sonarjs/cognitive-complexity): pre-existing at gate adoption; Refactor this function to reduce its Cognitive Complexity from 50 to the 30 allowed.; refactor before extending
  // eslint-disable-next-line complexity, sonarjs/cognitive-complexity
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
    const controlShortcut = workflowControlShortcut(
      input,
      key,
      workflow?.status
    );
    if (controlShortcut === 'pause') {
      pauseWorkflow();
      return;
    }
    if (controlShortcut === 'resume') {
      resumeWorkflow();
      return;
    }
    if (controlShortcut === 'retry') {
      retryWorkflow();
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
        // Persist the explicit preference (fire-and-forget; failure is logged
        // inside cli-settings and only costs the sticky default, not the toggle).
        void updateCliSetting(Settings.WORKFLOW_MONITOR_MOUSE, next);
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
        <Text>{getColor('brand').bold('WORKFLOW(S)')}</Text>
        <Text>
          {getColor('secondary')('No active workflows. Esc to return.')}
        </Text>
      </Box>
    );
  }

  const stopNotice = runStopNotice(workflow);
  const headerRows =
    2 + Number(workflowList.length > 1) + Number(Boolean(stopNotice));
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
  // #13: the actionable question a paused step is waiting on. Surfaced in a
  // dedicated wrapping banner (below) that lives OUTSIDE the scrollable output
  // pane, so it can never be clipped at "...if" like the raw transcript tail.
  // Any parked step, since KAS parks interactive ones with no `need_input`.
  // Excludes the two parks that aren't questions: a user-initiated stop, already
  // explained by `stopNotice`, and a park that resumes on its own.
  const awaitingAnswer =
    workflow.status === 'paused' && workflow.stopInitiator !== 'user';
  // The reason can genuinely be absent — KAS parks some interactive steps without
  // one, and after a reload an ambiguous run-level reason is deliberately given to
  // nobody. The affordance still works, so say so rather than offering `s respond`
  // against blank space.
  const pausedQuestion =
    awaitingAnswer && messageModeForNode(selectedNode) === 'respond'
      ? selectedNode?.pauseReason?.trim() ||
        'Waiting on you. The step did not say what for — press s to reply.'
      : undefined;
  const questionBannerRows = pausedQuestion ? QUESTION_BANNER_ROWS : 0;
  // #13: jump the output pane to its end whenever the selected step (or its
  // status) changes, so the tail is visible immediately on pause/select.
  const scrollToEndKey = selectedNode
    ? `${selectedNode.sessionId ?? selectedNode.id} ${selectedNode.status}`
    : undefined;
  const outputAccessoryHeight =
    questionBannerRows +
    (selectedApproval
      ? WORKFLOW_APPROVAL_HEIGHT
      : inputMode === 'none'
        ? inputError
          ? 1
          : 0
        : WORKFLOW_MESSAGE_COMPOSER_HEIGHT);
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
    glyphs,
  });

  return (
    <Box flexDirection="column" width={width} height={height}>
      <Box width={width} paddingX={1}>
        <Box flexGrow={1}>
          <Text>
            {getColor('brand').bold('WORKFLOW(S)')}
            {workflowList.length > 1
              ? getColor('secondary')(
                  `  ${glyphs.lineVertical}  ${glyphs.arrowLeft}${glyphs.arrow} workflows ${glyphs.smallDot} 1-9 jump`
                )
              : ''}
          </Text>
        </Box>
        {mouseModeEnabled && (
          <Text>
            {copiedFlash
              ? getColor('success').bold(
                  `COPIED${allowIcons ? ` ${glyphs.checkmark}` : ''}`
                )
              : getColor('info').bold(
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
          {allowIcons
            ? statusColor(`${runStatusGlyph(workflow.status, glyphs)} `)
            : ''}
          {getColor('brand')(workflow.name)}
          {getColor('secondary')(
            ` - ${runStatusLabel(workflow.status, pauseRequested)} ${glyphs.lineVertical} ${progress.completed}/${progress.total} ${glyphs.lineVertical} ${elapsedLabel(workflow.startedAt, now())} ${glyphs.lineVertical} Tab agent monitor`
          )}
          {queuedMessageCount > 0
            ? getColor('warning')(
                ` ${glyphs.lineVertical} ${queuedMessageCount} messages queued`
              )
            : ''}
        </Text>
      </Box>

      {stopNotice && (
        <Box paddingX={1}>
          <Text wrap="truncate">{getColor('warning')(stopNotice)}</Text>
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
            scrollActive={inputMode === 'none' && !selectedApproval}
            scrollToEndKey={scrollToEndKey}
          />
          {pausedQuestion && (
            <Box
              width={dimensions.outputWidth}
              height={QUESTION_BANNER_ROWS}
              paddingX={1}
              overflow="hidden"
            >
              <Text wrap="wrap">
                {getColor('warning').bold(`${glyphs.warning} Waiting on you: `)}
                {getColor('primary')(pausedQuestion)}
              </Text>
            </Box>
          )}
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
              key={selectedConversation?.target.sessionId ?? selectedIndex}
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
            {navigationHints && (
              <>
                {getColor('secondary')(navigationHints)}
                {getColor('secondary')(` ${glyphs.smallDot} `)}
              </>
            )}
            {footer.map((hint, index) => (
              <React.Fragment key={`${hint.key}-${hint.label}`}>
                {index > 0 && getColor('secondary')(` ${glyphs.smallDot} `)}
                {getColor('primary')(hint.key)}{' '}
                {getColor('secondary')(hint.label)}
              </React.Fragment>
            ))}
          </Text>
        </Box>
      )}
    </Box>
  );
});

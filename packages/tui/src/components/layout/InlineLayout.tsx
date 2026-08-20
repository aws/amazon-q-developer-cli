import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { Box, Text } from './../../renderer.js';
import { truncateToWidth } from '../../utils/text-width.js';
import { enterAltScreen } from '../../utils/alt-screen';
import { usePlanModeToggle } from '../../hooks/usePlanModeToggle.js';
import { useQueuedInputRestore } from '../../hooks/useQueuedInputRestore.js';
import {
  AnimationPausedContext,
  useAnimationPaused,
} from '../../contexts/AnimationPausedContext.js';
import { ConversationView } from '../ui/ConversationView';
import { WelcomeScreen } from '../welcome-screen/index.js';
import { SourceProviderGate } from '../ui/SourceProviderGate.js';
import { openUrlInBrowser } from '../../utils/browser.js';
import { SOURCE_PROVIDER_SETUP_URL } from '../../utils/cloud-urls.js';
import { Feature, features } from '../../features.js';
import { formatCloudStartupChecklist } from './shared/cloud-startup-checklist.js';
import { cloudConnectStage } from './shared/cloud-connect-stage.js';
import { ExitHint } from '../ui/ExitHint';
import { CommandMenu } from '../ui/CommandMenu';
import { ActionHint } from '../ui/hint/ActionHint.js';
import {
  PromptBar,
  type PromptBarHeader,
} from '../chat/prompt-bar/PromptBar.js';
import { SnackBar } from '../chat/prompt-bar/SnackBar.js';
import { NotificationBar } from '../chat/notification-bar/NotificationBar.js';
import { BlockingErrorAlert } from '../ui/alert/BlockingErrorAlert.js';
import { CrewApprovalRequest } from '../ui/CrewApprovalRequest.js';
import { VoiceModelDownloadGate } from '../ui/VoiceModelDownloadGate.js';
import { Question } from '../ui/Question.js';
import { SpecDescriptionIntro } from '../ui/SpecDescriptionIntro.js';
import { TrustAllToolsBanner } from '../ui/TrustAllToolsBanner.js';
import { SurveyPromptBar } from '../ui/SurveyPromptBar';
import { ArtifactGenerationCard } from '../ui/ArtifactView/ArtifactGenerationCard.js';
import {
  BackendPanels,
  useBackendPanelVisibility,
} from './shared/BackendPanels.js';
import { SpecCheckpointChip } from '../ui/SpecCheckpointChip.js';
import { useCheckpointAnswer } from '../../hooks/useCheckpointAnswer.js';
import { useBackendPanelHandlers } from './shared/useBackendPanelHandlers.js';
import {
  getCachedAllWorkspaceSessions,
  scanAllWorkspaceSessions,
} from '../../utils/all-workspace-sessions.js';
import type { VariantLayoutProps } from './variant-layout.js';

import {
  useNotificationState,
  useNotificationActions,
  useProcessingState,
  useUIState,
  useUIActions,
  useContextState,
  useCommandActions,
  useInputActions,
  useConversationState,
  useApprovalState,
  useQueueState,
  useQueueActions,
} from '../../stores/selectors.js';
import {
  useAppStore,
  summarizeInitErrors,
  severityForInitErrors,
  MessageRole,
} from '../../stores/app-store.js';
import { ToolUseMessage } from '../ui/ToolUseMessage.js';
import { useSessionConversation } from '../../stores/session-conversations.js';
import { useKeypress, type Key } from '../../hooks/useKeypress';
import { useInteractionReady } from '../../hooks/useInteractionReady.js';
import {
  resolveKeybinding,
  formatKeybinding,
} from '../../utils/keybindings.js';
import { useKeybindings } from '../../hooks/useKeybindings.js';
import { getPlaceholder } from './getPlaceholder.js';
import { useStatusSurfaceProps } from './useStatusSurfaceProps.js';
import { getGitBranch } from '../../utils/git';
import { useStatusBilling } from './status-line/useStatusBilling.js';
import { getAgentColor, isAutonomousAgent } from '../../utils/agentColors.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import {
  useGlyphs,
  useSpinners,
  useAllowAnimations,
} from '../../hooks/useGlyphs.js';
import {
  activityTrayOwnsInput,
  resolveActivityTrayQueueInputAction,
} from '../ui/activity-tray/input-ownership.js';
import {
  useActivityTrayInputGateReader,
  useActivityTrayModel,
} from '../ui/activity-tray/useActivityTrayModel.js';

const TRIGGER_RULES = [
  { key: '/', type: 'start' as const },
  { key: '@', type: 'inline' as const },
];

const ActivityTrayAwarePromptBar = React.memo(
  function ActivityTrayAwarePromptBar(
    props: React.ComponentProps<typeof PromptBar>
  ) {
    const { activeTab, inputOwnership, navigationActive } =
      useActivityTrayModel();
    const { pendingSteerContent, queuedMessages } = useQueueState();
    const { removeQueuedMessage, startEditingQueue } = useQueueActions();
    const clearSteerMessage = useAppStore((state) => state.clearSteerMessage);
    const selectedIndex = useAppStore(
      (state) => state.activityTraySelectedIndex
    );
    const readInputGate = useActivityTrayInputGateReader();
    const isInputOwnedExternally = useCallback(
      (input: string, key: Key, promptIsEmpty: boolean) => {
        if (!readInputGate().inputEnabled) return false;
        if (activityTrayOwnsInput(input, key, inputOwnership)) return true;

        const action = resolveActivityTrayQueueInputAction(
          key,
          navigationActive && activeTab === 'queue',
          promptIsEmpty
        );
        if (!action) return false;

        const queueIndex = Math.max(
          0,
          Math.min(selectedIndex, queuedMessages.length - 1)
        );
        if (action === 'remove-queue-entry') {
          if (queuedMessages.length > 0) {
            removeQueuedMessage(queueIndex);
          } else if (pendingSteerContent != null) {
            clearSteerMessage();
          }
        } else if (queuedMessages.length > 0) {
          startEditingQueue(queueIndex);
        }
        return true;
      },
      [
        activeTab,
        clearSteerMessage,
        inputOwnership,
        navigationActive,
        pendingSteerContent,
        queuedMessages.length,
        readInputGate,
        removeQueuedMessage,
        selectedIndex,
        startEditingQueue,
      ]
    );

    return (
      <PromptBar {...props} isInputOwnedExternally={isInputOwnedExternally} />
    );
  }
);

function triggerEasterEgg() {
  const cols = process.stdout.columns || 60;
  const rows = process.stdout.rows || 20;
  const COUNT = 8;
  const TICK = 150;
  const DURATION = 4000;
  const emoji = '👻';
  const save = '\x1b7';
  const restore = '\x1b8';
  const moveTo = (r: number, c: number) => `\x1b[${r};${c}H`;

  const ghosts = Array.from({ length: COUNT }, () => ({
    x: Math.floor(Math.random() * (cols - 2)),
    y: Math.floor(Math.random() * (rows - 2)),
    dx: Math.random() > 0.5 ? 2 : -2,
    dy: Math.random() > 0.5 ? 1 : -1,
  }));
  let prev = ghosts.map((g) => ({ x: g.x, y: g.y }));

  const interval = setInterval(() => {
    let out = save;
    for (const p of prev) out += moveTo(p.y + 1, p.x + 1) + '  ';
    for (const g of ghosts) {
      g.x += g.dx;
      g.y += g.dy;
      if (g.x <= 0 || g.x >= cols - 2) g.dx = -g.dx;
      if (g.y <= 0 || g.y >= rows - 2) g.dy = -g.dy;
      g.x = Math.max(0, Math.min(cols - 2, g.x));
      g.y = Math.max(0, Math.min(rows - 2, g.y));
      out += moveTo(g.y + 1, g.x + 1) + emoji;
    }
    prev = ghosts.map((g) => ({ x: g.x, y: g.y }));
    out += restore;
    process.stdout.write(out);
  }, TICK);

  setTimeout(() => {
    clearInterval(interval);
    let out = save;
    for (const p of prev) out += moveTo(p.y + 1, p.x + 1) + '  ';
    out += restore;
    process.stdout.write(out);
  }, DURATION);
}

export const InlineLayout: React.FC<VariantLayoutProps> = ({
  ApprovalPrompt,
  StatusLine,
  ActivityTray,
  // LINT-DEBT(complexity): pre-existing at gate adoption; Arrow function has a complexity of 130. Maximum allowed is 30.; refactor before extending
  // LINT-DEBT(sonarjs/cognitive-complexity): pre-existing at gate adoption; Refactor this function to reduce its Cognitive Complexity from 38 to the 30 allowed.; refactor before extending
  // eslint-disable-next-line complexity, sonarjs/cognitive-complexity
}) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { allowAnimations } = useAllowAnimations();
  // Grouped selectors using useShallow - prevents re-render cascades
  const {
    transientAlert,
    loadingMessage,
    agentError,
    agentErrorGuidance,
    initErrors,
    pendingOAuthServers,
  } = useNotificationState();
  const { dismissTransientAlert, setAgentError } = useNotificationActions();
  const {
    isProcessing,
    isCompacting,
    isShellEscape,
    pendingApproval,
    pendingQuestion,
    cancelMessage,
    noInteractive,
  } = useProcessingState();
  const { respondToApproval, approvalMode } = useApprovalState();
  const specDescriptionFeature = useAppStore(
    (state) => state.pendingSpecDescription?.featureName ?? null
  );
  const specCheckpointActive = useAppStore(
    (state) => state.specPhaseCheckpoint !== null
  );
  const voiceDownloadConfirm = useAppStore(
    (state) => state.voiceDownloadConfirm
  );
  const { checkpointOptions, answerCheckpoint } = useCheckpointAnswer();
  const globalPaused = useAnimationPaused();
  const keybindings = useKeybindings();
  const trustAllToolsAccepted = useAppStore(
    (state) => state.trustAllToolsConfirmed
  );
  const {
    toolOutputsExpanded,
    hasExpandableToolOutputs,
    tangentName,
    showSourceProviderGate,
    sourceProviderSetupUrl,
  } = useUIState();
  const { toggleToolOutputsExpanded } = useUIActions();
  const {
    sessionId,
    contextUsagePercent,
    currentModel,
    currentAgent,
    goalStatus,
  } = useContextState();
  const statusSurface = useStatusSurfaceProps();
  const activeCommand = useAppStore((state) => state.activeCommand);
  const goalCancelFailed = useAppStore((state) => state.goalCancelFailed);
  const cloudSessionActive = useAppStore((state) => state.cloudSessionActive);
  // MCP OAuth prompts come from LOCAL MCP servers; a cloud session runs its
  // tools in the sandbox, so the local auth nag doesn't apply there and would
  // read as a leak from the previous local session. It reappears untouched
  // when the user switches back to a local session.
  const oauthBannerVisible =
    pendingOAuthServers.size > 0 && !cloudSessionActive;
  const cloudProviderChecked = useAppStore(
    (state) => state.cloudProviderChecked
  );
  const cloudProvider = useAppStore((state) => state.cloudProvider);
  const cloudRepoCount = useAppStore((state) => state.cloudRepoCount);
  const cloudNewSessionChecklist = useAppStore(
    (state) => state.cloudNewSessionChecklist
  );
  // Resume vs create wording for the session checklist row: the boot path
  // records the origin (via beginKasSession) before the session RPC runs.
  const cloudSessionResumed = useAppStore(
    (state) => state.kas.sessionOrigin === 'resumed'
  );
  const bootProgress = useAppStore((state) => state.bootProgress);
  const inlineSpinners = useSpinners();
  const promptHint = useAppStore((state) => state.promptHint);
  const commandInputValue = useAppStore((state) => state.commandInputValue);
  const { setActiveCommand, setActiveTrigger, clearCommandInput } =
    useCommandActions();
  const { handleUserInput, clearInput } = useInputActions();
  const { messages } = useConversationState();
  const { pendingSteerContent, activeInterruptMode, editingQueueIndex } =
    useQueueState();
  const replaceQueuedMessage = useAppStore((s) => s.replaceQueuedMessage);
  const cancelEditingQueue = useAppStore((s) => s.cancelEditingQueue);
  const isInitialized = useAppStore((s) => s.isInitialized);
  const hasEnteredConversation = useAppStore((s) => s.hasEnteredConversation);
  const settings = useAppStore((s) => s.settings);
  const mode = useAppStore((state) => state.mode);
  const backendPanelHandlers = useBackendPanelHandlers();

  const toggleHintLabel = useMemo(() => {
    const binding = resolveKeybinding(settings, 'toggleInterruptMode');
    return formatKeybinding(binding);
  }, [settings]);
  const setMode = useAppStore((state) => state.setMode);
  const exitSequence = useAppStore((state) => state.exitSequence);
  const suspendArmed = useAppStore((state) => state.suspendArmed);

  const backendPanelVisibility = useBackendPanelVisibility();
  const surveyPrompt = useAppStore((s) => s.surveyPrompt);
  const dismissSurveyPrompt = useAppStore((s) => s.dismissSurveyPrompt);

  // Tick every 60s while a goal is active so the elapsed time chip updates.
  const [, setGoalTick] = useState(0);
  useEffect(() => {
    if (
      !goalStatus ||
      goalStatus.state === 'completed' ||
      goalStatus.state === 'exhausted'
    )
      return;
    const id = setInterval(() => setGoalTick((t) => t + 1), 60_000);
    return () => clearInterval(id);
  }, [goalStatus]);

  // Detect if pending approval is from a crew subagent (not the main session)
  const isCrewApproval = !!(
    pendingApproval?.sessionId &&
    sessionId &&
    pendingApproval.sessionId !== sessionId
  );

  const approvalSessionMessages = useSessionConversation(
    pendingApproval?.sessionId ?? ''
  );
  const sessions = useAppStore((state) => state.sessions);
  const questionStageName =
    pendingQuestion?.sessionId && pendingQuestion.sessionId !== sessionId
      ? sessions.get(pendingQuestion.sessionId)?.name
      : undefined;

  const handleCrewConfigure = useCallback(() => {
    enterAltScreen();
    setMode('crew-monitor');
  }, [setMode]);

  // Esc during approval is handled by Panel's useInput → handleClose in
  // ApprovalRequest (drill-in → dropdown, trust → default, dropdown → cancel).

  const [gitBranch, _setGitBranch] = useState(() => getGitBranch());
  const statusBilling = useStatusBilling('tui');

  // Handle Ctrl+O to toggle tool output expansion
  const announcement = useAppStore((s) => s.announcement);
  const toggleAnnouncementExpanded = useAppStore(
    (s) => s.toggleAnnouncementExpanded
  );
  const announcementTruncated = !!announcement;
  const canToggleToolOutputs = toolOutputsExpanded || hasExpandableToolOutputs;

  useKeypress(
    (input, key) => {
      if (key.ctrl && input.toLowerCase() === 'o') {
        if (canToggleToolOutputs) {
          toggleToolOutputsExpanded();
        } else if (announcementTruncated) {
          toggleAnnouncementExpanded();
        }
      }
    },
    {
      isActive: canToggleToolOutputs || announcementTruncated,
    }
  );

  // Ctrl+E enters the full-screen session dashboard (KAS only).
  const setShowSessionDashboard = useAppStore((s) => s.setShowSessionDashboard);
  const inlineAgentEngine = useAppStore((s) => s.agentEngine);
  // Mid-prompt, Ctrl+E means end-of-line (readline). Only an empty prompt
  // lets the chord open the dashboard.
  const inlinePromptEmpty = useAppStore(
    (s) =>
      s.commandInputValue.length === 0 &&
      s.input.lines.every((l) => l.length === 0)
  );
  // Warm the cross-workspace scan cache at boot so the first toggle is instant.
  useEffect(() => {
    if (
      inlineAgentEngine === 'kas' &&
      features.isEnabled(Feature.SessionDashboard)
    ) {
      void scanAllWorkspaceSessions();
    }
  }, [inlineAgentEngine]);
  useKeypress(
    (input, key) => {
      if (keybindings.matches('toggleSessionDashboard', input, key)) {
        if (
          features.isEnabled(Feature.SessionDashboard) &&
          inlineAgentEngine === 'kas' &&
          inlinePromptEmpty &&
          !isProcessing &&
          !backendPanelVisibility.any &&
          !activeCommand &&
          !pendingApproval &&
          !pendingQuestion
        ) {
          // Enter alt screen immediately (before React re-renders) so the
          // full-screen dashboard doesn't pollute main-screen scrollback.
          setShowSessionDashboard(
            true,
            getCachedAllWorkspaceSessions(),
            'ctrl_e'
          );
          enterAltScreen();
          setMode('session-dashboard');
        }
      }
    },
    { isActive: inlineAgentEngine === 'kas' }
  );

  // Handle Esc to collapse expanded outputs
  useKeypress(
    (_input, key) => {
      if (key.escape && toolOutputsExpanded) {
        toggleToolOutputsExpanded();
      }
    },
    { isActive: toolOutputsExpanded }
  );

  // Handle Esc to cancel queue editing
  useKeypress(
    (_input, key) => {
      if (key.escape) {
        cancelEditingQueue();
        clearInput();
        clearCommandInput();
      }
    },
    { isActive: editingQueueIndex != null }
  );

  // Shift+Tab toggles plan mode (shared with LiteLayout).
  usePlanModeToggle(!pendingQuestion);
  useQueuedInputRestore();
  const interactionReady = useInteractionReady(
    pendingQuestion ?? pendingApproval
  );
  const showQuestion = interactionReady ? pendingQuestion : null;
  const showApproval =
    interactionReady && !pendingQuestion ? pendingApproval : null;

  // Build the header - ContextBar
  const promptBarHeader = useMemo(() => {
    if (pendingApproval) {
      const toolCallId = pendingApproval.toolCall.toolCallId;
      const searchMessages = isCrewApproval
        ? approvalSessionMessages
        : messages;
      const toolMessage = searchMessages.find(
        (msg) => msg.role === 'tool_use' && msg.id === toolCallId
      );
      const toolName =
        toolMessage && 'name' in toolMessage ? toolMessage.name : 'Tool';

      // Extract key detail (path, command, etc.) from tool args
      let detail = '';
      if (toolMessage && 'content' in toolMessage && toolMessage.content) {
        try {
          const parsed = JSON.parse(toolMessage.content);
          let value: string =
            parsed.path || parsed.command || parsed.query || '';
          if (value) {
            // Collapse to first line and truncate long values (e.g. commands)
            value = value.split('\n')[0]!;
            value = truncateToWidth(value, 60, '...');
            detail = ` ${glyphs.smallDot} ${value}`;
          }
        } catch {
          /* ignore parse errors */
        }
      }

      return (
        <SnackBar
          title={`${toolName}${detail} requires approval`}
          rightHint="esc to cancel"
          slideIn={true}
        />
      ) as PromptBarHeader;
    }

    return (
      <StatusLine
        {...statusSurface}
        agentName={currentAgent?.name ?? null}
        autonomousModeActive={isAutonomousAgent(currentAgent?.name ?? null)}
        modelName={currentModel?.name ?? null}
        contextUsagePercent={contextUsagePercent}
        gitBranch={gitBranch}
        usagePercent={statusBilling.usagePercent}
        creditsRemaining={statusBilling.creditsRemaining}
      />
    ) as PromptBarHeader;
  }, [
    StatusLine,
    statusBilling,
    pendingApproval,
    messages,
    isCrewApproval,
    approvalSessionMessages,
    currentAgent,
    contextUsagePercent,
    gitBranch,
    currentModel,
    statusSurface,
    glyphs,
  ]);

  // Build a dimmed version of the context bar for when tool outputs are expanded
  const dimmedPromptBarHeader = useMemo(() => {
    if (!toolOutputsExpanded) return null;
    return (
      <StatusLine
        {...statusSurface}
        agentName={currentAgent?.name ?? null}
        autonomousModeActive={isAutonomousAgent(currentAgent?.name ?? null)}
        modelName={currentModel?.name ?? null}
        contextUsagePercent={contextUsagePercent}
        gitBranch={gitBranch}
        goalStatus={null}
        dimmed
        usagePercent={statusBilling.usagePercent}
        creditsRemaining={statusBilling.creditsRemaining}
      />
    ) as PromptBarHeader;
  }, [
    StatusLine,
    statusBilling,
    toolOutputsExpanded,
    currentAgent,
    currentModel,
    contextUsagePercent,
    gitBranch,
    statusSurface,
  ]);

  const handleSubmit = useCallback(
    (value: string) => {
      // Queue edit mode: replace the queued message in place
      if (editingQueueIndex != null) {
        const trimmed = value.trim();
        if (trimmed) {
          replaceQueuedMessage(editingQueueIndex, trimmed);
        } else {
          cancelEditingQueue();
        }
        return;
      }

      if (approvalMode === 'drill-in') {
        const feedback = value.trim().slice(0, 1000);
        respondToApproval(
          'reject_once',
          undefined,
          feedback ? { feedback } : undefined
        );
        return;
      }
      if (value.trim().toLowerCase() === '/kiro') {
        if (allowAnimations) triggerEasterEgg();
        return;
      }
      handleUserInput(value);
    },
    [
      editingQueueIndex,
      replaceQueuedMessage,
      cancelEditingQueue,
      approvalMode,
      respondToApproval,
      handleUserInput,
      allowAnimations,
    ]
  );

  const handleTriggerDetected = useCallback(
    (
      trigger: {
        key: string;
        position: number;
        type: 'start' | 'inline';
      } | null
    ) => {
      setActiveTrigger(trigger);
    },
    [setActiveTrigger]
  );

  // Handler to dismiss blocking error (for recoverable errors)
  const handleDismissError = useCallback(() => {
    setAgentError(null);
  }, [setAgentError]);

  // Pre-first-prompt: the latch (not isInitialized, which flips mid-boot and
  // hid the checklist) keeps /chat new from replaying a stale connect screen.
  // The latch now flips only on the user's first message (see app-store), so
  // the screen survives the cloud prefetch tool calls (fetch_cloud_config, repo
  // clone) that stream in during bring-up. No `messages.length === 0` clause —
  // that would tear the screen down the instant the first prefetch call landed.
  const showCloudConnectScreen =
    cloudSessionActive &&
    !hasEnteredConversation &&
    bootProgress.has('agent_connect');
  const [cloudBootFrame, setCloudBootFrame] = useState(0);
  useEffect(() => {
    if (!showCloudConnectScreen || globalPaused) return;
    const t = setInterval(() => setCloudBootFrame((f) => f + 1), 150);
    return () => clearInterval(t);
  }, [showCloudConnectScreen, globalPaused]);

  const cloudSessionCreated =
    bootProgress.get('session_create')?.status === 'ready';
  const cloudSessionFailed =
    bootProgress.get('session_create')?.status === 'failed';

  // A cloud session's chat UI is unusable until the session is created and
  // linked, so while it is still being created (or creation failed) render only
  // the connect screen — the milestone checklist plus any error — and suppress
  // the conversation, prompt, and footer. Dark-safe: a non-cloud session never
  // enters this branch, so its startup is unchanged.
  // Source-provider gate: a cloud session with no connected provider must show
  // ONLY the gate — no welcome, no checklist, no TUI chrome — until the provider
  // is verified (the session isn't created yet at this point). Highest-priority
  // cloud branch so the connect screen below never renders behind it.
  if (cloudSessionActive && showSourceProviderGate) {
    return (
      <AnimationPausedContext.Provider value={globalPaused}>
        <SourceProviderGate
          setupUrl={sourceProviderSetupUrl ?? null}
          onOpenBrowser={() => {
            openUrlInBrowser(
              sourceProviderSetupUrl ?? SOURCE_PROVIDER_SETUP_URL
            );
          }}
          onRetry={backendPanelHandlers.handleSourceProviderRetry}
          onQuit={backendPanelHandlers.handleSourceProviderQuit}
        />
      </AnimationPausedContext.Provider>
    );
  }

  if (cloudSessionActive && !cloudSessionCreated) {
    return (
      <AnimationPausedContext.Provider value={globalPaused || !!agentError}>
        <Box flexDirection="column">
          {agentError && (
            <BlockingErrorAlert
              message={agentError}
              guidance={agentErrorGuidance ?? undefined}
              onDismiss={handleDismissError}
            />
          )}
          {/* The connecting phase shows ONLY the spinner — no welcome
              banner yet. The welcome + checklist wait until the source-provider
              probe has resolved (cloudProviderChecked), so neither flashes
              before the 2.1 gate can take the screen when no provider is linked.
              Once connected to kiro.dev, the welcome banner renders
              above the checklist. */}
          {cloudProviderChecked &&
            bootProgress.get('agent_connect')?.status === 'ready' && (
              <Box marginBottom={1}>
                <WelcomeScreen agent="kiro" mcpServers={[]} animate={false} />
              </Box>
            )}
          {cloudConnectStage(
            cloudProviderChecked,
            bootProgress.get('agent_connect')?.status
          ) === 'connecting' && (
            <Box flexDirection="column">
              <Text>
                {getColor('secondary')(
                  `  ${
                    inlineSpinners.brailleRotate[
                      cloudBootFrame % inlineSpinners.brailleRotate.length
                    ]
                  } Connecting to kiro.dev${glyphs.ellipsis}`
                )}
              </Text>
            </Box>
          )}
          {cloudConnectStage(
            cloudProviderChecked,
            bootProgress.get('agent_connect')?.status
          ) === 'failed' && (
            // Terminal failure row: a rejected connect leaves cloudProviderChecked
            // false, so without this the spinner above would spin forever beside
            // the error alert. No spinner — the connect is not still in flight.
            <Box flexDirection="column">
              <Text>
                {getColor('error')(
                  `  ${glyphs.cross} Couldn't connect to kiro.dev`
                )}
              </Text>
            </Box>
          )}
          {cloudProviderChecked && (
            <Box flexDirection="column">
              {formatCloudStartupChecklist(
                {
                  connected:
                    bootProgress.get('agent_connect')?.status === 'ready',
                  sessionCreated: false,
                  sessionFailed: cloudSessionFailed,
                  resumed: cloudSessionResumed,
                  provider: cloudProvider ?? undefined,
                  repoCount: cloudRepoCount ?? undefined,
                },
                {
                  check: glyphs.checkmark,
                  cross: glyphs.cross,
                  ellipsis: glyphs.ellipsis,
                  spinner:
                    inlineSpinners.brailleRotate[
                      cloudBootFrame % inlineSpinners.brailleRotate.length
                    ]!,
                }
              ).map((line, i) => (
                <Text key={i}>{line}</Text>
              ))}
            </Box>
          )}
        </Box>
      </AnimationPausedContext.Provider>
    );
  }

  return (
    <AnimationPausedContext.Provider
      value={
        globalPaused || !!pendingApproval || !!pendingQuestion || !!agentError
      }
    >
      <Box flexDirection="column">
        {agentError && (
          <BlockingErrorAlert
            message={agentError}
            guidance={agentErrorGuidance ?? undefined}
            onDismiss={handleDismissError}
          />
        )}

        {/* ConversationView - always rendered. It renders the Kiro welcome
            banner at its top when no message has been sent. */}
        <ConversationView
          questionPanelVisible={!!showQuestion && mode === 'inline'}
        />

        {/* /spec new description-collection intro — live-region only, so it
            vanishes when the user submits a description or cancels. */}
        {specDescriptionFeature && (
          <SpecDescriptionIntro featureName={specDescriptionFeature} />
        )}

        {/* Titles the check-in question below it; spaced off the agent's
            closing message so it doesn't read as the last line of it. */}
        <SpecCheckpointChip
          questionVisible={!!showQuestion && mode === 'inline'}
        />

        {/* Cloud connect screen: milestone checklist while a cloud session is
            booting and no message has been sent yet. Rendered AFTER
            ConversationView so it appears below the welcome banner,
            not above it. Inert unless a cloud session is active, so non-cloud
            startup is unchanged. */}
        {showCloudConnectScreen && (
          <Box flexDirection="column">
            {formatCloudStartupChecklist(
              {
                connected:
                  bootProgress.get('agent_connect')?.status === 'ready',
                sessionCreated:
                  bootProgress.get('session_create')?.status === 'ready',
                resumed: cloudSessionResumed,
                provider: cloudProvider ?? undefined,
                repoCount: cloudRepoCount ?? undefined,
              },
              {
                check: glyphs.checkmark,
                cross: glyphs.cross,
                ellipsis: glyphs.ellipsis,
                spinner:
                  inlineSpinners.brailleRotate[
                    cloudBootFrame % inlineSpinners.brailleRotate.length
                  ]!,
              }
            ).map((line, i) => (
              <Text key={i}>{line}</Text>
            ))}
          </Box>
        )}

        {/* Cloud prefetch tool cards (fetch_cloud_config, repo clone, steering/
            learnings) streamed during bring-up, before the first prompt.
            ConversationView holds these out of the transcript while the connect
            screen is up (so they can't jump above it via <Static>); this block
            shows them live BELOW the checklist + hint as they arrive. On the
            first user message the hold lifts — the same messages commit to the
            conversation scrollback — and this block unmounts with the connect
            screen, so only the checklist/hint disappear. */}
        {showCloudConnectScreen &&
          messages.some(
            (m) => m.role === MessageRole.ToolUse && !m.isSubagentTool
          ) && (
            <Box flexDirection="column" marginTop={1}>
              {messages
                // Subagent tool rows are excluded, mirroring the transcript
                // renderers (they surface via their own subagent panels).
                .filter(
                  (
                    m
                  ): m is Extract<
                    (typeof messages)[number],
                    { role: MessageRole.ToolUse }
                  > => m.role === MessageRole.ToolUse && !m.isSubagentTool
                )
                .map((m) => (
                  <ToolUseMessage
                    key={m.id}
                    id={m.id}
                    name={m.name}
                    isQuestion={m.isQuestion}
                    kind={m.kind}
                    content={m.content}
                    diff={m.diff}
                    isFinished={m.isFinished}
                    isStatic={true}
                    status={m.status}
                    result={m.result}
                    locations={m.locations}
                    purpose={m.purpose}
                    startTime={m.startTime}
                    finishTime={m.finishTime}
                    mcpServerName={m.mcpServerName}
                    denial={m.denial}
                  />
                ))}
            </Box>
          )}

        {/* Banner-less confirmation for a session created mid-conversation;
            armed only after the create resolves, dismissed by the first message. */}
        {cloudNewSessionChecklist && !showCloudConnectScreen && (
          <Box flexDirection="column">
            {formatCloudStartupChecklist(
              {
                connected: true,
                sessionCreated: true,
                resumed: false,
                provider: cloudProvider ?? undefined,
                repoCount: cloudRepoCount ?? undefined,
              },
              {
                check: glyphs.checkmark,
                cross: glyphs.cross,
                ellipsis: glyphs.ellipsis,
                spinner:
                  inlineSpinners.brailleRotate[
                    cloudBootFrame % inlineSpinners.brailleRotate.length
                  ]!,
              }
            ).map((line, i) => (
              <Text key={i}>{line}</Text>
            ))}
          </Box>
        )}

        <NotificationBar
          message={
            !sessionId
              ? 'Initializing...'
              : (loadingMessage ??
                transientAlert?.message ??
                (oauthBannerVisible
                  ? `${pendingOAuthServers.keys().next().value} requires OAuth — Ctrl+y to authenticate`
                  : undefined) ??
                summarizeInitErrors(
                  initErrors.filter(
                    (e) =>
                      e.type !== 'mcp_governance_disabled' &&
                      e.type !== 'web_tools_governance_disabled'
                  )
                ) ??
                undefined)
          }
          status={
            !sessionId || loadingMessage
              ? 'loading'
              : (transientAlert?.status ??
                (oauthBannerVisible
                  ? 'info'
                  : initErrors.some(
                        (e) =>
                          e.type !== 'mcp_governance_disabled' &&
                          e.type !== 'web_tools_governance_disabled'
                      )
                    ? severityForInitErrors(initErrors)
                    : undefined))
          }
          autoHideMs={
            !sessionId || loadingMessage
              ? undefined
              : transientAlert?.autoHideMs
          }
          onDismiss={
            !sessionId || loadingMessage
              ? undefined
              : transientAlert
                ? dismissTransientAlert
                : undefined
          }
          actionHint={
            transientAlert?.action
              ? `${transientAlert.action.key}: ${transientAlert.action.label}`
              : oauthBannerVisible
                ? 'Ctrl+y: Authenticate'
                : undefined
          }
        />

        <ActivityTray />

        <ArtifactGenerationCard />

        {surveyPrompt && (
          <SurveyPromptBar
            message={surveyPrompt.message}
            onDismiss={dismissSurveyPrompt}
          />
        )}

        {trustAllToolsAccepted && <TrustAllToolsBanner />}
        <Box marginBottom={1}>
          <ActivityTrayAwarePromptBar
            header={
              backendPanelVisibility.inlineHeader ||
              !!pendingApproval ||
              !!pendingQuestion ||
              !!voiceDownloadConfirm
                ? undefined
                : toolOutputsExpanded
                  ? (dimmedPromptBarHeader ?? undefined)
                  : promptBarHeader
            }
            subHeader={undefined}
            onSubmit={handleSubmit}
            triggerRules={TRIGGER_RULES}
            onTriggerDetected={handleTriggerDetected}
            isProcessing={
              editingQueueIndex != null
                ? false
                : isProcessing ||
                  isCompacting ||
                  !!activeCommand ||
                  !!agentError
            }
            placeholder={getPlaceholder({
              glyphs,
              editingQueueIndex,
              pendingApproval: !!pendingApproval || !!pendingQuestion,
              isShellEscape,
              isProcessing,
              isInitialized,
              pendingSteerContent,
              activeInterruptMode,
              toggleHintLabel,
              agentName: currentAgent?.name,
              tangentName,
              specDescriptionFeature,
              goalStatus,
              goalCancelFailed,
              cancelLabel: keybindings.label('cancelStream'),
              quitLabel: keybindings.label('quit'),
            })}
            hint={
              promptHint ||
              (activeCommand?.command.meta?.hint as string | undefined) ||
              (activeCommand && activeCommand.command.meta?.searchable === false
                ? activeCommand.command.description
                : undefined)
            }
            hideInput={
              editingQueueIndex != null
                ? false
                : mode === 'session-view' ||
                  mode === 'crew-monitor' ||
                  toolOutputsExpanded ||
                  noInteractive ||
                  !!pendingApproval ||
                  !!pendingQuestion ||
                  !!voiceDownloadConfirm ||
                  backendPanelVisibility.inlineInput
            }
          >
            {!pendingQuestion && <CommandMenu />}
            {showQuestion && mode === 'inline' && (
              <Question
                key={`${showQuestion.sessionId}:${showQuestion.toolCallId}`}
                question={showQuestion.question}
                options={checkpointOptions(showQuestion.options)}
                onAnswer={(answer, answerForAgent) =>
                  answerCheckpoint(answer, answerForAgent, showQuestion)
                }
                onCancel={() => void cancelMessage()}
                titlePrefix={
                  questionStageName ? `${questionStageName} > ` : undefined
                }
                freeTextIsFeedback={specCheckpointActive}
              />
            )}
            {showApproval &&
              mode === 'inline' &&
              (isCrewApproval ? (
                <CrewApprovalRequest onConfigure={handleCrewConfigure} />
              ) : (
                <ApprovalPrompt
                  key={showApproval.toolCall.toolCallId}
                  messages={messages}
                  approval={showApproval}
                  respondToApproval={respondToApproval}
                  getStageInputColor={(stageName: string) =>
                    getAgentColor(stageName, getColor)
                  }
                  mainAgentName={currentAgent?.name ?? null}
                  onInputSubmit={handleSubmit}
                />
              ))}
            {voiceDownloadConfirm && mode === 'inline' && (
              <VoiceModelDownloadGate
                info={voiceDownloadConfirm.info}
                onConfirm={voiceDownloadConfirm.onConfirm}
                onDecline={voiceDownloadConfirm.onDecline}
              />
            )}
            {!pendingQuestion && (
              <BackendPanels handlers={backendPanelHandlers} surface="tui" />
            )}
            <ActionHint
              text={`Showing detailed output ${glyphs.smallDot} ctrl+o to toggle`}
              visible={toolOutputsExpanded}
              overlay={{
                badge: 'Viewing detailed tool output',
                hint: 'Press Ctrl+O to return to chat',
              }}
            />
            <ActionHint
              text={
                inlineAgentEngine === 'kas' &&
                features.isEnabled(Feature.SessionDashboard)
                  ? `/sessions to resume ${glyphs.smallDot} /copy to clipboard`
                  : '/copy to clipboard'
              }
              visible={
                !toolOutputsExpanded &&
                !isProcessing &&
                !pendingApproval &&
                !pendingQuestion &&
                !activeCommand &&
                !backendPanelVisibility.inlineCopyHint &&
                commandInputValue.length === 0 &&
                exitSequence === 0 &&
                !suspendArmed
              }
            />
            <ExitHint />
          </ActivityTrayAwarePromptBar>
        </Box>
      </Box>
    </AnimationPausedContext.Provider>
  );
};

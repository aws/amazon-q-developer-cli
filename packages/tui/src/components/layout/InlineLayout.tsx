import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { Box, Text } from './../../renderer.js';
import { useRenderMetrics, isDevMode } from '../../hooks/useRenderMetrics.js';
import { truncateToWidth } from '../../utils/text-width.js';
import { ModeChangeSource } from '../../types/generated/chat-cli.js';

// Region is twinki-only — lazy import for dev mode metrics
const Region = isDevMode()
  ? (await import('twinki').catch(() => ({ Region: null }))).Region
  : null;
import {
  AnimationPausedContext,
  useAnimationPaused,
} from '../../contexts/AnimationPausedContext.js';
import { ConversationView } from '../ui/ConversationView';
import { ActivityTray } from '../ui/activity-tray/index.js';
import { ExitHint } from '../ui/ExitHint';
import { CommandMenu } from '../ui/CommandMenu';
import { ActionHint } from '../ui/hint/ActionHint.js';
import {
  PromptBar,
  type PromptBarHeader,
} from '../chat/prompt-bar/PromptBar.js';
import { ContextBar } from '../chat/prompt-bar/ContextBar.js';
import { SnackBar } from '../chat/prompt-bar/SnackBar.js';
import { NotificationBar } from '../chat/notification-bar/NotificationBar.js';
import { BlockingErrorAlert } from '../ui/alert/BlockingErrorAlert.js';
import { Chip, ChipColor, ProgressChip } from '../ui/chip/index.js';
import { ApprovalRequest } from '../ui/ApprovalRequest.js';
import { CrewApprovalRequest } from '../ui/CrewApprovalRequest.js';
import { TrustAllToolsBanner } from '../ui/TrustAllToolsBanner.js';
import { SurveyPromptBar } from '../ui/SurveyPromptBar';
import { ArtifactGenerationCard } from '../ui/ArtifactView/ArtifactGenerationCard.js';
import { BackendPanels } from './shared/BackendPanels.js';
import { useBackendPanelHandlers } from './shared/useBackendPanelHandlers.js';

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
  useKiroClient,
} from '../../stores/selectors.js';
import {
  useAppStore,
  summarizeInitErrors,
  severityForInitErrors,
} from '../../stores/app-store.js';
import { useSessionConversation } from '../../stores/session-conversations.js';
import { useShallow } from 'zustand/react/shallow';
import { useKeypress } from '../../hooks/useKeypress';
import {
  resolveKeybinding,
  formatKeybinding,
} from '../../utils/keybindings.js';
import { useKeybindings } from '../../hooks/useKeybindings.js';
import { getPlaceholder } from './getPlaceholder.js';
import { getGitBranch } from '../../utils/git';
import { shortenPath, formatEffort } from '../../utils/string';
import { getAgentColor, getAgentDisplayName } from '../../utils/agentColors.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs, useAllowAnimations } from '../../hooks/useGlyphs.js';

const TRIGGER_RULES = [
  { key: '/', type: 'start' as const },
  { key: '@', type: 'inline' as const },
];

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

/** Only mounted when KIRO_DEV=1 — keeps the hook out of production renders. */
const RenderMetricsChip: React.FC<{
  color?: ChipColor | ((text: string) => string);
}> = ({ color }) => {
  const metrics = useRenderMetrics();
  const glyphs = useGlyphs();
  if (!metrics) return null;
  return (
    <Chip
      value={`${metrics.lastRenderMs.toFixed(1)}ms ${glyphs.smallDot} ${metrics.yogaNodeCount}n ${glyphs.smallDot} ${metrics.heapUsedMB}MB ${glyphs.smallDot} #${metrics.renderCount} ${glyphs.smallDot} r${metrics.fullRedrawCount}`}
      color={color ?? ChipColor.PRIMARY}
    />
  );
};

export const InlineLayout: React.FC = () => {
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
  const {
    dismissTransientAlert,
    setAgentError,
    setLoadingMessage,
    showTransientAlert,
  } = useNotificationActions();
  const {
    isProcessing,
    isCompacting,
    isShellEscape,
    pendingApproval,
    noInteractive,
  } = useProcessingState();
  const { respondToApproval, approvalMode } = useApprovalState();
  const globalPaused = useAnimationPaused();
  const keybindings = useKeybindings();
  const trustAllToolsAccepted = useAppStore(
    (state) => state.trustAllToolsConfirmed
  );
  // Panel show-flags drive PromptBar header/hideInput gating; the panels
  // themselves render via the shared <BackendPanels> cluster.
  const {
    toolOutputsExpanded,
    hasExpandableToolOutputs,
    showContextBreakdown,
    showTuiPanel,
    showChangelogPanel,
    showHelpPanel,
    showUsagePanel,
    showRewindExplorer,
    showMcpPanel,
    showToolsPanel,
    showGoalPanel,
    showStatsPanel,
    showHooksPanel,
    showKeybindingsPanel,
    showDisplaySettingsPanel,
    showThemePanel,
    showSettingsPanel,
    showKnowledgePanel,
    showCodePanel,
    artifactViewOpen,
  } = useUIState();
  const { toggleToolOutputsExpanded } = useUIActions();
  const {
    sessionId,
    contextUsagePercent,
    currentModel,
    currentEffort,
    currentAgent,
    previousAgentName,
    codeIntelligenceActive,
    goalStatus,
  } = useContextState();
  const activeCommand = useAppStore((state) => state.activeCommand);
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
  const settings = useAppStore((s) => s.settings);
  const { kiro } = useKiroClient();
  const mode = useAppStore((state) => state.mode);
  const backendPanelHandlers = useBackendPanelHandlers();

  const toggleHintLabel = useMemo(() => {
    const binding = resolveKeybinding(settings, 'toggleInterruptMode');
    return formatKeybinding(binding);
  }, [settings]);
  const setMode = useAppStore((state) => state.setMode);
  const exitSequence = useAppStore((state) => state.exitSequence);
  const suspendArmed = useAppStore((state) => state.suspendArmed);

  // Research-survey state. showSurveyPanel gates PromptBar chrome; the panel
  // itself renders via <BackendPanels>. surveyPrompt drives the inline nudge.
  const showSurveyPanel = useAppStore((s) => s.showSurveyPanel);
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

  const handleCrewConfigure = useCallback(() => {
    process.stdout.write('\x1b[?1049h');
    setMode('crew-monitor');
  }, [setMode]);

  // Esc during approval is handled by Panel's useInput → handleClose in
  // ApprovalRequest (drill-in → dropdown, trust → default, dropdown → cancel).

  const { setCurrentAgent, setPreviousAgentName } = useAppStore(
    useShallow((s) => ({
      setCurrentAgent: s.setCurrentAgent,
      setPreviousAgentName: s.setPreviousAgentName,
    }))
  );

  const [gitBranch, _setGitBranch] = useState(() => getGitBranch());

  // Handle Ctrl+O to toggle tool output expansion
  const announcement = useAppStore((s) => s.announcement);
  const toggleAnnouncementExpanded = useAppStore(
    (s) => s.toggleAnnouncementExpanded
  );
  const announcementTruncated = !!announcement;

  useKeypress(
    (input, key) => {
      if (key.ctrl && input.toLowerCase() === 'o') {
        if (hasExpandableToolOutputs) {
          toggleToolOutputsExpanded();
        } else if (announcementTruncated) {
          toggleAnnouncementExpanded();
        }
      }
    },
    { isActive: hasExpandableToolOutputs || announcementTruncated }
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

  // Handle Shift+Tab for agent switching
  useKeypress(
    (_input, key) => {
      if (key.tab && key.shift) {
        const currentName = currentAgent?.name;

        // Emit a Shift+Tab mode-change telemetry event when the agent
        // actually changed. Centralized so both branches stay in sync if
        // the payload shape grows or a new entry point is added.
        const emitModeChange = (
          from: string | undefined,
          to: string | undefined
        ) => {
          if (from && to && from !== to) {
            kiro.sendModeChanged({
              fromMode: from,
              toMode: to,
              source: ModeChangeSource.ShiftTab,
              sessionId: kiro.sessionId,
            });
          }
        };

        // Replace any in-flight toast (e.g. stale "Switched to spec" from a
        // prior /agent command) with a fresh one for this swap so rapid
        // Shift+Tab presses don't keep showing the previous target's label.
        // Uses the raw id (not the display name) to match the /agent toast.
        const announceSwitch = (name: string) => {
          showTransientAlert({
            message: `Switched to ${name}`,
            status: 'success',
            autoHideMs: 2000,
          });
        };

        if (currentName === 'kiro_planner') {
          const target = previousAgentName;
          if (!target) return;
          setLoadingMessage(`Agent changing to ${target}`);
          kiro
            .executeCommand({ command: 'agent', args: { agentName: target } })
            .then((result) => {
              setLoadingMessage(null);
              if (result?.success) {
                const name = (result.data as any)?.agent?.name;
                emitModeChange(currentName, name);
                if (name) {
                  setCurrentAgent({ name });
                  announceSwitch(name);
                }
              }
            })
            .catch(() => setLoadingMessage(null));
        } else {
          if (currentName) setPreviousAgentName(currentName);
          setLoadingMessage('Agent changing to kiro_planner');
          kiro
            .executeCommand({
              command: 'agent',
              args: { agentName: 'kiro_planner' },
            })
            .then((result) => {
              setLoadingMessage(null);
              if (result?.success) {
                const name = (result.data as any)?.agent?.name;
                emitModeChange(currentName, name);
                if (name) {
                  setCurrentAgent({ name });
                  announceSwitch(name);
                }
              }
            })
            .catch(() => setLoadingMessage(null));
        }
      }
    },
    { isActive: true }
  );

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

    const primaryItems = [
      currentAgent && (
        <Chip
          value={getAgentDisplayName(currentAgent.name)}
          color={getAgentColor(currentAgent.name, getColor)}
        />
      ),
      currentModel && (
        <Chip value={currentModel.name} color={ChipColor.PRIMARY} />
      ),
      currentEffort && (
        <Chip value={formatEffort(currentEffort)} color={ChipColor.SECONDARY} />
      ),
      contextUsagePercent != null && (
        <ProgressChip value={contextUsagePercent} warningThreshold={60} />
      ),
      codeIntelligenceActive && <Text>{getColor('primary')('λ')}</Text>,
      goalStatus &&
        (() => {
          const icon =
            goalStatus.state === 'paused'
              ? glyphs.pause
              : goalStatus.state === 'completed'
                ? glyphs.checkmark
                : goalStatus.state === 'exhausted'
                  ? glyphs.cross
                  : '⟳';
          const label =
            goalStatus.state === 'paused'
              ? 'Paused'
              : goalStatus.state === 'completed'
                ? 'Done'
                : goalStatus.state === 'exhausted'
                  ? 'Exhausted'
                  : `Active [${goalStatus.iteration + 1}/${goalStatus.maxIterations}]`;
          const secs = goalStatus.startedAt
            ? Math.floor((Date.now() - goalStatus.startedAt) / 1000)
            : (goalStatus.elapsedSecs ?? 0);
          const elapsed =
            secs > 0
              ? secs >= 3600
                ? `${Math.floor(secs / 3600)}h${Math.floor((secs % 3600) / 60)}m`
                : secs >= 60
                  ? `${Math.floor(secs / 60)}m`
                  : `${secs}s`
              : '';
          return (
            <Chip
              value={`${icon} Goal ${label}${elapsed ? ` ${glyphs.smallDot} ${elapsed}` : ''}`}
              color={
                goalStatus.state === 'completed'
                  ? ChipColor.SUCCESS
                  : goalStatus.state === 'exhausted'
                    ? ChipColor.ERROR
                    : ChipColor.SECONDARY
              }
            />
          );
        })(),
    ];

    const secondaryItems = [
      isDevMode() && Region ? (
        <Region id="metrics">
          <RenderMetricsChip />
        </Region>
      ) : (
        isDevMode() && <RenderMetricsChip />
      ),
      <Chip value={shortenPath(process.cwd())} color={ChipColor.BRAND} />,
      gitBranch && (
        <Chip value={gitBranch} color={ChipColor.PRIMARY} wrap={true} />
      ),
    ];

    return (
      <ContextBar primaryItems={primaryItems} secondaryItems={secondaryItems} />
    ) as PromptBarHeader;
  }, [
    pendingApproval,
    messages,
    isCrewApproval,
    approvalSessionMessages,
    currentAgent,
    contextUsagePercent,
    codeIntelligenceActive,
    gitBranch,
    currentModel,
    currentEffort,
    goalStatus,
    getColor,
    glyphs,
  ]);

  // Build a dimmed version of the context bar for when tool outputs are expanded
  const dimmedPromptBarHeader = useMemo(() => {
    if (!toolOutputsExpanded) return null;
    const mutedColor = getColor('muted');

    const primaryItems = [
      currentAgent && (
        <Chip
          value={getAgentDisplayName(currentAgent.name)}
          color={mutedColor}
        />
      ),
      currentModel && <Chip value={currentModel.name} color={mutedColor} />,
      currentEffort && (
        <Chip value={formatEffort(currentEffort)} color={mutedColor} />
      ),
      contextUsagePercent != null && (
        <ProgressChip
          value={contextUsagePercent}
          warningThreshold={60}
          colorOverride={mutedColor}
        />
      ),
      codeIntelligenceActive && <Text>{mutedColor('λ')}</Text>,
    ];

    const secondaryItems = [
      isDevMode() && Region ? (
        <Region id="metrics">
          <RenderMetricsChip color={mutedColor} />
        </Region>
      ) : (
        isDevMode() && <RenderMetricsChip color={mutedColor} />
      ),
      <Chip value={shortenPath(process.cwd())} color={mutedColor} />,
      gitBranch && <Chip value={gitBranch} color={mutedColor} wrap={true} />,
    ];

    return (
      <ContextBar primaryItems={primaryItems} secondaryItems={secondaryItems} />
    ) as PromptBarHeader;
  }, [
    toolOutputsExpanded,
    currentAgent,
    currentModel,
    currentEffort,
    contextUsagePercent,
    codeIntelligenceActive,
    gitBranch,
    getColor,
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

  return (
    <AnimationPausedContext.Provider
      value={globalPaused || !!pendingApproval || !!agentError}
    >
      <Box flexDirection="column">
        {agentError && (
          <BlockingErrorAlert
            message={agentError}
            guidance={agentErrorGuidance ?? undefined}
            onDismiss={handleDismissError}
          />
        )}

        {/* ConversationView - always rendered */}
        <ConversationView />

        <NotificationBar
          message={
            !sessionId
              ? 'Initializing...'
              : (loadingMessage ??
                transientAlert?.message ??
                (pendingOAuthServers.size > 0
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
                (pendingOAuthServers.size > 0
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
              : pendingOAuthServers.size > 0
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
          <PromptBar
            header={
              showContextBreakdown ||
              showHelpPanel ||
              showTuiPanel ||
              showChangelogPanel ||
              showUsagePanel ||
              showRewindExplorer ||
              showMcpPanel ||
              showToolsPanel ||
              showGoalPanel ||
              showStatsPanel ||
              showHooksPanel ||
              showKeybindingsPanel ||
              showDisplaySettingsPanel ||
              showThemePanel ||
              showSettingsPanel ||
              showKnowledgePanel ||
              showCodePanel ||
              !!artifactViewOpen ||
              showSurveyPanel ||
              !!pendingApproval
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
              pendingApproval: !!pendingApproval,
              isShellEscape,
              isProcessing,
              isInitialized,
              pendingSteerContent,
              activeInterruptMode,
              toggleHintLabel,
              agentName: currentAgent?.name,
              goalStatus,
              cancelLabel: keybindings.label('cancelStream'),
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
                  showContextBreakdown ||
                  showHelpPanel ||
                  showTuiPanel ||
                  showChangelogPanel ||
                  showUsagePanel ||
                  showRewindExplorer ||
                  showMcpPanel ||
                  showToolsPanel ||
                  showStatsPanel ||
                  showHooksPanel ||
                  showKeybindingsPanel ||
                  showDisplaySettingsPanel ||
                  showThemePanel ||
                  showSettingsPanel ||
                  showKnowledgePanel ||
                  showCodePanel ||
                  !!artifactViewOpen ||
                  showSurveyPanel
            }
          >
            <CommandMenu />
            {pendingApproval &&
              mode === 'inline' &&
              (isCrewApproval ? (
                <CrewApprovalRequest onConfigure={handleCrewConfigure} />
              ) : (
                <ApprovalRequest
                  key={pendingApproval?.toolCall.toolCallId}
                  onDrillInSubmit={handleSubmit}
                />
              ))}
            <BackendPanels handlers={backendPanelHandlers} />
            <ActionHint
              text={`Showing detailed output ${glyphs.smallDot} ctrl+o to toggle`}
              visible={toolOutputsExpanded}
              overlay={{
                badge: 'Viewing detailed tool output',
                hint: 'Press Ctrl+O to return to chat',
              }}
            />
            <ActionHint
              text="/copy to clipboard"
              visible={
                !toolOutputsExpanded &&
                !isProcessing &&
                !pendingApproval &&
                !activeCommand &&
                !showContextBreakdown &&
                !showHelpPanel &&
                !showTuiPanel &&
                !showChangelogPanel &&
                !showUsagePanel &&
                !showRewindExplorer &&
                !showMcpPanel &&
                !showToolsPanel &&
                !showHooksPanel &&
                !showKeybindingsPanel &&
                !showDisplaySettingsPanel &&
                !showThemePanel &&
                !showSettingsPanel &&
                !showKnowledgePanel &&
                !showCodePanel &&
                !artifactViewOpen &&
                !showSurveyPanel &&
                commandInputValue.length === 0 &&
                exitSequence === 0 &&
                !suspendArmed
              }
            />
            <ExitHint />
          </PromptBar>
        </Box>
      </Box>
    </AnimationPausedContext.Provider>
  );
};

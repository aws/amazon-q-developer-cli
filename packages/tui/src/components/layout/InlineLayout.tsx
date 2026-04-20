import React, { useCallback, useMemo, useState } from 'react';
import { Box, Text } from './../../renderer.js';
import { useRenderMetrics, isDevMode } from '../../hooks/useRenderMetrics.js';
import { truncateToWidth } from '../../utils/text-width.js';

// Region is twinki-only — lazy import for dev mode metrics
const Region = isDevMode()
  ? (await import('twinki').catch(() => ({ Region: null }))).Region
  : null;
import { AnimationPausedContext } from '../../contexts/AnimationPausedContext.js';
import { ConversationView } from '../ui/ConversationView';
import { ActivityTray } from '../ui/activity-tray/index.js';
import { ExitHint } from '../ui/ExitHint';
import { CommandMenu } from '../ui/CommandMenu';
import { ActionHint } from '../ui/hint/ActionHint.js';
import { HelpPanel } from '../ui/HelpPanel';
import { TuiPanel } from '../ui/TuiPanel';
import { McpPanel } from '../ui/McpPanel';
import { ToolsPanel } from '../ui/ToolsPanel';
import { HooksPanel } from '../ui/HooksPanel';
import { KnowledgePanel } from '../ui/KnowledgePanel';
import {
  PromptBar,
  type PromptBarHeader,
} from '../chat/prompt-bar/PromptBar.js';
import { ContextBar } from '../chat/prompt-bar/ContextBar.js';
import { SnackBar } from '../chat/prompt-bar/SnackBar.js';
import { NotificationBar } from '../chat/notification-bar/NotificationBar.js';
import { BlockingErrorAlert } from '../ui/alert/BlockingErrorAlert.js';
import { Chip, ChipColor, ProgressChip } from '../ui/chip/index.js';
import { ContextBreakdown } from '../ui/ContextBreakdown';
import { ApprovalRequest } from '../ui/ApprovalRequest.js';
import { CrewApprovalRequest } from '../ui/CrewApprovalRequest.js';
import { TrustAllToolsBanner } from '../ui/TrustAllToolsBanner.js';
import { UsagePanel } from '../ui/UsagePanel';
import { CodePanel } from '../ui/CodePanel';

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
  useKiroClient,
} from '../../stores/selectors.js';
import {
  useAppStore,
  summarizeInitErrors,
  type CodePanelData,
  type McpServerInfo,
} from '../../stores/app-store.js';
import { useSessionConversation } from '../../stores/session-conversations.js';
import { useShallow } from 'zustand/react/shallow';
import { useKeypress } from '../../hooks/useKeypress';
import { getGitBranch } from '../../utils/git';
import { shortenPath } from '../../utils/string';
import { getAgentColor } from '../../utils/agentColors.js';
import { useTheme } from '../../hooks/useThemeContext.js';

const TRIGGER_RULES = [
  { key: '/', type: 'start' as const },
  { key: '@', type: 'inline' as const },
];

function getPlaceholder(opts: {
  editingQueueIndex: number | null;
  pendingApproval: boolean;
  isShellEscape: boolean;
  isProcessing: boolean;
  queuedMessages: string[];
  agentName: string | undefined;
}): string {
  if (opts.editingQueueIndex != null) {
    return `Editing queued message ${opts.editingQueueIndex + 1} · esc to cancel`;
  }
  if (opts.pendingApproval || opts.isProcessing) {
    return opts.queuedMessages.length > 0
      ? 'Kiro is working · type to queue another message'
      : 'Kiro is working · type to queue a message';
  }
  if (opts.isShellEscape) {
    return 'running shell command · ctrl+c to cancel';
  }
  if (opts.agentName === 'kiro_planner') {
    return 'ask a question or describe a task ↵  ·  exit plan mode: shift+tab';
  }
  return 'ask a question or describe a task ↵';
}

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
  if (!metrics) return null;
  return (
    <Chip
      value={`${metrics.lastRenderMs.toFixed(1)}ms · ${metrics.yogaNodeCount}n · ${metrics.heapUsedMB}MB · #${metrics.renderCount} · r${metrics.fullRedrawCount}`}
      color={color ?? ChipColor.PRIMARY}
    />
  );
};

export const InlineLayout: React.FC = () => {
  const { getColor } = useTheme();
  // Grouped selectors using useShallow - prevents re-render cascades
  const {
    transientAlert,
    loadingMessage,
    agentError,
    agentErrorGuidance,
    initErrors,
    pendingOAuthServers,
  } = useNotificationState();
  const { dismissTransientAlert, setAgentError, setLoadingMessage } =
    useNotificationActions();
  const {
    isProcessing,
    isCompacting,
    isShellEscape,
    pendingApproval,
    noInteractive,
  } = useProcessingState();
  const { cancelApproval, approvalMode } = useApprovalState();
  const trustAllToolsAccepted = useAppStore(
    (state) => state.trustAllToolsConfirmed
  );
  const {
    toolOutputsExpanded,
    hasExpandableToolOutputs,
    showContextBreakdown,
    contextBreakdown,
    showTuiPanel,
    showHelpPanel,
    helpCommands,
    showUsagePanel,
    usageData,
    showMcpPanel,
    mcpServers,
    mcpRegistryServers,
    mcpMode,
    showToolsPanel,
    toolsList,
    showHooksPanel,
    hooksList,
    showKnowledgePanel,
    knowledgeEntries,
    knowledgeStatus,
    showCodePanel,
    codeData,
  } = useUIState();
  const {
    toggleToolOutputsExpanded,
    setShowContextBreakdown,
    setShowHelpPanel,
    setShowTuiPanel,
    setShowUsagePanel,
    setShowMcpPanel,
    setShowToolsPanel,
    setShowHooksPanel,
    setShowKnowledgePanel,
    setShowCodePanel,
  } = useUIActions();
  const {
    sessionId,
    contextUsagePercent,
    currentModel,
    currentAgent,
    previousAgentName,
    codeIntelligenceActive,
  } = useContextState();
  const activeCommand = useAppStore((state) => state.activeCommand);
  const promptHint = useAppStore((state) => state.promptHint);
  const commandInputValue = useAppStore((state) => state.commandInputValue);
  const { setActiveCommand, setActiveTrigger, clearCommandInput } =
    useCommandActions();
  const { handleUserInput, clearInput } = useInputActions();
  const { messages } = useConversationState();
  const { editingQueueIndex, queuedMessages } = useQueueState();
  const { replaceQueuedMessage, cancelEditingQueue } = useQueueActions();
  const { kiro } = useKiroClient();
  const mode = useAppStore((state) => state.mode);
  const setMode = useAppStore((state) => state.setMode);
  const exitSequence = useAppStore((state) => state.exitSequence);

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

  // Handle escape to cancel queue editing
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

  const { setCurrentAgent, setPreviousAgentName } = useAppStore(
    useShallow((s) => ({
      setCurrentAgent: s.setCurrentAgent,
      setPreviousAgentName: s.setPreviousAgentName,
    }))
  );

  const [gitBranch, setGitBranch] = useState(() => getGitBranch());

  // Handle Ctrl+O to toggle tool output expansion
  const announcement = useAppStore((s) => s.announcement);
  const toggleAnnouncementExpanded = useAppStore(
    (s) => s.toggleAnnouncementExpanded
  );
  const announcementTruncated =
    !!announcement &&
    announcement.content.split('\n').length > announcement.maxLines;

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

  // Handle Shift+Tab for agent switching
  useKeypress(
    (_input, key) => {
      if (key.tab && key.shift) {
        const currentName = currentAgent?.name;
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
                if (name) setCurrentAgent({ name });
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
                if (name) setCurrentAgent({ name });
              }
            })
            .catch(() => setLoadingMessage(null));
        }
      }
    },
    { isActive: true }
  );

  const handleCloseContextBreakdown = useCallback(() => {
    setShowContextBreakdown(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowContextBreakdown, setActiveCommand, clearCommandInput]);

  const handleCloseHelpPanel = useCallback(() => {
    setShowHelpPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowHelpPanel, setActiveCommand, clearCommandInput]);

  const handleCloseTuiPanel = useCallback(() => {
    setShowTuiPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowTuiPanel, setActiveCommand, clearCommandInput]);

  const handleCloseUsagePanel = useCallback(() => {
    setShowUsagePanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowUsagePanel, setActiveCommand, clearCommandInput]);

  const handleTabFromContext = useCallback(async () => {
    try {
      const result = await kiro.executeCommand({
        command: 'usage',
        args: {},
      } as any);
      if (result?.data) {
        setShowUsagePanel(true, result.data);
        setShowContextBreakdown(false);
      }
    } catch {
      /* ignore */
    }
  }, [setShowContextBreakdown, setShowUsagePanel, kiro]);

  const handleTabFromUsage = useCallback(async () => {
    try {
      const result = await kiro.executeCommand({
        command: 'context',
        args: {},
      } as any);
      if (
        result?.data &&
        typeof result.data === 'object' &&
        'breakdown' in result.data
      ) {
        setShowContextBreakdown(true, result.data.breakdown as any);
        setShowUsagePanel(false);
      }
    } catch {
      /* ignore */
    }
  }, [setShowUsagePanel, setShowContextBreakdown, kiro]);

  const handleCloseMcpPanel = useCallback(() => {
    setShowMcpPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowMcpPanel, setActiveCommand, clearCommandInput]);

  // Overlay auth-required status onto MCP servers that are pending OAuth
  const mcpServersWithAuth = useMemo(() => {
    if (pendingOAuthServers.size === 0) return mcpServers;
    return mcpServers.map((s) =>
      pendingOAuthServers.has(s.name)
        ? { ...s, status: 'auth-required' as const }
        : s
    );
  }, [mcpServers, pendingOAuthServers]);

  const handleCloseToolsPanel = useCallback(() => {
    setShowToolsPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowToolsPanel, setActiveCommand, clearCommandInput]);

  const handleCloseHooksPanel = useCallback(() => {
    setShowHooksPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowHooksPanel, setActiveCommand, clearCommandInput]);

  const handleCloseKnowledgePanel = useCallback(() => {
    setShowKnowledgePanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowKnowledgePanel, setActiveCommand, clearCommandInput]);

  const handleCloseCodePanel = useCallback(() => {
    setShowCodePanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowCodePanel, setActiveCommand, clearCommandInput]);

  const handleRefreshCodePanel = useCallback(async () => {
    try {
      const result = await kiro.executeCommand({
        command: 'code',
        args: {},
      } as any);
      if (result?.data) {
        setShowCodePanel(true, result.data as CodePanelData);
      }
    } catch {
      /* ignore */
    }
  }, [kiro, setShowCodePanel]);

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
            detail = ` · ${value}`;
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
        <>
          {currentAgent.name === 'kiro_planner' && (
            <Text color="magenta">[plan] </Text>
          )}
          <Chip
            value={
              currentAgent.name === 'kiro_default' ? 'Kiro' : currentAgent.name
            }
            color={getAgentColor(currentAgent.name, getColor)}
          />
        </>
      ),
      currentModel && (
        <Chip value={currentModel.name} color={ChipColor.PRIMARY} />
      ),
      contextUsagePercent != null && (
        <ProgressChip value={contextUsagePercent} warningThreshold={60} />
      ),
      codeIntelligenceActive && <Text>{getColor('primary')('λ')}</Text>,
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
    getColor,
  ]);

  // Build a dimmed version of the context bar for when tool outputs are expanded
  const dimmedPromptBarHeader = useMemo(() => {
    if (!toolOutputsExpanded) return null;
    const mutedColor = getColor('muted');

    const primaryItems = [
      currentAgent && (
        <Chip
          value={
            currentAgent.name === 'kiro_default' ? 'Kiro' : currentAgent.name
          }
          color={mutedColor}
        />
      ),
      currentModel && <Chip value={currentModel.name} color={mutedColor} />,
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
        cancelApproval();
        if (value.trim()) handleUserInput(value.trim());
        return;
      }
      if (value.trim().toLowerCase() === '/kiro') {
        triggerEasterEgg();
        return;
      }
      handleUserInput(value);
    },
    [
      editingQueueIndex,
      replaceQueuedMessage,
      cancelEditingQueue,
      approvalMode,
      cancelApproval,
      handleUserInput,
      setGitBranch,
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
    <AnimationPausedContext.Provider value={!!pendingApproval || !!agentError}>
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
                  ? `${pendingOAuthServers.keys().next().value} requires OAuth — Ctrl+y to copy URL`
                  : undefined) ??
                summarizeInitErrors(initErrors) ??
                undefined)
          }
          status={
            !sessionId || loadingMessage
              ? 'loading'
              : (transientAlert?.status ??
                (pendingOAuthServers.size > 0
                  ? 'info'
                  : initErrors.length > 0
                    ? 'error'
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
                ? 'Ctrl+y: Copy URL'
                : undefined
          }
        />

        <ActivityTray />

        {trustAllToolsAccepted && <TrustAllToolsBanner />}
        <Box marginBottom={1}>
          <PromptBar
            header={
              showContextBreakdown ||
              showHelpPanel ||
              showTuiPanel ||
              showUsagePanel ||
              showMcpPanel ||
              showToolsPanel ||
              showHooksPanel ||
              showKnowledgePanel ||
              showCodePanel ||
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
              editingQueueIndex,
              pendingApproval: !!pendingApproval,
              isShellEscape,
              isProcessing,
              queuedMessages,
              agentName: currentAgent?.name,
            })}
            hint={
              promptHint ||
              (activeCommand?.command.meta?.hint as string | undefined)
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
                  showUsagePanel ||
                  showMcpPanel ||
                  showToolsPanel ||
                  showHooksPanel ||
                  showKnowledgePanel ||
                  showCodePanel
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
            {showContextBreakdown && (
              <ContextBreakdown
                percent={contextUsagePercent}
                breakdown={contextBreakdown ?? undefined}
                model={currentModel?.name ?? null}
                agentName={currentAgent?.name ?? null}
                initialExpanded={contextBreakdown?.initialExpanded}
                onClose={handleCloseContextBreakdown}
                onTabSwitch={handleTabFromContext}
              />
            )}
            {showUsagePanel && (
              <UsagePanel
                data={usageData}
                onClose={handleCloseUsagePanel}
                onTabSwitch={handleTabFromUsage}
              />
            )}
            {showHelpPanel && (
              <HelpPanel
                commands={helpCommands}
                onClose={handleCloseHelpPanel}
              />
            )}
            {showTuiPanel && <TuiPanel onClose={handleCloseTuiPanel} />}
            {showMcpPanel && (
              <McpPanel
                servers={mcpServersWithAuth}
                registryServers={mcpRegistryServers}
                initErrors={initErrors}
                pendingOAuthUrls={pendingOAuthServers}
                mode={mcpMode}
                onClose={handleCloseMcpPanel}
                onAction={async (serverNames: string[]) => {
                  const action = mcpMode === 'add' ? 'add' : 'remove';
                  await kiro.executeCommand({
                    command: 'mcp',
                    args: { value: `${action} ${serverNames.join(',')}` },
                  } as any);
                  const result = await kiro.executeCommand({
                    command: 'mcp',
                    args: { value: action },
                  } as any);
                  if (result?.data) {
                    const data = result.data as {
                      servers?: McpServerInfo[];
                      mode?: string;
                    };
                    setShowMcpPanel(
                      true,
                      data.servers ?? [],
                      data.mode ?? action
                    );
                  }
                }}
              />
            )}
            {showToolsPanel && (
              <ToolsPanel tools={toolsList} onClose={handleCloseToolsPanel} />
            )}
            {showHooksPanel && (
              <HooksPanel hooks={hooksList} onClose={handleCloseHooksPanel} />
            )}
            {showKnowledgePanel && (
              <KnowledgePanel
                entries={knowledgeEntries}
                status={knowledgeStatus}
                onClose={handleCloseKnowledgePanel}
              />
            )}
            {showCodePanel && (
              <CodePanel
                data={codeData}
                onClose={handleCloseCodePanel}
                onRefresh={handleRefreshCodePanel}
              />
            )}
            <ActionHint
              text="Showing detailed output · ctrl+o to toggle"
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
                !showContextBreakdown &&
                !showHelpPanel &&
                !showTuiPanel &&
                !showUsagePanel &&
                !showMcpPanel &&
                !showToolsPanel &&
                !showHooksPanel &&
                !showKnowledgePanel &&
                !showCodePanel &&
                commandInputValue.length === 0 &&
                exitSequence === 0
              }
            />
            <ExitHint />
          </PromptBar>
        </Box>
      </Box>
    </AnimationPausedContext.Provider>
  );
};

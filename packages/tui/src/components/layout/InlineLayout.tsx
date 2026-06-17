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
import { HelpPanel } from '../ui/HelpPanel';
import { TuiPanel } from '../ui/TuiPanel';
import { ChangelogPanel } from '../ui/ChangelogPanel';
import { McpPanel } from '../ui/McpPanel';
import { ToolsPanel } from '../ui/ToolsPanel';
import { GoalPanel } from '../ui/GoalPanel';
import { StatsPanel } from '../ui/StatsPanel';
import { HooksPanel } from '../ui/HooksPanel';
import { KeybindingsPanel } from '../ui/KeybindingsPanel';
import { DisplaySettingsPanel } from '../ui/DisplaySettingsPanel';
import { ThemePanel } from '../ui/ThemePanel';
import { SettingsPanel } from '../ui/SettingsPanel';
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
import { Explorer } from '../ui/Explorer';
import { CodePanel } from '../ui/CodePanel';
import { SurveyPanel } from '../ui/SurveyPanel';
import { SurveyPromptBar } from '../ui/SurveyPromptBar';
import { ArtifactView } from '../ui/ArtifactView/index.js';
import { ArtifactGenerationCard } from '../ui/ArtifactView/ArtifactGenerationCard.js';

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
  type CodePanelData,
  type McpServerInfo,
} from '../../stores/app-store.js';
import { useSessionConversation } from '../../stores/session-conversations.js';
import { useShallow } from 'zustand/react/shallow';
import { useKeypress } from '../../hooks/useKeypress';
import {
  resolveKeybinding,
  formatKeybinding,
} from '../../utils/keybindings.js';
import { useKeybindings } from '../../hooks/useKeybindings.js';
import { InterruptMode } from '../../constants/interrupt-mode.js';
import type { AgentEngine } from '../../agent-engine.js';
import { startMcpOAuth } from '../../utils/mcp-oauth.js';
import { copyToSystemClipboard } from '../../commands/effects.js';
import { getGitBranch } from '../../utils/git';
import { shortenPath, formatEffort } from '../../utils/string';
import { getAgentColor, getAgentDisplayName } from '../../utils/agentColors.js';
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
  isInitialized: boolean;
  pendingSteerContent: string | null;
  activeInterruptMode: InterruptMode;
  agentEngine: AgentEngine;
  queuedMessages: string[];
  toggleHintLabel: string;
  agentName: string | undefined;
  goalStatus?: {
    state: string;
    iteration: number;
    maxIterations: number;
    message?: string;
  } | null;
  cancelLabel?: string;
}): string {
  // Editing a queued message takes precedence over all other states.
  if (opts.editingQueueIndex != null) {
    return `Editing queued message ${opts.editingQueueIndex + 1} · esc to cancel`;
  }
  // While the session is still initializing, the user can type freely —
  // input is buffered locally (as `pendingSteerContent`) and replayed once init
  // completes.
  if (!opts.isInitialized) {
    return opts.pendingSteerContent != null
      ? 'Initializing · type to queue another message'
      : 'Initializing · type to queue a message';
  }
  if (opts.goalStatus && opts.goalStatus.state === 'active') {
    const desc =
      opts.goalStatus.message && opts.goalStatus.message.length > 50
        ? opts.goalStatus.message.slice(0, 47) + '...'
        : (opts.goalStatus.message ?? 'Running');
    const cancel = opts.cancelLabel ?? 'Ctrl+C';
    return `Goal Active: ${desc} · Iteration ${opts.goalStatus.iteration + 1}/${opts.goalStatus.maxIterations} · ${cancel} to pause`;
  }
  if (opts.pendingApproval || opts.isProcessing) {
    // KAS ("v3") has no mid-turn steering, so omit the steer toggle hint.
    if (opts.agentEngine === 'kas') {
      return 'Kiro is working · Type to queue';
    }
    if (opts.activeInterruptMode === InterruptMode.STEER) {
      return `Kiro is working · Type to steer · ${opts.toggleHintLabel} to queue`;
    }
    return `Kiro is working · Type to queue · ${opts.toggleHintLabel} to steer`;
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
  const {
    toolOutputsExpanded,
    hasExpandableToolOutputs,
    showContextBreakdown,
    contextBreakdown,
    showTuiPanel,
    showChangelogPanel,
    showHelpPanel,
    helpCommands,
    showUsagePanel,
    usageData,
    showRewindExplorer,
    rewindRows,
    showMcpPanel,
    mcpServers,
    mcpRegistryServers,
    mcpMode,
    showToolsPanel,
    showGoalPanel,
    toolsList,
    showStatsPanel,
    statsList,
    statsSummary,
    showHooksPanel,
    hooksList,
    showKeybindingsPanel,
    showDisplaySettingsPanel,
    showThemePanel,
    showSettingsPanel,
    settingsReturnOnEscape,
    showKnowledgePanel,
    knowledgeEntries,
    knowledgeStatus,
    showCodePanel,
    codeData,
    artifactViewOpen,
  } = useUIState();
  const {
    toggleToolOutputsExpanded,
    setShowContextBreakdown,
    setShowHelpPanel,
    setShowTuiPanel,
    setShowChangelogPanel,
    setShowUsagePanel,
    setShowRewindExplorer,
    setShowMcpPanel,
    setShowToolsPanel,
    setShowGoalPanel,
    setShowStatsPanel,
    setShowHooksPanel,
    setShowKeybindingsPanel,
    setShowDisplaySettingsPanel,
    setShowThemePanel,
    setShowSettingsPanel,
    setSettingsReturnOnEscape,
    reopenSettingsMenu,
    setShowKnowledgePanel,
    setShowCodePanel,
  } = useUIActions();
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
  const agentEngine = useAppStore((state) => state.agentEngine);
  const commandInputValue = useAppStore((state) => state.commandInputValue);
  const agentEngine = useAppStore((state) => state.agentEngine);
  const { setActiveCommand, setActiveTrigger, clearCommandInput } =
    useCommandActions();
  const { handleUserInput, clearInput } = useInputActions();
  const { messages } = useConversationState();
  const {
    pendingSteerContent,
    activeInterruptMode,
    queuedMessages,
    editingQueueIndex,
  } = useQueueState();
  const replaceQueuedMessage = useAppStore((s) => s.replaceQueuedMessage);
  const cancelEditingQueue = useAppStore((s) => s.cancelEditingQueue);
  const isInitialized = useAppStore((s) => s.isInitialized);
  const agentEngine = useAppStore((s) => s.agentEngine);
  const settings = useAppStore((s) => s.settings);
  const { kiro } = useKiroClient();
  const mode = useAppStore((state) => state.mode);

  const toggleHintLabel = useMemo(() => {
    const binding = resolveKeybinding(settings, 'toggleInterruptMode');
    return formatKeybinding(binding);
  }, [settings]);
  const setMode = useAppStore((state) => state.setMode);
  const exitSequence = useAppStore((state) => state.exitSequence);
  const suspendArmed = useAppStore((state) => state.suspendArmed);

  // Research-survey state — kept as a simple trio of selectors since it's
  // only consumed here.
  const showSurveyPanel = useAppStore((s) => s.showSurveyPanel);
  const closeSurveyPanel = useAppStore((s) => s.closeSurveyPanel);
  const submitSurvey = useAppStore((s) => s.submitSurvey);
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

  const handleCloseChangelogPanel = useCallback(() => {
    setShowChangelogPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowChangelogPanel, setActiveCommand, clearCommandInput]);

  const handleCloseUsagePanel = useCallback(() => {
    setShowUsagePanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowUsagePanel, setActiveCommand, clearCommandInput]);

  const handleCloseRewindExplorer = useCallback(() => {
    setShowRewindExplorer(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowRewindExplorer, setActiveCommand, clearCommandInput]);

  const handleRewindSelect = useCallback(
    (rowId: string) => {
      setShowRewindExplorer(false);
      setActiveCommand(null);
      clearCommandInput();
      // Fire `/rewind <idx>` through the normal command pipeline so the
      // rewindAction effect handles the clone + session load.
      void handleUserInput(`/rewind ${rowId}`);
    },
    [
      setShowRewindExplorer,
      setActiveCommand,
      clearCommandInput,
      handleUserInput,
    ]
  );

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
      // KAS: prefer the typed cached breakdown — no round-trip required.
      const cached = kiro.getCachedContextBreakdown();
      if (cached) {
        setShowContextBreakdown(true, cached);
        setShowUsagePanel(false);
        return;
      }
      // V2 Rust: fall back to the engine-specific executeCommand path,
      // which returns the breakdown inline.
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

  const handleCloseStatsPanel = useCallback(() => {
    setShowStatsPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowStatsPanel, setActiveCommand, clearCommandInput]);

  const handleCloseHooksPanel = useCallback(() => {
    setShowHooksPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowHooksPanel, setActiveCommand, clearCommandInput]);

  const handleCloseKeybindingsPanel = useCallback(() => {
    setShowKeybindingsPanel(false);
    setActiveCommand(null);
    clearCommandInput();
    // Return to /settings menu if this panel was opened from there.
    if (settingsReturnOnEscape) {
      setSettingsReturnOnEscape(false);
      reopenSettingsMenu();
    }
  }, [
    setShowKeybindingsPanel,
    setActiveCommand,
    clearCommandInput,
    settingsReturnOnEscape,
    setSettingsReturnOnEscape,
    reopenSettingsMenu,
  ]);

  const handleCloseDisplaySettingsPanel = useCallback(() => {
    setShowDisplaySettingsPanel(false);
    setActiveCommand(null);
    clearCommandInput();
    if (settingsReturnOnEscape) {
      setSettingsReturnOnEscape(false);
      reopenSettingsMenu();
    }
  }, [
    setShowDisplaySettingsPanel,
    setActiveCommand,
    clearCommandInput,
    settingsReturnOnEscape,
    setSettingsReturnOnEscape,
    reopenSettingsMenu,
  ]);

  const handleCloseThemePanel = useCallback(() => {
    setShowThemePanel(false);
    setActiveCommand(null);
    clearCommandInput();
    if (settingsReturnOnEscape) {
      setSettingsReturnOnEscape(false);
      reopenSettingsMenu();
    }
  }, [
    setShowThemePanel,
    setActiveCommand,
    clearCommandInput,
    settingsReturnOnEscape,
    setSettingsReturnOnEscape,
    reopenSettingsMenu,
  ]);

  const handleCloseSettingsPanel = useCallback(() => {
    // Top-level /settings close. Always clears the back-flag so the next
    // overlay open starts fresh — avoids a stale `settingsReturnOnEscape`
    // bouncing the user into /settings unexpectedly.
    setShowSettingsPanel(false);
    setActiveCommand(null);
    clearCommandInput();
    setSettingsReturnOnEscape(false);
  }, [
    setShowSettingsPanel,
    setActiveCommand,
    clearCommandInput,
    setSettingsReturnOnEscape,
  ]);

  const handleDismissDisplaySettingsPanel = useCallback(() => {
    setShowDisplaySettingsPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowDisplaySettingsPanel, setActiveCommand, clearCommandInput]);

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
              ? '⏸'
              : goalStatus.state === 'completed'
                ? '✓'
                : goalStatus.state === 'exhausted'
                  ? '✗'
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
              value={`${icon} Goal ${label}${elapsed ? ` · ${elapsed}` : ''}`}
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
      respondToApproval,
      handleUserInput,
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
              editingQueueIndex,
              pendingApproval: !!pendingApproval,
              isShellEscape,
              isProcessing,
              isInitialized,
              pendingSteerContent,
              activeInterruptMode,
              agentEngine,
              queuedMessages,
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
            {showRewindExplorer && (
              <Explorer
                title="/rewind"
                description="Fork from a previous prompt in this session"
                columns={[
                  { key: 'label', label: 'User Prompt' },
                  { key: 'group', label: 'Context used', align: 'right' },
                ]}
                rows={rewindRows.map((turn) => ({
                  id: String(turn.logIndex),
                  values: {
                    label: turn.label,
                    group: turn.group ?? '',
                  },
                  preview: turn.responseSnippet
                    ? { body: turn.responseSnippet }
                    : undefined,
                }))}
                previewHeading="● Turn Activity"
                keyHints={[
                  { key: '↑↓', label: 'navigate' },
                  { key: 'Enter', label: 'to fork' },
                ]}
                onSelect={(row) => handleRewindSelect(row.id)}
                onClose={handleCloseRewindExplorer}
              />
            )}
            {showHelpPanel && (
              <HelpPanel
                commands={helpCommands}
                onClose={handleCloseHelpPanel}
              />
            )}
            {showTuiPanel && <TuiPanel onClose={handleCloseTuiPanel} />}
            {showChangelogPanel && (
              <ChangelogPanel onClose={handleCloseChangelogPanel} />
            )}
            {showMcpPanel && (
              <McpPanel
                servers={mcpServersWithAuth}
                registryServers={mcpRegistryServers}
                initErrors={initErrors}
                pendingOAuthUrls={pendingOAuthServers}
                mode={mcpMode}
                onClose={handleCloseMcpPanel}
                onAuthenticate={(serverName) => {
                  // Mirror the Ctrl+Y path so the panel shows the same
                  // notification: KAS resets the server to (re)start OAuth;
                  // V2 copies the (already valid) URL to the clipboard.
                  startMcpOAuth({
                    agentEngine,
                    serverName,
                    url: pendingOAuthServers.get(serverName) ?? null,
                    resetMcpServer: (name, startOAuth) =>
                      kiro.resetMcpServer(name, startOAuth),
                    copyToClipboard: copyToSystemClipboard,
                    showAlert: (message, status, autoHideMs) =>
                      showTransientAlert({ message, status, autoHideMs }),
                  });
                }}
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
              <ToolsPanel
                tools={toolsList}
                initErrors={initErrors}
                onClose={handleCloseToolsPanel}
              />
            )}
            {showGoalPanel && (
              <GoalPanel onClose={() => setShowGoalPanel(false)} />
            )}
            {showStatsPanel && (
              <StatsPanel
                stats={statsList}
                summary={statsSummary}
                onClose={handleCloseStatsPanel}
              />
            )}
            {showHooksPanel && (
              <HooksPanel hooks={hooksList} onClose={handleCloseHooksPanel} />
            )}
            {showKeybindingsPanel && (
              <KeybindingsPanel onClose={handleCloseKeybindingsPanel} />
            )}
            {showDisplaySettingsPanel && (
              <DisplaySettingsPanel
                onClose={handleCloseDisplaySettingsPanel}
                onDismiss={handleDismissDisplaySettingsPanel}
              />
            )}
            {showThemePanel && <ThemePanel onClose={handleCloseThemePanel} />}
            {showSettingsPanel && (
              <SettingsPanel onClose={handleCloseSettingsPanel} />
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
            {artifactViewOpen && <ArtifactView />}
            {showSurveyPanel && (
              <SurveyPanel onClose={closeSurveyPanel} onSubmit={submitSurvey} />
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

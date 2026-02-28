import React, {
  useCallback,
  useMemo,
  useState,
  useEffect,
  useRef,
} from 'react';
import { Box, Text } from 'ink';
import { AnimationPausedContext } from '../../contexts/AnimationPausedContext.js';
import { ConversationView } from '../ui/ConversationView';
import { ExitHint } from '../ui/ExitHint';
import { CommandMenu } from '../ui/CommandMenu';
import { ActionHint } from '../ui/hint/ActionHint.js';
import { HelpPanel } from '../ui/HelpPanel';
import { McpPanel } from '../ui/McpPanel';
import { ToolsPanel } from '../ui/ToolsPanel';
import { PromptsPanel } from '../ui/PromptsPanel';
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
import { RadioGroup, type RadioOption } from '../ui/radio/RadioGroup.js';
import { UsagePanel } from '../ui/UsagePanel';
import {
  useNotificationState,
  useNotificationActions,
  useProcessingState,
  useUIState,
  useUIActions,
  useContextState,
  useCommandState,
  useCommandActions,
  useInputActions,
  useConversationState,
  useApprovalState,
  useQueueState,
  useKiroClient,
} from '../../stores/selectors.js';
import { useAppStore } from '../../stores/app-store.js';
import { useShallow } from 'zustand/react/shallow';
import { useKeypress } from '../../hooks/useKeypress';
import { getGitBranch } from '../../utils/git';
import { shortenPath } from '../../utils/string';
import { getAgentColor } from '../../utils/agentColors.js';
import { ApprovalOptionId } from '../../types/agent-events.js';

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

export const InlineLayout: React.FC = () => {
  // Grouped selectors using useShallow - prevents re-render cascades
  const { transientAlert, loadingMessage, agentError, agentErrorGuidance } =
    useNotificationState();
  const { dismissTransientAlert, setAgentError, setLoadingMessage } =
    useNotificationActions();
  const { isProcessing, isCompacting, pendingApproval, noInteractive } =
    useProcessingState();
  const { respondToApproval, cancelApproval } = useApprovalState();
  const {
    toolOutputsExpanded,
    hasExpandableToolOutputs,
    showContextBreakdown,
    contextBreakdown,
    showHelpPanel,
    helpCommands,
    showPromptsPanel,
    prompts,
    showUsagePanel,
    usageData,
    showMcpPanel,
    mcpServers,
    showToolsPanel,
    toolsList,
  } = useUIState();
  const {
    toggleToolOutputsExpanded,
    setShowContextBreakdown,
    setShowHelpPanel,
    setShowPromptsPanel,
    setShowUsagePanel,
    setShowMcpPanel,
    setShowToolsPanel,
  } = useUIActions();
  const {
    sessionId,
    contextUsagePercent,
    currentModel,
    currentAgent,
    previousAgentName,
  } = useContextState();
  const { activeCommand, promptHint } = useCommandState();
  const { setActiveCommand, setActiveTrigger, clearCommandInput } =
    useCommandActions();
  const { handleUserInput, clearInput } = useInputActions();
  const { messages } = useConversationState();
  const { queuedMessages } = useQueueState();
  const { kiro } = useKiroClient();
  const { setCurrentAgent, setPreviousAgentName } = useAppStore(
    useShallow((s) => ({
      setCurrentAgent: s.setCurrentAgent,
      setPreviousAgentName: s.setPreviousAgentName,
    }))
  );

  const [gitBranch, setGitBranch] = useState(() => getGitBranch());

  useEffect(() => {
    if (!isProcessing) setGitBranch(getGitBranch());
  }, [isProcessing]);

  // Build radio options from pending approval permissions
  const approvalOptions = useMemo((): RadioOption[] => {
    if (!pendingApproval) return [];
    const opts: RadioOption[] = [];
    const perms = pendingApproval.permissionOptions;
    if (perms.find((opt) => opt.kind === ApprovalOptionId.RejectOnce)) {
      opts.push({ value: ApprovalOptionId.RejectOnce, label: 'No' });
    }
    if (perms.find((opt) => opt.kind === ApprovalOptionId.AllowOnce)) {
      opts.push({
        value: ApprovalOptionId.AllowOnce,
        label: 'Yes, single permission',
      });
    }
    if (perms.find((opt) => opt.kind === ApprovalOptionId.AllowAlways)) {
      opts.push({
        value: ApprovalOptionId.AllowAlways,
        label: 'Trust, always allow in this session',
      });
    }
    return opts;
  }, [pendingApproval]);

  // Default to "No" when approval appears
  const [approvalSelected, setApprovalSelected] = useState<string>(
    ApprovalOptionId.RejectOnce
  );

  // Reset selection when a new approval arrives
  const approvalToolCallId = pendingApproval?.toolCall.toolCallId;
  const [lastApprovalId, setLastApprovalId] = useState<string | undefined>();
  if (approvalToolCallId && approvalToolCallId !== lastApprovalId) {
    setLastApprovalId(approvalToolCallId);
    setApprovalSelected(ApprovalOptionId.RejectOnce);
  }

  // Handle approval radio confirm (Enter)
  const handleApprovalConfirm = useCallback(
    (value: string) => {
      if (!pendingApproval) return;
      const selected = pendingApproval.permissionOptions.find(
        (opt) => opt.kind === value
      );
      if (selected) {
        respondToApproval(selected.optionId);
      }
    },
    [pendingApproval, respondToApproval]
  );

  // Handle escape to cancel approval, Enter to confirm
  useKeypress(
    (_input, key) => {
      if (!pendingApproval) return;
      if (key.escape) {
        cancelApproval();
        clearInput();
        clearCommandInput();
        return;
      }
      // Confirm via Enter — always confirms radio selection during approval
      if (key.return) {
        handleApprovalConfirm(approvalSelected);
      }
    },
    { isActive: !!pendingApproval }
  );

  // Handle Ctrl+O to toggle expansion (single state for both tool outputs and queue)
  useKeypress(
    (input, key) => {
      if (key.ctrl && input.toLowerCase() === 'o') {
        toggleToolOutputsExpanded();
      }
    },
    { isActive: hasExpandableToolOutputs || queuedMessages.length > 0 }
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

  const handleClosePromptsPanel = useCallback(() => {
    setShowPromptsPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowPromptsPanel, setActiveCommand, clearCommandInput]);

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

  const handleCloseToolsPanel = useCallback(() => {
    setShowToolsPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowToolsPanel, setActiveCommand, clearCommandInput]);

  // Build the header - use SnackBar for approval, ContextBar otherwise
  const promptBarHeader = useMemo(() => {
    if (pendingApproval) {
      const toolMessage = messages.find(
        (msg) =>
          msg.role === 'tool_use' &&
          msg.id === pendingApproval.toolCall.toolCallId
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
            if (value.length > 60) value = value.slice(0, 57) + '...';
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

    return (
      <ContextBar>
        {currentAgent && (
          <>
            {currentAgent.name === 'kiro_planner' && (
              <Text color="magenta">[plan] </Text>
            )}
            <Chip
              value={currentAgent.name}
              hexColor={getAgentColor(currentAgent.name).hex}
              prefix="agent:"
            />
          </>
        )}
        {contextUsagePercent != null && (
          <ProgressChip
            value={contextUsagePercent}
            barColor="success"
            label="context remaining"
            showRemaining={true}
          />
        )}
        <Chip value={shortenPath(process.cwd())} color={ChipColor.BRAND} />
        {gitBranch && (
          <Chip
            value={gitBranch}
            color={ChipColor.PRIMARY}
            prefix="git:"
            wrap={true}
          />
        )}
        {currentModel && (
          <Chip value={currentModel.name} color={ChipColor.PRIMARY} />
        )}
      </ContextBar>
    ) as PromptBarHeader;
  }, [
    pendingApproval,
    messages,
    currentAgent,
    contextUsagePercent,
    gitBranch,
    currentModel,
  ]);

  const handleSubmit = useCallback(
    (value: string) => {
      if (value.trim().toLowerCase() === '/kiro') {
        triggerEasterEgg();
        return;
      }
      handleUserInput(value);
      setGitBranch(getGitBranch());
    },
    [handleUserInput, setGitBranch]
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
    <AnimationPausedContext.Provider value={!!pendingApproval}>
      <Box flexDirection="column">
        {agentError && (
          <BlockingErrorAlert
            message={agentError}
            guidance={agentErrorGuidance ?? undefined}
            onDismiss={handleDismissError}
          />
        )}

        <ConversationView />

        <NotificationBar
          message={
            !sessionId
              ? 'Initializing...'
              : (loadingMessage ?? transientAlert?.message)
          }
          status={
            !sessionId || loadingMessage ? 'loading' : transientAlert?.status
          }
          autoHideMs={
            !sessionId || loadingMessage
              ? undefined
              : transientAlert?.autoHideMs
          }
          onDismiss={
            !sessionId || loadingMessage ? undefined : dismissTransientAlert
          }
        />

        <Box marginBottom={1}>
          <PromptBar
            header={
              showContextBreakdown ||
              showHelpPanel ||
              showUsagePanel ||
              showMcpPanel ||
              showToolsPanel ||
              showPromptsPanel
                ? undefined
                : promptBarHeader
            }
            subHeader={
              pendingApproval && approvalOptions.length > 0 ? (
                <RadioGroup
                  options={approvalOptions}
                  selectedValue={approvalSelected}
                  onChange={(value) => setApprovalSelected(value)}
                  direction="vertical"
                />
              ) : undefined
            }
            onSubmit={handleSubmit}
            triggerRules={TRIGGER_RULES}
            onTriggerDetected={handleTriggerDetected}
            isProcessing={
              isProcessing || isCompacting || !!activeCommand || !!agentError
            }
            placeholder={
              pendingApproval
                ? 'queue up your next message'
                : currentAgent?.name === 'kiro_planner'
                  ? 'ask a question, or describe a task ↵  ·  exit plan mode: shift+tab'
                  : undefined
            }
            hint={
              promptHint ||
              (activeCommand?.command.meta?.hint as string | undefined)
            }
            hideInput={
              toolOutputsExpanded ||
              noInteractive ||
              !!pendingApproval ||
              showContextBreakdown ||
              showHelpPanel ||
              showUsagePanel ||
              showMcpPanel ||
              showToolsPanel ||
              showPromptsPanel
            }
          >
            <CommandMenu />
            {showContextBreakdown && (
              <ContextBreakdown
                percent={contextUsagePercent}
                breakdown={contextBreakdown ?? undefined}
                model={currentModel?.name ?? null}
                agentName={currentAgent?.name ?? null}
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
            {showMcpPanel && (
              <McpPanel servers={mcpServers} onClose={handleCloseMcpPanel} />
            )}
            {showToolsPanel && (
              <ToolsPanel tools={toolsList} onClose={handleCloseToolsPanel} />
            )}
            {showPromptsPanel && (
              <PromptsPanel
                prompts={prompts}
                onClose={handleClosePromptsPanel}
              />
            )}
            <ActionHint
              text="Showing detailed output · ctrl+o to toggle"
              visible={toolOutputsExpanded}
            />
            <ExitHint />
          </PromptBar>
        </Box>
      </Box>
    </AnimationPausedContext.Provider>
  );
};

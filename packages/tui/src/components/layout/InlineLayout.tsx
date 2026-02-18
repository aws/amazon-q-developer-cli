import React, { useCallback, useMemo, useState } from 'react';
import { Box } from 'ink';
import { AnimationPausedContext } from '../../contexts/AnimationPausedContext.js';
import { ConversationView } from '../ui/ConversationView';
import { ExitHint } from '../ui/ExitHint';
import { CommandMenu } from '../ui/CommandMenu';
import { ActionHint } from '../ui/hint/ActionHint.js';
import { HelpPanel } from '../ui/HelpPanel';
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
} from '../../stores/selectors.js';
import { useKeypress } from '../../hooks/useKeypress';
import { getGitBranch } from '../../utils/git';
import { shortenPath } from '../../utils/string';
import { getAgentColor } from '../../utils/agentColors.js';
import { ApprovalOptionId } from '../../types/agent-events.js';

const TRIGGER_RULES = [
  { key: '/', type: 'start' as const },
  { key: '@', type: 'inline' as const },
];

export const InlineLayout: React.FC = () => {
  // Grouped selectors using useShallow - prevents re-render cascades
  const { transientAlert, loadingMessage, agentError, agentErrorGuidance } =
    useNotificationState();
  const { dismissTransientAlert, setAgentError } = useNotificationActions();
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
  } = useUIState();
  const {
    toggleToolOutputsExpanded,
    setShowContextBreakdown,
    setShowHelpPanel,
  } = useUIActions();
  const { sessionId, contextUsagePercent, currentModel, currentAgent } =
    useContextState();
  const { activeCommand } = useCommandState();
  const { setActiveCommand, setActiveTrigger, clearCommandInput } =
    useCommandActions();
  const { handleUserInput, clearInput } = useInputActions();
  const { messages } = useConversationState();
  const { queuedMessages } = useQueueState();

  // Cache git branch - only call once on mount to avoid blocking renders
  const gitBranch = useMemo(() => getGitBranch(), []);

  // Build radio options from pending approval permissions
  const approvalOptions = useMemo((): RadioOption[] => {
    if (!pendingApproval) return [];
    const opts: RadioOption[] = [];
    const perms = pendingApproval.permissionOptions;
    if (perms.find((opt) => opt.kind === ApprovalOptionId.RejectOnce)) {
      opts.push({ value: ApprovalOptionId.RejectOnce, label: '(N)o' });
    }
    if (perms.find((opt) => opt.kind === ApprovalOptionId.AllowOnce)) {
      opts.push({
        value: ApprovalOptionId.AllowOnce,
        label: '(Y)es, single permission',
      });
    }
    if (perms.find((opt) => opt.kind === ApprovalOptionId.AllowAlways)) {
      opts.push({
        value: ApprovalOptionId.AllowAlways,
        label: '(T)rust, always allow in this session',
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
          const value = parsed.path || parsed.command || parsed.query || '';
          if (value) detail = ` · ${value}`;
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
          <Chip
            value={currentAgent.name}
            hexColor={getAgentColor(currentAgent.name).hex}
            prefix="agent:"
          />
        )}
        <ProgressChip
          value={contextUsagePercent ?? 0}
          barColor="success"
          label="context remaining"
          showRemaining={true}
        />
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
      handleUserInput(value);
    },
    [handleUserInput]
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
              showContextBreakdown || showHelpPanel
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
              pendingApproval ? 'queue up your next message' : undefined
            }
            hint={activeCommand?.command.meta?.hint as string | undefined}
            hideInput={
              toolOutputsExpanded || noInteractive || !!pendingApproval
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
              />
            )}
            {showHelpPanel && (
              <HelpPanel
                commands={helpCommands}
                onClose={handleCloseHelpPanel}
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

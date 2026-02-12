import React, { useCallback, useMemo } from 'react';
import { Box } from 'ink';
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
  const { transientAlert, loadingMessage, agentError, agentErrorGuidance } = useNotificationState();
  const { dismissTransientAlert, setAgentError } = useNotificationActions();
  const { isProcessing, isCompacting, pendingApproval } = useProcessingState();
  const { respondToApproval, cancelApproval } = useApprovalState();
  const { toolOutputsExpanded, hasExpandableToolOutputs, showContextBreakdown, showHelpPanel, helpCommands } = useUIState();
  const { toggleToolOutputsExpanded, setShowContextBreakdown, setShowHelpPanel } = useUIActions();
  const { sessionId, contextUsagePercent, lastTurnTokens, currentModel, currentAgent } = useContextState();
  const { activeCommand } = useCommandState();
  const { setActiveCommand, setActiveTrigger, clearCommandInput } = useCommandActions();
  const { handleUserInput } = useInputActions();
  const { messages } = useConversationState();

  // Cache git branch - only call once on mount to avoid blocking renders
  const gitBranch = useMemo(() => getGitBranch(), []);

  // Handle approval keypresses
  useKeypress(
    (input, key) => {
      if (!pendingApproval) return;

      // Handle escape to cancel
      if (key.escape) {
        cancelApproval();
        return;
      }

      const inputLower = input.toLowerCase();
      let selectedOption;

      // Map keys to option kinds
      if (inputLower === 't') {
        selectedOption = pendingApproval.permissionOptions.find(
          (opt) => opt.kind === ApprovalOptionId.AllowAlways
        );
      } else if (inputLower === 'y') {
        selectedOption = pendingApproval.permissionOptions.find(
          (opt) => opt.kind === ApprovalOptionId.AllowOnce
        );
      } else if (inputLower === 'n') {
        selectedOption = pendingApproval.permissionOptions.find(
          (opt) => opt.kind === ApprovalOptionId.RejectOnce
        );
      }

      if (selectedOption) {
        respondToApproval(selectedOption.optionId);
      }
    },
    { isActive: !!pendingApproval }
  );

  // Handle Ctrl+O to toggle expansion mode (works both ways)
  useKeypress(
    (input, key) => {
      if (key.ctrl && input.toLowerCase() === 'o' && hasExpandableToolOutputs) {
        toggleToolOutputsExpanded();
      }
    },
    { isActive: hasExpandableToolOutputs }
  );

  // Handle Esc to collapse expanded outputs (secondary shortcut)
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
      // Find the tool message to get the tool name
      const toolMessage = messages.find(
        (msg) => msg.role === 'tool_use' && msg.id === pendingApproval.toolCall.toolCallId
      );
      const toolName = toolMessage && 'name' in toolMessage ? toolMessage.name : 'Tool';

      // Build actions array
      const actions = [];
      if (pendingApproval.permissionOptions.find((opt) => opt.kind === ApprovalOptionId.AllowOnce)) {
        actions.push({ key: 'y', label: 'Yes' });
      }
      if (pendingApproval.permissionOptions.find((opt) => opt.kind === ApprovalOptionId.RejectOnce)) {
        actions.push({ key: 'n', label: 'No' });
      }
      if (pendingApproval.permissionOptions.find((opt) => opt.kind === ApprovalOptionId.AllowAlways)) {
        actions.push({ key: 't', label: 'Trust' });
      }

      return (
        <SnackBar
          title={`${toolName} requires approval`}
          actions={actions}
          slideIn={true}
        />
      ) as PromptBarHeader;
    }

    return (
      <ContextBar>
        {currentAgent && (
          <Chip value={currentAgent.name} hexColor={getAgentColor(currentAgent.name).hex} prefix="agent:" />
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
  }, [pendingApproval, messages, currentAgent, contextUsagePercent, gitBranch, currentModel]);

  const handleSubmit = useCallback((value: string) => {
    handleUserInput(value);
  }, [handleUserInput]);

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
        message={!sessionId ? 'Initializing...' : loadingMessage ?? transientAlert?.message}
        status={!sessionId || loadingMessage ? 'loading' : transientAlert?.status}
        autoHideMs={!sessionId || loadingMessage ? undefined : transientAlert?.autoHideMs}
        onDismiss={!sessionId || loadingMessage ? undefined : dismissTransientAlert}
      />

      <Box marginBottom={1}>
        <PromptBar
          header={promptBarHeader}
          onSubmit={handleSubmit}
          triggerRules={TRIGGER_RULES}
          onTriggerDetected={handleTriggerDetected}
          isProcessing={
            isProcessing ||
            isCompacting ||
            !!pendingApproval ||
            !!activeCommand ||
            !!agentError
          }
          placeholder={pendingApproval ? 'type permission (y/n/t) ↵' : undefined}
          hint={activeCommand?.command.meta?.hint as string | undefined}
          hideInput={toolOutputsExpanded}
        >
          <CommandMenu />
          {showContextBreakdown && (
            <ContextBreakdown
              percent={contextUsagePercent}
              tokens={lastTurnTokens}
              model={currentModel?.name ?? null}
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
  );
};

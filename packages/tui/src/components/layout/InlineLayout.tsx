import React, { useCallback } from 'react';
import { Box, Text } from 'ink';
import { ConversationView } from '../ui/ConversationView';
import { ApprovalRequest } from '../ui/ApprovalRequest';
import { ExitHint } from '../ui/ExitHint';
import { CommandMenu } from '../ui/CommandMenu';
import { ActionHint } from '../ui/hint/ActionHint.js';
import { PromptBar } from '../chat/prompt-bar/PromptBar.js';
import { ContextBar } from '../chat/prompt-bar/ContextBar.js';
import { NotificationBar } from '../chat/notification-bar/NotificationBar.js';
import { WelcomeScreen } from '../welcome-screen/index.js';
import { Chip, ChipColor, ProgressChip } from '../ui/chip/index.js';
import { ContextBreakdown } from '../ui/ContextBreakdown';
import { useAppStore } from '../../stores/app-store';
import { useKiro } from '../../hooks/useKiro';
import { useTheme } from '../../hooks/useTheme';
import { useKeypress } from '../../hooks/useKeypress';
import { getGitBranch } from '../../utils/git';
import { shortenPath } from '../../utils/string';

const TRIGGER_RULES = [
  { key: '/', type: 'start' as const },
  { key: '@', type: 'inline' as const },
];

export const InlineLayout: React.FC = () => {
  const handleUserInput = useAppStore((state) => state.handleUserInput);
  const pendingApproval = useAppStore((state) => state.pendingApproval);
  const isProcessing = useAppStore((state) => state.isProcessing);
  const activeCommand = useAppStore((state) => state.activeCommand);
  const setActiveCommand = useAppStore((state) => state.setActiveCommand);
  const slashCommands = useAppStore((state) => state.slashCommands);
  const currentModel = useAppStore((state) => state.currentModel);
  const contextUsagePercent = useAppStore((state) => state.contextUsagePercent);
  const lastTurnTokens = useAppStore((state) => state.lastTurnTokens);
  const showContextBreakdown = useAppStore((state) => state.showContextBreakdown);
  const setShowContextBreakdown = useAppStore((state) => state.setShowContextBreakdown);
  const transientAlert = useAppStore((state) => state.transientAlert);
  const dismissTransientAlert = useAppStore((state) => state.dismissTransientAlert);
  const commandInputValue = useAppStore((state) => state.commandInputValue);
  const setCommandInput = useAppStore((state) => state.setCommandInput);
  const setActiveTrigger = useAppStore((state) => state.setActiveTrigger);
  const clearCommandInput = useAppStore((state) => state.clearCommandInput);
  const toolOutputsExpanded = useAppStore((state) => state.toolOutputsExpanded);
  const hasExpandableToolOutputs = useAppStore((state) => state.hasExpandableToolOutputs);
  const toggleToolOutputsExpanded = useAppStore((state) => state.toggleToolOutputsExpanded);
  const { error } = useKiro();
  const { colors } = useTheme();

  const gitBranch = getGitBranch();

  // Handle Ctrl+O to toggle expansion mode (single handler for all tool outputs)
  useKeypress((input, key) => {
    if (key.ctrl && input.toLowerCase() === 'o' && hasExpandableToolOutputs) {
      toggleToolOutputsExpanded();
    }
  }, { isActive: hasExpandableToolOutputs && !toolOutputsExpanded });

  // Handle Esc to collapse expanded outputs
  useKeypress((_input, key) => {
    if (key.escape && toolOutputsExpanded) {
      toggleToolOutputsExpanded();
    }
  }, { isActive: toolOutputsExpanded });

  const handleCloseContextBreakdown = useCallback(() => {
    setShowContextBreakdown(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowContextBreakdown, setActiveCommand, clearCommandInput]);

  const contextBarHeader = (
    <ContextBar>
      <ProgressChip value={contextUsagePercent ?? 0} barColor="success" label="context remaining" showRemaining={true} />
      <Chip value={shortenPath(process.cwd())} color={ChipColor.BRAND} />
      {gitBranch && <Chip value={gitBranch} color={ChipColor.PRIMARY} prefix="git:" wrap={true} />}
      {currentModel && <Chip value={currentModel.name} color={ChipColor.PRIMARY} />}
    </ContextBar>
  );

  const handleSubmit = useCallback((value: string) => {
    const cmdName = value.match(/^\/(\w+)/)?.[0];
    const cmd = slashCommands.find((c) => c.name === cmdName);
    const retainInput = (cmd?.meta?.inputType === 'selection' || cmd?.meta?.inputType === 'panel') && !value.includes(' ');
    
    if (retainInput) {
      setCommandInput(value);
    } else {
      clearCommandInput();
    }
    handleUserInput(value);
  }, [handleUserInput, slashCommands, setCommandInput, clearCommandInput]);

  const handleInputChange = useCallback((value: string) => {
    setCommandInput(value);
  }, [setCommandInput]);

  const handleTriggerDetected = useCallback((trigger: { key: string; position: number; type: 'start' | 'inline' } | null) => {
    setActiveTrigger(trigger);
  }, [setActiveTrigger]);

  return (
    <Box flexDirection="column">
      <Box marginBottom={1}>
        <WelcomeScreen agent="kiro" mcpServers={[]} animate={true} />
      </Box>

      {error && (
        <Box paddingX={1} marginBottom={1}>
          <Text color={colors.error}>⚠️ {error}</Text>
        </Box>
      )}

      <ConversationView />

      <Box marginTop={1} height={1}>
        {transientAlert && (
          <NotificationBar
            message={transientAlert.message}
            status={transientAlert.status}
            autoHideMs={transientAlert.autoHideMs}
            onDismiss={dismissTransientAlert}
          />
        )}
      </Box>

      <Box marginBottom={1}>
        <PromptBar
          header={contextBarHeader}
          onSubmit={handleSubmit}
          onInputChange={handleInputChange}
          triggerRules={TRIGGER_RULES}
          onTriggerDetected={handleTriggerDetected}
          isProcessing={isProcessing || !!pendingApproval || !!activeCommand}
          clearOnSubmit={false}
          value={commandInputValue}
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
          <ApprovalRequest />
          <ActionHint text="esc to collapse output" visible={toolOutputsExpanded} />
          <ActionHint text="Ctrl + o to expand output" visible={!toolOutputsExpanded && hasExpandableToolOutputs} />
          <ExitHint />
        </PromptBar>
      </Box>
    </Box>
  );
};

/**
 * Optimized selectors using useShallow to prevent unnecessary re-renders.
 *
 * These hooks group related state and return stable references when values haven't changed.
 */
import { useMemo } from 'react';
import { useShallow } from 'zustand/react/shallow';
import { useAppStore, type AppState } from './app-store.js';
import type { AvailableCommand } from '../types/commands.js';

/**
 * Returns the slash commands the autocomplete should show for the
 * current engine. In KAS mode the static TUI-side `kasCommands` list
 * is concatenated with `slashCommands`, which holds both the V2-host-
 * side `local` commands seeded at boot (`/exit`, `/settings`, etc.)
 * and KAS's own `available_commands_update` broadcast (built-ins plus
 * prompts/skills/steering). In V2 mode `slashCommands` already contains
 * locals plus V2's backend broadcast, so we return it directly.
 */
export const selectVisibleSlashCommands = (
  state: Pick<AppState, 'agentEngine' | 'kasCommands' | 'slashCommands'>
): readonly AvailableCommand[] =>
  state.agentEngine === 'kas'
    ? [...state.kasCommands, ...state.slashCommands]
    : state.slashCommands;

/**
 * Notification state selector - for NotificationBar and BlockingErrorAlert
 */
export const useNotificationState = () =>
  useAppStore(
    useShallow((state) => ({
      transientAlert: state.transientAlert,
      loadingMessage: state.loadingMessage,
      agentError: state.agentError,
      agentErrorGuidance: state.agentErrorGuidance,
      initErrors: state.initErrors,
      pendingOAuthServers: state.pendingOAuthServers,
    }))
  );

export const useNotificationActions = () =>
  useAppStore(
    useShallow((state) => ({
      showTransientAlert: state.showTransientAlert,
      dismissTransientAlert: state.dismissTransientAlert,
      setAgentError: state.setAgentError,
      setLoadingMessage: state.setLoadingMessage,
    }))
  );

/**
 * Command state selector - for CommandMenu
 */
export const useCommandState = () => {
  const state = useAppStore(
    useShallow((s) => ({
      _slashCommands: s.slashCommands,
      _kasCommands: s.kasCommands,
      _agentEngine: s.agentEngine,
      activeCommand: s.activeCommand,
      commandInputValue: s.commandInputValue,
      activeTrigger: s.activeTrigger,
      filePickerHasResults: s.filePickerHasResults,
      promptHint: s.promptHint,
      commandShadowText: s.commandShadowText,
    }))
  );
  const slashCommands = useMemo(
    () =>
      selectVisibleSlashCommands({
        agentEngine: state._agentEngine,
        kasCommands: state._kasCommands,
        slashCommands: state._slashCommands,
      }),
    [state._agentEngine, state._kasCommands, state._slashCommands]
  );
  return { ...state, slashCommands };
};

export const useCommandActions = () =>
  useAppStore(
    useShallow((state) => ({
      setSlashCommands: state.setSlashCommands,
      setActiveCommand: state.setActiveCommand,
      setCommandInput: state.setCommandInput,
      setActiveTrigger: state.setActiveTrigger,
      setFilePickerHasResults: state.setFilePickerHasResults,
      setPromptHint: state.setPromptHint,
      setCommandShadowText: state.setCommandShadowText,
      clearCommandInput: state.clearCommandInput,
      executeCommandWithArg: state.executeCommandWithArg,
    }))
  );

/**
 * Processing state selector - for PromptBar isProcessing prop
 */
export const useProcessingState = () =>
  useAppStore(
    useShallow((state) => ({
      isProcessing: state.isProcessing,
      isCompacting: state.isCompacting,
      isShellEscape: state.isShellEscape,
      pendingApproval: state.pendingApproval,
      cancelMessage: state.cancelMessage,
      noInteractive: state.noInteractive,
    }))
  );

/**
 * Approval state selector - for ApprovalRequest
 */
export const useApprovalState = () =>
  useAppStore(
    useShallow((state) => ({
      pendingApproval: state.pendingApproval,
      approvalMode: state.approvalMode,
      respondToApproval: state.respondToApproval,
      cancelApproval: state.cancelApproval,
      setApprovalMode: state.setApprovalMode,
      sessionId: state.sessionId,
      sessions: state.sessions,
    }))
  );

/**
 * Conversation state selector - for ConversationView
 */
export const useConversationState = () =>
  useAppStore(
    useShallow((state) => ({
      messages: state.messages,
      isProcessing: state.isProcessing,
      settings: state.settings,
    }))
  );

/**
 * UI state selector - for layout components
 */
export const useUIState = () =>
  useAppStore(
    useShallow((state) => ({
      mode: state.mode,
      exitSequence: state.exitSequence,
      suspendArmed: state.suspendArmed,
      toolOutputsExpanded: state.toolOutputsExpanded,
      hasExpandableToolOutputs: state.hasExpandableToolOutputs,
      showContextBreakdown: state.showContextBreakdown,
      contextBreakdown: state.contextBreakdown,
      showTuiPanel: state.showTuiPanel,
      showChangelogPanel: state.showChangelogPanel,
      showHelpPanel: state.showHelpPanel,
      helpCommands: state.helpCommands,
      showUsagePanel: state.showUsagePanel,
      usageData: state.usageData,
      showRewindExplorer: state.showRewindExplorer,
      rewindRows: state.rewindRows,
      showMcpPanel: state.showMcpPanel,
      mcpServers: state.mcpServers,
      mcpRegistryServers: state.mcpRegistryServers,
      mcpMode: state.mcpMode,
      showToolsPanel: state.showToolsPanel,
      toolsList: state.toolsList,
      showStatsPanel: state.showStatsPanel,
      statsList: state.statsList,
      statsSummary: state.statsSummary,
      showHooksPanel: state.showHooksPanel,
      showKeybindingsPanel: state.showKeybindingsPanel,
      showDisplaySettingsPanel: state.showDisplaySettingsPanel,
      settingsReturnOnEscape: state.settingsReturnOnEscape,
      hooksList: state.hooksList,
      showKnowledgePanel: state.showKnowledgePanel,
      knowledgeEntries: state.knowledgeEntries,
      knowledgeStatus: state.knowledgeStatus,
      showCodePanel: state.showCodePanel,
      codeData: state.codeData,
      // Spec artifact view
      artifactViewOpen: state.artifactViewOpen,
    }))
  );

export const useUIActions = () =>
  useAppStore(
    useShallow((state) => ({
      setMode: state.setMode,
      incrementExitSequence: state.incrementExitSequence,
      resetExitSequence: state.resetExitSequence,
      armSuspend: state.armSuspend,
      disarmSuspend: state.disarmSuspend,
      toggleToolOutputsExpanded: state.toggleToolOutputsExpanded,
      setHasExpandableToolOutputs: state.setHasExpandableToolOutputs,
      setShowContextBreakdown: state.setShowContextBreakdown,
      setShowHelpPanel: state.setShowHelpPanel,
      setShowTuiPanel: state.setShowTuiPanel,
      setShowChangelogPanel: state.setShowChangelogPanel,
      setShowUsagePanel: state.setShowUsagePanel,
      setShowRewindExplorer: state.setShowRewindExplorer,
      setShowMcpPanel: state.setShowMcpPanel,
      setShowToolsPanel: state.setShowToolsPanel,
      setShowStatsPanel: state.setShowStatsPanel,
      setShowHooksPanel: state.setShowHooksPanel,
      setShowKeybindingsPanel: state.setShowKeybindingsPanel,
      setShowDisplaySettingsPanel: state.setShowDisplaySettingsPanel,
      setSettingsReturnOnEscape: state.setSettingsReturnOnEscape,
      reopenSettingsMenu: state.reopenSettingsMenu,
      setShowKnowledgePanel: state.setShowKnowledgePanel,
      setShowCodePanel: state.setShowCodePanel,
      // Spec artifact view actions
      closeArtifactView: state.closeArtifactView,
      moveArtifactCursor: state.moveArtifactCursor,
      toggleArtifactExpand: state.toggleArtifactExpand,
      enterArtifactDetail: state.enterArtifactDetail,
      leaveArtifactDetail: state.leaveArtifactDetail,
    }))
  );

/**
 * Context usage selector - for ContextBar and ContextBreakdown
 */
export const useContextState = () =>
  useAppStore(
    useShallow((state) => ({
      sessionId: state.sessionId,
      contextUsagePercent: state.contextUsagePercent,
      lastTurnTokens: state.lastTurnTokens,
      currentModel: state.currentModel,
      currentEffort: state.currentEffort,
      currentAgent: state.currentAgent,
      previousAgentName: state.previousAgentName,
      codeIntelligenceActive: state.codeIntelligenceActive,
    }))
  );

/**
 * Kiro client selector - for command execution
 */
export const useKiroClient = () =>
  useAppStore(
    useShallow((state) => ({
      kiro: state.kiro,
    }))
  );

/**
 * Streaming buffer selector - for StreamingMessage
 */
export const useStreamingBuffer = () =>
  useAppStore(
    useShallow((state) => ({
      startBuffering: state.streamingBuffer?.startBuffering ?? null,
      stopBuffering: state.streamingBuffer?.stopBuffering ?? null,
    }))
  );

/**
 * Input actions selector - for PromptInput
 */
export const useInputActions = () =>
  useAppStore(
    useShallow((state) => ({
      handleUserInput: state.handleUserInput,
      clearInput: state.clearInput,
      insert: state.insert,
      newline: state.newline,
      backspace: state.backspace,
      moveCursor: state.moveCursor,
      setViewport: state.setViewport,
      navigateHistory: state.navigateHistory,
    }))
  );

/**
 * File attachment selector
 */
export const useFileAttachmentState = () =>
  useAppStore(
    useShallow((state) => ({
      attachedFiles: state.attachedFiles,
      pendingFileAttachment: state.pendingFileAttachment,
    }))
  );

export const useFileAttachmentActions = () =>
  useAppStore(
    useShallow((state) => ({
      attachFile: state.attachFile,
      removeAttachedFile: state.removeAttachedFile,
      clearAttachedFiles: state.clearAttachedFiles,
      setPendingFileAttachment: state.setPendingFileAttachment,
      consumePendingFileAttachment: state.consumePendingFileAttachment,
    }))
  );

/**
 * Image attachment selector
 */
export const useImageAttachmentState = () =>
  useAppStore(
    useShallow((state) => ({
      pendingImages: state.pendingImages,
    }))
  );

export const useImageAttachmentActions = () =>
  useAppStore(
    useShallow((state) => ({
      addPendingImage: state.addPendingImage,
      removePendingImage: state.removePendingImage,
      clearPendingImages: state.clearPendingImages,
    }))
  );

/**
 * Queue state selector - for ActivityTray and ConversationView queued messages
 */
export const useQueueState = () =>
  useAppStore(
    useShallow((state) => ({
      queuedMessages: state.queuedMessages,
      editingQueueIndex: state.editingQueueIndex,
    }))
  );

export const useQueueActions = () =>
  useAppStore(
    useShallow((state) => ({
      removeQueuedMessage: state.removeQueuedMessage,
      replaceQueuedMessage: state.replaceQueuedMessage,
      startEditingQueue: state.startEditingQueue,
      cancelEditingQueue: state.cancelEditingQueue,
    }))
  );

/**
 * Task state selector - for ActivityTray
 */
export const useTaskState = () =>
  useAppStore(
    useShallow((state) => ({
      tasks: state.tasks,
      activityTrayExpanded: state.activityTrayExpanded,
    }))
  );

export const useTaskActions = () =>
  useAppStore((state) => state.toggleActivityTray);

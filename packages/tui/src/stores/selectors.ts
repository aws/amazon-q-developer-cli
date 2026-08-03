/**
 * Optimized selectors using useShallow to prevent unnecessary re-renders.
 *
 * These hooks group related state and return stable references when values haven't changed.
 */
import { useMemo } from 'react';
import { useShallow } from 'zustand/react/shallow';
import { useAppStore } from './app-store.js';
import { selectVisibleSlashCommands } from './visible-slash-commands.js';

// Re-export the merge selector. The per-slice mappers
// (`promptToSlashCommand` etc.) are intentionally not re-exported -- they
// are an implementation detail of the merge.
export { selectVisibleSlashCommands };

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
      _cloudSessionActive: s.cloudSessionActive,
      _prompts: s.prompts,
      _skills: s.skills,
      _steering: s.steering,
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
        prompts: state._prompts,
        skills: state._skills,
        steering: state._steering,
        cloudSessionActive: state._cloudSessionActive,
      }),
    [
      state._agentEngine,
      state._kasCommands,
      state._slashCommands,
      state._prompts,
      state._skills,
      state._steering,
      state._cloudSessionActive,
    ]
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
      resumeSession: state.resumeSession,
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
      pendingQuestion: state.pendingQuestion,
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
      showMemoriesPanel: state.showMemoriesPanel,
      showHelpPanel: state.showHelpPanel,
      helpCommands: state.helpCommands,
      showUsagePanel: state.showUsagePanel,
      usageData: state.usageData,
      showRewindExplorer: state.showRewindExplorer,
      rewindRows: state.rewindRows,
      showTangentExplorer: state.showTangentExplorer,
      tangentRows: state.tangentRows,
      tangentName: state.tangentName,
      showMcpPanel: state.showMcpPanel,
      mcpServers: state.mcpServers,
      mcpRegistryServers: state.mcpRegistryServers,
      mcpMode: state.mcpMode,
      showToolsPanel: state.showToolsPanel,
      showGoalPanel: state.showGoalPanel,
      toolsList: state.toolsList,
      showStatsPanel: state.showStatsPanel,
      statsList: state.statsList,
      statsSummary: state.statsSummary,
      showHooksPanel: state.showHooksPanel,
      showRepoPicker: state.showRepoPicker,
      repoPickerResources: state.repoPickerResources,
      attachedRepos: state.attachedRepos,
      cloudProviderChecked: state.cloudProviderChecked,
      showSourceProviderGate: state.showSourceProviderGate,
      sourceProviderSetupUrl: state.sourceProviderSetupUrl,
      showSessionPicker: state.showSessionPicker,
      sessionPickerRows: state.sessionPickerRows,
      sessionPickerTitle: state.sessionPickerTitle,
      showKeybindingsPanel: state.showKeybindingsPanel,
      showDisplaySettingsPanel: state.showDisplaySettingsPanel,
      showThemePanel: state.showThemePanel,
      showStatusLinePanel: state.showStatusLinePanel,
      showCloudQuitPrompt: state.showCloudQuitPrompt,
      showSettingsPanel: state.showSettingsPanel,
      terminalTitleEnabled: state.terminalTitleEnabled,
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
      setShowMemoriesPanel: state.setShowMemoriesPanel,
      setShowUsagePanel: state.setShowUsagePanel,
      setShowRewindExplorer: state.setShowRewindExplorer,
      setShowTangentExplorer: state.setShowTangentExplorer,
      setTangentName: state.setTangentName,
      setShowMcpPanel: state.setShowMcpPanel,
      setShowToolsPanel: state.setShowToolsPanel,
      setShowGoalPanel: state.setShowGoalPanel,
      setShowStatsPanel: state.setShowStatsPanel,
      setShowHooksPanel: state.setShowHooksPanel,
      setShowRepoPicker: state.setShowRepoPicker,
      submitRepoPicker: state.submitRepoPicker,
      setShowSourceProviderGate: state.setShowSourceProviderGate,
      retrySourceProviderConnection: state.retrySourceProviderConnection,
      setShowSessionPicker: state.setShowSessionPicker,
      setShowKeybindingsPanel: state.setShowKeybindingsPanel,
      setShowDisplaySettingsPanel: state.setShowDisplaySettingsPanel,
      setShowThemePanel: state.setShowThemePanel,
      setShowStatusLinePanel: state.setShowStatusLinePanel,
      setShowCloudQuitPrompt: state.setShowCloudQuitPrompt,
      setShowSettingsPanel: state.setShowSettingsPanel,
      setTerminalTitleEnabled: state.setTerminalTitleEnabled,
      setSettingsReturnOnEscape: state.setSettingsReturnOnEscape,
      setVerboseReturnOnEscape: state.setVerboseReturnOnEscape,
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
      goalStatus: state.goalStatus,
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
      dispatchSlashCommand: state.dispatchSlashCommand,
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
 * Queue state selector — for ActivityTray display of the "what will run next"
 * message.
 */
export const useQueueState = () =>
  useAppStore(
    useShallow((state) => ({
      pendingSteerContent: state.pendingSteerContent,
      queuedMessages: state.queuedMessages,
      activeInterruptMode: state.activeInterruptMode,
      editingQueueIndex: state.editingQueueIndex,
    }))
  );

/**
 * Queue action selector — for queue management (remove, edit, reorder).
 */
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

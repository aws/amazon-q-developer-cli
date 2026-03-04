/**
 * Optimized selectors using useShallow to prevent unnecessary re-renders.
 *
 * These hooks group related state and return stable references when values haven't changed.
 */
import { useShallow } from 'zustand/react/shallow';
import { useAppStore } from './app-store.js';

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
export const useCommandState = () =>
  useAppStore(
    useShallow((state) => ({
      slashCommands: state.slashCommands,
      activeCommand: state.activeCommand,
      commandInputValue: state.commandInputValue,
      activeTrigger: state.activeTrigger,
      filePickerHasResults: state.filePickerHasResults,
      promptHint: state.promptHint,
    }))
  );

export const useCommandActions = () =>
  useAppStore(
    useShallow((state) => ({
      setSlashCommands: state.setSlashCommands,
      setActiveCommand: state.setActiveCommand,
      setCommandInput: state.setCommandInput,
      setActiveTrigger: state.setActiveTrigger,
      setFilePickerHasResults: state.setFilePickerHasResults,
      setPromptHint: state.setPromptHint,
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
      toolOutputsExpanded: state.toolOutputsExpanded,
      hasExpandableToolOutputs: state.hasExpandableToolOutputs,
      showContextBreakdown: state.showContextBreakdown,
      contextBreakdown: state.contextBreakdown,
      showHelpPanel: state.showHelpPanel,
      helpCommands: state.helpCommands,
      showPromptsPanel: state.showPromptsPanel,
      prompts: state.prompts,
      showUsagePanel: state.showUsagePanel,
      usageData: state.usageData,
      showMcpPanel: state.showMcpPanel,
      mcpServers: state.mcpServers,
      showToolsPanel: state.showToolsPanel,
      toolsList: state.toolsList,
    }))
  );

export const useUIActions = () =>
  useAppStore(
    useShallow((state) => ({
      setMode: state.setMode,
      incrementExitSequence: state.incrementExitSequence,
      resetExitSequence: state.resetExitSequence,
      toggleToolOutputsExpanded: state.toggleToolOutputsExpanded,
      setHasExpandableToolOutputs: state.setHasExpandableToolOutputs,
      setShowContextBreakdown: state.setShowContextBreakdown,
      setShowHelpPanel: state.setShowHelpPanel,
      setShowPromptsPanel: state.setShowPromptsPanel,
      setShowUsagePanel: state.setShowUsagePanel,
      setShowMcpPanel: state.setShowMcpPanel,
      setShowToolsPanel: state.setShowToolsPanel,
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
      currentAgent: state.currentAgent,
      previousAgentName: state.previousAgentName,
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
 * Queue state selector - for QueueStack and ConversationView queued messages
 */
export const useQueueState = () =>
  useAppStore(
    useShallow((state) => ({
      queuedMessages: state.queuedMessages,
    }))
  );

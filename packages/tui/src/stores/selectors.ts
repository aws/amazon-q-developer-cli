/**
 * Optimized selectors using useShallow to prevent unnecessary re-renders.
 *
 * These hooks group related state and return stable references when values haven't changed.
 */
import { useShallow } from 'zustand/react/shallow';
import { useAppStore, type AppState } from './app-store.js';

// Type for actions (functions) - these are stable references from the store
type AppActions = ReturnType<typeof useAppStore>;

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
    }))
  );

/**
 * Approval state selector - for ApprovalRequest
 */
export const useApprovalState = () =>
  useAppStore(
    useShallow((state) => ({
      pendingApproval: state.pendingApproval,
      respondToApproval: state.respondToApproval,
      cancelApproval: state.cancelApproval,
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
      showHelpPanel: state.showHelpPanel,
      helpCommands: state.helpCommands,
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

import { createStore, useStore } from 'zustand';
import { Kiro } from '../kiro';
import { type Theme, defaultTheme, noColorTheme } from '../types/theme';
import { createContext, useContext } from 'react';
import {
  AgentEventType,
  ApprovalOptionId,
  type AgentStreamEvent,
  type ApprovalRequestInfo,
  type ToolKind,
} from '../types/agent-events';
import type {
  InputBufferState,
  InputBufferActions,
  MoveCursorDir,
} from '../types/input-buffer';
import type { AvailableCommand, CommandOption } from '../types/commands';
import type { StatusType } from '../types/componentTypes';
import {
  executeCommand,
  executeCommandWithArg,
  type CommandContext,
} from '../commands/index.js';
import { expandFileReferences, readFileContent } from '../utils/file-search.js';
import { logger } from '../utils/logger.js';
import {
  getAuthErrorGuidance,
  getSessionErrorGuidance,
  getErrorGuidance,
  simplifyErrorMessage,
  detectErrorCategory,
} from '../utils/error-guidance.js';
import { CommandHistory } from '../utils/command-history.js';

export enum MessageRole {
  User = 'user',
  Model = 'model',
  ToolUse = 'tool_use',
  System = 'system',
}

// Helper to generate unique message IDs
const generateMessageId = () => crypto.randomUUID();

export enum ToolUseStatus {
  Pending = 'pending',
  Approved = 'approved',
  Rejected = 'rejected',
}

export type ToolResult =
  | { status: 'success'; output: unknown }
  | { status: 'error'; error: string }
  | { status: 'cancelled' };

export type MessageType =
  | { id: string; role: MessageRole.User; content: string; agentName?: string }
  | { id: string; role: MessageRole.Model; content: string; agentName?: string }
  | {
      id: string;
      role: MessageRole.ToolUse;
      name: string;
      kind?: ToolKind;
      content: string;
      isFinished?: boolean;
      status?: ToolUseStatus;
      result?: ToolResult;
      locations?: Array<{ path: string; line?: number }>;
      agentName?: string;
    }
  | { id: string; role: MessageRole.System; content: string; success: boolean };

export interface SlashCommand extends AvailableCommand {
  source: 'local' | 'backend';
}

export interface ActiveCommand {
  command: SlashCommand;
  options: CommandOption[];
}

export interface TransientAlert {
  message: string;
  status: StatusType;
  autoHideMs?: number;
}

export interface LastTurnTokens {
  input: number;
  output: number;
  cached: number;
}

const initialInputBufferState = (): InputBufferState => ({
  lines: [''],
  cursorRow: 0,
  cursorCol: 0,
  preferredCursorCol: 0,
  undoStack: [],
  redoStack: [],
  viewportWidth: 0,
  viewportHeight: 0,
  visibleLines: [],
  logicalToVisibleMap: [],
  visibleToLogicalMap: [],
});

interface AppStoreProps {
  kiro: Kiro;
}

type AppActions = BaseAppActions & InputBufferActions;

interface BaseAppActions {
  // Kiro actions
  sendMessage: (content: string) => Promise<void>;
  createStreamEventHandler: () => (event: AgentStreamEvent) => void;
  processMessageStream: (
    stream: AsyncGenerator<AgentStreamEvent>
  ) => Promise<void>;
  cancelMessage: () => Promise<void>;
  setProcessing: (processing: boolean) => void;
  setAgentError: (error: string | null, guidance?: string | null) => void;
  respondToApproval: (optionId: string) => void;
  cancelApproval: () => void;
  setCurrentModel: (model: { id: string; name: string } | null) => void;
  setCurrentAgent: (agent: { name: string } | null) => void;
  handleCompactionEvent: (event: AgentStreamEvent) => void;

  // Chat actions
  clearMessages: () => void;
  queueMessage: (content: string) => void;
  processQueue: () => Promise<void>;
  setSlashCommands: (commands: SlashCommand[]) => void;

  // Command UI actions
  setActiveCommand: (command: ActiveCommand | null) => void;
  executeCommandWithArg: (arg: string) => Promise<void>;
  setCommandInput: (value: string) => void;
  setActiveTrigger: (
    trigger: { key: string; position: number; type: 'start' | 'inline' } | null
  ) => void;
  setFilePickerHasResults: (hasResults: boolean) => void;
  clearCommandInput: () => void;

  navigateHistory: (direction: 'up' | 'down') => string | null;

  // UI actions
  setMode: (mode: 'inline' | 'expanded') => void;
  incrementExitSequence: () => void;
  resetExitSequence: () => void;
  showTransientAlert: (alert: TransientAlert) => void;
  dismissTransientAlert: () => void;
  setLoadingMessage: (message: string | null) => void;
  toggleToolOutputsExpanded: () => void;
  setHasExpandableToolOutputs: (has: boolean) => void;

  // Context usage actions
  setContextUsage: (percent: number) => void;
  setLastTurnTokens: (tokens: LastTurnTokens) => void;
  toggleContextBreakdown: () => void;
  setShowContextBreakdown: (show: boolean) => void;
  setShowHelpPanel: (show: boolean, commands?: Array<{ name: string; description: string; usage: string }>) => void;

  // File attachment actions
  attachFile: (path: string) => void;
  removeAttachedFile: (path: string) => void;
  clearAttachedFiles: () => void;
  setPendingFileAttachment: (
    path: string | null,
    triggerPosition?: number
  ) => void;
  consumePendingFileAttachment: () => {
    path: string;
    triggerPosition: number;
  } | null;

  // Theme actions
  setTheme: (theme: Theme) => void;

  // Main orchestrator
  handleUserInput: (input: string) => Promise<void>;
}

const getInitialTheme = (): Theme => {
  return process.env.NO_COLOR ? noColorTheme : defaultTheme;
};

export const AppStoreContext = createContext<AppStoreApi | null>(null);

export type AppStoreApi = ReturnType<typeof createAppStore>;

export interface AppState {
  // Chat state
  messages: MessageType[];
  queuedMessages: string[];
  slashCommands: SlashCommand[];

  // Kiro/Agent state
  kiro: Kiro;
  sessionId: string | null;
  isProcessing: boolean;
  isCompacting: boolean;
  agentError: string | null;
  agentErrorGuidance: string | null;
  pendingApproval: ApprovalRequestInfo | null;
  currentModel: { id: string; name: string } | null;
  currentAgent: { name: string } | null;

  // Command UI state
  activeCommand: ActiveCommand | null;
  commandInputValue: string;
  activeTrigger: {
    key: string;
    position: number;
    type: 'start' | 'inline';
  } | null;
  filePickerHasResults: boolean;

  // Input state
  input: InputBufferState;

  // UI state
  mode: 'inline' | 'expanded';
  exitSequence: number;
  exitTimer: NodeJS.Timeout | null;
  transientAlert: TransientAlert | null;
  loadingMessage: string | null;
  toolOutputsExpanded: boolean; // Global toggle for all tool outputs
  hasExpandableToolOutputs: boolean; // Whether there are any tool outputs that can be expanded

  // Context usage state
  contextUsagePercent: number | null;
  lastTurnTokens: LastTurnTokens | null;

  // File attachments
  attachedFiles: string[];
  pendingFileAttachment: { path: string; triggerPosition: number } | null;
  showContextBreakdown: boolean;
  showHelpPanel: boolean;
  helpCommands: Array<{ name: string; description: string; usage: string }>;

  // Abort controller for current stream
  currentAbortController: AbortController | null;

  // Streaming buffer control (typed properly instead of `any`)
  streamingBuffer: {
    startBuffering: (() => void) | null;
    stopBuffering: (() => void) | null;
  };

  // Theme state
  theme: Theme;
}

interface AppStoreProps {
  kiro: Kiro;
}

interface AppStoreProps {
  kiro: Kiro;
}

export const useAppStore = <T>(
  selector: (state: AppState & AppActions) => T
) => {
  const store = useContext(AppStoreContext);
  if (!store) throw new Error('Missing StoreContext.Provider in the tree');
  return useStore(store, selector);
};

export const createAppStore = (props: AppStoreProps) =>
  createStore<AppState & AppActions>((set, get) => ({
    // Initial state
    messages: [],
    queuedMessages: [],
    slashCommands: [], // Backend sends all commands via CommandsUpdate
    kiro: props.kiro,
    sessionId: null,
    isProcessing: false,
    isCompacting: false,
    agentError: null,
    agentErrorGuidance: null,
    pendingApproval: null,
    currentModel: null,
    currentAgent: null,

    activeCommand: null,
    commandInputValue: '',
    activeTrigger: null,
    filePickerHasResults: false,

    input: initialInputBufferState(),

    mode: 'inline',

    exitSequence: 0,
    exitTimer: null,
    transientAlert: null,
    loadingMessage: null as string | null,
    toolOutputsExpanded: false,
    hasExpandableToolOutputs: false,

    contextUsagePercent: null,
    lastTurnTokens: null,
    showContextBreakdown: false,
    showHelpPanel: false,
    helpCommands: [],
    attachedFiles: [],
    pendingFileAttachment: null,
    currentAbortController: null,
    streamingBuffer: { startBuffering: null, stopBuffering: null },

    theme: getInitialTheme(),

    sendMessage: async (content: string) => {
      const { kiro, isProcessing, attachedFiles } = get();
      if (isProcessing) {
        return;
      }

      logger.debug('[store] sendMessage', { contentLength: content.length });

      // Add to history
      CommandHistory.getInstance().add(content);

      // Expand @file: references and attached files
      let expandedContent = expandFileReferences(content);
      for (const filePath of attachedFiles) {
        const fileContent = readFileContent(filePath);
        if (fileContent) {
          expandedContent += `\n<attached_file path="${filePath}">\n${fileContent}\n</attached_file>`;
        }
      }

      const abortController = new AbortController();
      set({ currentAbortController: abortController });

      const userMessageId = generateMessageId();
      const userMessage: MessageType = {
        id: userMessageId,
        role: MessageRole.User,
        content, // Show original content in UI
        agentName: get().currentAgent?.name,
      };

      set((state) => ({
        isProcessing: true,
        agentError: null,
        agentErrorGuidance: null,
        messages: [...state.messages, userMessage],
        attachedFiles: [], // Clear attachments after sending
        // Reset expandable state for new turn
        hasExpandableToolOutputs: false,
        toolOutputsExpanded: false,
      }));

      try {
        const eventHandler = get().createStreamEventHandler();
        await kiro.streamMessage(
          expandedContent,
          abortController.signal,
          eventHandler,
        );
        (eventHandler as any).flush?.();

        // Mark any remaining tool calls as finished and mark turn as complete
        // Clear agentError on successful completion (Requirement 4.3)
        set((state) => {
          // Check if any tool calls need to be marked finished
          const hasUnfinishedToolCalls = state.messages.some(
            (msg) => msg.role === MessageRole.ToolUse && !msg.isFinished
          );

          if (hasUnfinishedToolCalls) {
            const messages = state.messages.map((msg) => {
              if (msg.role === MessageRole.ToolUse && !msg.isFinished) {
                return { ...msg, isFinished: true };
              }
              return msg;
            });
            return {
              messages,
              isProcessing: false,
              currentAbortController: null,
              agentError: null,
              agentErrorGuidance: null,
            };
          }

          return {
            isProcessing: false,
            currentAbortController: null,
            agentError: null,
            agentErrorGuidance: null,
          };
        });
      } catch (error) {
        set({ currentAbortController: null });
        logger.error('[store] sendMessage: caught error', error);
        if (error instanceof DOMException && error.name === 'AbortError')
          return;
        // Extract error message
        let errorMessage = 'Unknown error';
        if (error instanceof Error) {
          errorMessage = error.message || error.name || 'Unknown error';
        } else if (typeof error === 'string') {
          errorMessage = error;
        } else if (typeof error === 'object' && error !== null) {
          const errObj = error as Record<string, unknown>;
          if (typeof errObj.message === 'string' && errObj.message) {
            errorMessage = errObj.message;
          } else if (
            typeof errObj.error === 'object' &&
            errObj.error !== null
          ) {
            const innerErr = errObj.error as Record<string, unknown>;
            if (typeof innerErr.message === 'string' && innerErr.message) {
              errorMessage = innerErr.message;
            }
          }
        }

        // Mark any remaining tool calls as finished on error
        set((state) => {
          const hasUnfinishedToolCalls = state.messages.some(
            (msg) => msg.role === MessageRole.ToolUse && !msg.isFinished
          );
          if (hasUnfinishedToolCalls) {
            return {
              messages: state.messages.map((msg) =>
                msg.role === MessageRole.ToolUse && !msg.isFinished
                  ? { ...msg, isFinished: true }
                  : msg
              ),
            };
          }
          return state;
        });

        // Determine error category and handle accordingly
        const category = detectErrorCategory(errorMessage);
        const displayMessage = simplifyErrorMessage(errorMessage);

        // Only auth and session errors are blocking (require user action)
        if (category === 'auth' || category === 'session') {
          set({
            agentError: displayMessage,
            agentErrorGuidance: getErrorGuidance(errorMessage).message,
            isProcessing: false,
          });
        } else {
          // All other errors are non-blocking (transient alerts)
          get().showTransientAlert({
            message: displayMessage,
            status: 'error',
            autoHideMs: 5000,
          });
          set({ isProcessing: false });
        }
      }
    },

    /**
     * Creates a synchronous event handler callback for stream events.
     * This is the core event-processing logic, used by streamMessage.
     * Returns a cleanup function via the returned handler's `.flush` property.
     */
    createStreamEventHandler: () => {
      let isBuffering = false;
      let bufferedContent = '';

      // Batching: accumulate content chunks and flush to the store
      // on a timer so Ink's render loop isn't starved by rapid-fire
      // synchronous set() calls from the ACP notification handler.
      let pendingContentFlush: ReturnType<typeof setTimeout> | null = null;
      let lastContentEventId: string | null = null;

      const startBuffering = () => {
        isBuffering = true;
      };

      const commitBufferedContent = () => {
        if (!bufferedContent) return;
        set((state) => {
          const messages = state.messages.map((msg) => {
            if (msg.role === MessageRole.ToolUse && !msg.isFinished) {
              return { ...msg, isFinished: true };
            }
            return msg;
          });

          const lastModelMsgIndex = messages.findLastIndex(
            (msg) => msg.role === MessageRole.Model
          );
          if (lastModelMsgIndex !== -1) {
            const msg = messages[lastModelMsgIndex];
            if (msg && msg.role === MessageRole.Model) {
              messages[lastModelMsgIndex] = {
                ...msg,
                content: bufferedContent,
              };
              return { messages };
            }
          }
          return { messages };
        });
      };

      const flushContentToStore = () => {
        pendingContentFlush = null;
        if (!bufferedContent) return;

        set((state) => {
          const messages = state.messages.map((msg) => {
            if (msg.role === MessageRole.ToolUse && !msg.isFinished) {
              return { ...msg, isFinished: true };
            }
            return msg;
          });

          const lastMsg = messages[messages.length - 1];
          if (lastMsg?.role === MessageRole.Model) {
            return {
              messages: [
                ...messages.slice(0, -1),
                {
                  id: lastMsg.id,
                  role: MessageRole.Model,
                  content: bufferedContent,
                  agentName: lastMsg.agentName ?? state.currentAgent?.name,
                },
              ],
            };
          } else {
            return {
              messages: [
                ...messages,
                {
                  id: lastContentEventId ?? crypto.randomUUID(),
                  role: MessageRole.Model,
                  content: bufferedContent,
                  agentName: state.currentAgent?.name,
                },
              ],
            };
          }
        });
      };

      const stopBuffering = () => {
        if (isBuffering && bufferedContent) {
          commitBufferedContent();
          isBuffering = false;
        }
      };

      set({ streamingBuffer: { startBuffering, stopBuffering } });

      const handler = (event: AgentStreamEvent) => {
        switch (event.type) {
          case AgentEventType.Content:
            if (event.content.type === 'text') {
              const text = event.content.text;
              bufferedContent += text;
              lastContentEventId = event.id;

              if (!isBuffering) {
                // Schedule a batched flush instead of calling set() for
                // every chunk.  This lets Ink render between flushes.
                if (!pendingContentFlush) {
                  pendingContentFlush = setTimeout(flushContentToStore, 16);
                }
              }
            }
            break;
          case AgentEventType.ToolCall:
            if (isBuffering && bufferedContent) {
              commitBufferedContent();
              isBuffering = false;
            }
            // Flush any pending batched content before adding tool message
            if (pendingContentFlush) {
              clearTimeout(pendingContentFlush);
              pendingContentFlush = null;
              flushContentToStore();
            }
            // Reset buffer so the next Model message after this tool
            // doesn't repeat text from before the tool call.
            bufferedContent = '';
            lastContentEventId = null;

            set((state) => {
              const existingIndex = state.messages.findIndex(
                (msg) => msg.role === MessageRole.ToolUse && msg.id === event.id
              );

              let content: string;
              const diff = event.toolContent?.[0];
              if (diff) {
                const args = event.args as Record<string, unknown>;
                let command = 'create';
                if (args.oldStr !== undefined) {
                  command = 'strReplace';
                } else if (args.insertLine !== undefined || args.append) {
                  command = 'insert';
                }
                content = JSON.stringify({
                  command,
                  path: diff.path,
                  content: diff.newText,
                  oldStr: diff.oldText,
                  newStr: diff.newText,
                  insertLine: args.insertLine,
                });
              } else if (event.kind === 'edit') {
                const args = event.args as Record<string, unknown>;
                let command = 'create';
                if (args.oldStr !== undefined) {
                  command = 'strReplace';
                } else if (args.insertLine !== undefined || args.append) {
                  command = 'insert';
                }
                content = JSON.stringify({
                  command,
                  path: args.path,
                  content: args.text || args.content || '',
                  oldStr: args.oldStr,
                  newStr: args.newStr,
                  insertLine: args.insertLine,
                });
              } else {
                content = JSON.stringify(event.args);
              }

              if (existingIndex !== -1) {
                const existingMsg = state.messages[existingIndex];
                if (existingMsg && existingMsg.role === MessageRole.ToolUse) {
                  const hasNewContent =
                    Object.keys(event.args).length > 0 || event.toolContent;
                  if (hasNewContent) {
                    const messages = [...state.messages];
                    messages[existingIndex] = {
                      ...existingMsg,
                      content,
                      kind: event.kind || existingMsg.kind,
                      locations: event.locations || existingMsg.locations,
                    };
                    return { messages };
                  }
                }
                return state;
              }

              const messages = state.messages.map((msg) => {
                if (msg.role === MessageRole.ToolUse && !msg.isFinished) {
                  return { ...msg, isFinished: true };
                }
                return msg;
              });

              return {
                messages: [
                  ...messages,
                  {
                    id: event.id,
                    role: MessageRole.ToolUse,
                    name: event.name,
                    kind: event.kind,
                    content,
                    locations: event.locations,
                    agentName: state.currentAgent?.name,
                  },
                ],
              };
            });
            break;
          case AgentEventType.ToolCallUpdate:
            if (event.content.type === 'text') {
              const text = event.content.text;
              set((state) => {
                const toolMsgIndex = state.messages.findIndex(
                  (msg) =>
                    msg.role === MessageRole.ToolUse && msg.id === event.id
                );
                if (toolMsgIndex !== -1) {
                  const toolMsg = state.messages[toolMsgIndex];
                  if (toolMsg && toolMsg.role === MessageRole.ToolUse) {
                    const messages = [...state.messages];
                    messages[toolMsgIndex] = {
                      id: toolMsg.id,
                      name: toolMsg.name,
                      kind: toolMsg.kind,
                      role: MessageRole.ToolUse,
                      content: toolMsg.content + text,
                      locations: toolMsg.locations,
                      agentName: toolMsg.agentName,
                    };
                    return { messages };
                  }
                }
                return state;
              });
            }
            break;
          case AgentEventType.ToolCallFinished:
            set((state) => {
              const messages = [...state.messages];
              const toolMsgIndex = messages.findIndex(
                (msg) => msg.role === MessageRole.ToolUse && msg.id === event.id
              );
              if (toolMsgIndex !== -1) {
                const toolMsg = messages[toolMsgIndex];
                if (toolMsg && toolMsg.role === MessageRole.ToolUse) {
                  messages[toolMsgIndex] = {
                    id: toolMsg.id,
                    role: MessageRole.ToolUse,
                    name: toolMsg.name,
                    kind: toolMsg.kind,
                    content: toolMsg.content,
                    isFinished: true,
                    result: event.result,
                    locations: toolMsg.locations,
                    agentName: toolMsg.agentName,
                  };
                }
              }
              return { messages };
            });
            break;
          case AgentEventType.ApprovalRequest:
            set({ pendingApproval: event.value });
            break;
          case AgentEventType.ContextUsage:
            get().setContextUsage(event.percent);
            break;
          case AgentEventType.Metadata:
            if (
              event.inputTokens !== undefined ||
              event.outputTokens !== undefined
            ) {
              get().setLastTurnTokens({
                input: event.inputTokens ?? 0,
                output: event.outputTokens ?? 0,
                cached: event.cachedTokens ?? 0,
              });
            }
            break;
          case AgentEventType.CompactionStatus:
            if (event.status === 'started') {
              set({ isCompacting: true });
            } else if (event.status === 'completed') {
              set({ isCompacting: false });
              get().showTransientAlert({
                message: 'Conversation compacted',
                status: 'success',
                autoHideMs: 3000,
              });
            } else if (event.status === 'failed') {
              set({ isCompacting: false });
              get().showTransientAlert({
                message: `Compaction failed: ${event.error ?? 'unknown error'}`,
                status: 'error',
                autoHideMs: 5000,
              });
            }
            break;
          case AgentEventType.AuthError:
            {
              const guidance = getAuthErrorGuidance(event.errorType);
              set({
                agentError: event.message,
                agentErrorGuidance: guidance.message,
                isProcessing: false,
              });
            }
            break;
          case AgentEventType.SessionError:
            {
              const guidance = getSessionErrorGuidance(
                event.errorType,
                event.pid
              );
              set({
                agentError: event.message,
                agentErrorGuidance: guidance.message,
                isProcessing: false,
              });
            }
            break;
          case AgentEventType.McpServerInitFailure:
            {
              const message = `MCP server "${event.serverName}" failed to initialize: ${event.error}`;
              get().showTransientAlert({
                message,
                status: 'error',
                autoHideMs: 5000,
              });
            }
            break;
          case AgentEventType.RateLimitError:
            {
              get().showTransientAlert({
                message: event.message,
                status: 'error',
                autoHideMs: 5000,
              });
            }
            break;
        }
      };

      // Attach flush for callers to commit remaining buffered content
      (handler as any).flush = () => {
        // Cancel any pending batched flush and commit immediately
        if (pendingContentFlush) {
          clearTimeout(pendingContentFlush);
          pendingContentFlush = null;
        }
        // flushContentToStore handles both creating new and updating
        // existing Model messages — no need to also call commitBufferedContent
        flushContentToStore();
        set({ streamingBuffer: { startBuffering: null, stopBuffering: null } });
      };

      return handler;
    },

    /**
     * Backward-compatible wrapper: consumes an async generator using the
     * event handler. Used by unit tests that pass mock async generators.
     */
    processMessageStream: async (stream: AsyncGenerator<AgentStreamEvent>) => {
      const handler = get().createStreamEventHandler();
      for await (const event of stream) {
        handler(event);
      }
      (handler as any).flush?.();
    },

    cancelMessage: async () => {
      const { kiro, currentAbortController } = get();
      if (!kiro) return;

      try {
        // Abort local stream first
        if (currentAbortController) {
          currentAbortController.abort();
          set({ currentAbortController: null });
        }
        
        // Cancel any pending approval
        get().cancelApproval();
        
        // Then notify backend
        await kiro.cancel();
        
        // Mark any unfinished tool uses as finished with cancelled status
        set((state) => {
          const hasUnfinishedToolCalls = state.messages.some(
            (msg) => msg.role === MessageRole.ToolUse && !msg.isFinished
          );
          
          if (hasUnfinishedToolCalls) {
            return {
              messages: state.messages.map((msg) =>
                msg.role === MessageRole.ToolUse && !msg.isFinished
                  ? { ...msg, isFinished: true, result: { status: 'cancelled' } }
                  : msg
              ),
              isProcessing: false,
            };
          }
          
          return { isProcessing: false };
        });
        
        get().showTransientAlert({
          message: 'Cancelled streaming',
          status: 'info',
          autoHideMs: 2000,
        });
      } catch (error) {
        const errorMessage =
          error instanceof Error ? error.message : 'Cancel failed';
        set({
          agentError: errorMessage,
          agentErrorGuidance: getErrorGuidance(errorMessage).message,
          isProcessing: false,
        });
      }
    },

    setProcessing: (isProcessing) => set({ isProcessing }),
    setAgentError: (agentError, guidance) =>
      set({ agentError, agentErrorGuidance: guidance ?? null }),
    setCurrentModel: (currentModel) => set({ currentModel }),
    setCurrentAgent: (currentAgent) => set({ currentAgent }),

    handleCompactionEvent: (event) => {
      if (event.type === AgentEventType.ContextUsage) {
        get().setContextUsage(event.percent);
        return;
      }
      if (event.type !== AgentEventType.CompactionStatus) return;
      if (event.status === 'started') {
        set({ isCompacting: true });
      } else if (event.status === 'completed') {
        set({ isCompacting: false });
        get().showTransientAlert({
          message: 'Conversation compacted',
          status: 'success',
          autoHideMs: 3000,
        });
      } else if (event.status === 'failed') {
        set({ isCompacting: false });
        get().showTransientAlert({
          message: `Compaction failed: ${event.error ?? 'unknown error'}`,
          status: 'error',
          autoHideMs: 5000,
        });
      }
    },

    respondToApproval: (optionId: string) => {
      const { pendingApproval } = get();
      if (pendingApproval) {
        const toolCallId = pendingApproval.toolCall.toolCallId;
        const isRejected =
          optionId === ApprovalOptionId.RejectOnce ||
          optionId === ApprovalOptionId.RejectAlways;

        // Update the tool call status based on user response
        set((state) => ({
          messages: state.messages.map((msg) => {
            if (msg.role === MessageRole.ToolUse && msg.id === toolCallId) {
              return {
                ...msg,
                status: isRejected
                  ? ToolUseStatus.Rejected
                  : ToolUseStatus.Approved,
                isFinished: isRejected ? true : msg.isFinished,
              };
            }
            return msg;
          }),
        }));

        pendingApproval.resolve({
          outcome: 'selected',
          optionId,
        });
        set({ pendingApproval: null });
      }
    },

    cancelApproval: () => {
      const { pendingApproval } = get();
      if (pendingApproval) {
        const toolCallId = pendingApproval.toolCall.toolCallId;

        // Mark the tool call as cancelled
        set((state) => ({
          messages: state.messages.map((msg) => {
            if (msg.role === MessageRole.ToolUse && msg.id === toolCallId) {
              return {
                ...msg,
                isFinished: true,
                result: { status: 'cancelled' as const },
              };
            }
            return msg;
          }),
        }));

        pendingApproval.resolve({ outcome: 'cancelled' });
        set({ pendingApproval: null });
      }
    },

    // Clear conversation but keep last turn visible in UI
    clearMessages: () => {
      const msgs = get().messages;
      if (msgs.length < 2) return;
      
      // Find the last user message to keep the entire last turn
      let lastUserIndex = -1;
      for (let i = msgs.length - 1; i >= 0; i--) {
        if (msgs[i].role === MessageRole.User) {
          lastUserIndex = i;
          break;
        }
      }
      
      if (lastUserIndex === -1) return;
      set({ messages: msgs.slice(lastUserIndex) });
    },

    setSlashCommands: (commands: SlashCommand[]) => {
      set((state) => {
        const localCommands = state.slashCommands.filter(
          (cmd) => cmd.source === 'local'
        );
        return { slashCommands: [...localCommands, ...commands] };
      });
    },

    setActiveCommand: (command: ActiveCommand | null) => {
      set({ activeCommand: command });
    },

    setCommandInput: (value: string) => {
      set({ commandInputValue: value });
    },

    setActiveTrigger: (trigger) => {
      set({ activeTrigger: trigger });
    },

    setFilePickerHasResults: (hasResults) => {
      set({ filePickerHasResults: hasResults });
    },

    clearCommandInput: () => {
      set({
        commandInputValue: '',
        activeTrigger: null,
        filePickerHasResults: false,
      });
    },

    executeCommandWithArg: async (arg: string) => {
      const { activeCommand } = get();
      if (!activeCommand) return;

      const cmdName = activeCommand.command.name.replace(/^\//, '');
      set({ activeCommand: null });

      const state = get();
      const ctx: CommandContext = {
        kiro: state.kiro,
        slashCommands: state.slashCommands,
        showAlert: (message, status, autoHideMs = 3000) =>
          state.showTransientAlert({ message, status, autoHideMs }),
        setLoadingMessage: state.setLoadingMessage,
        setActiveCommand: state.setActiveCommand,
        setCurrentModel: state.setCurrentModel,
        setCurrentAgent: state.setCurrentAgent,
        setShowContextBreakdown: (show) => set({ showContextBreakdown: show }),
        setShowHelpPanel: state.setShowHelpPanel,
        clearMessages: state.clearMessages,
        clearUIState: () =>
          set({ activeCommand: null, showContextBreakdown: false, showHelpPanel: false }),
      };

      await executeCommandWithArg(cmdName, arg, ctx);
    },

    queueMessage: (content: string) => {
      set((state) => ({ queuedMessages: [...state.queuedMessages, content] }));
    },

    processQueue: async () => {
      const { queuedMessages } = get();
      const nextMessage = queuedMessages[0];
      if (!nextMessage) return;

      set((state) => ({ queuedMessages: state.queuedMessages.slice(1) }));
      await get().handleUserInput(nextMessage);
    },

    // Input actions

    insert: (char: string) => {
      set((state) => {
        const { lines, cursorRow, cursorCol } = state.input;
        const newLines = [...lines];
        const line = newLines[cursorRow] ?? '';
        newLines[cursorRow] =
          line.slice(0, cursorCol) + char + line.slice(cursorCol);

        return {
          input: {
            ...state.input,
            lines: newLines,
            cursorCol: cursorCol + char.length,
            preferredCursorCol: cursorCol + char.length,
          },
        };
      });
    },
    newline: () => {
      set((state) => {
        const { lines, cursorRow, cursorCol } = state.input;
        const newLines = [...lines];
        const currentLine = newLines[cursorRow] ?? '';
        const beforeCursor = currentLine.slice(0, cursorCol);
        const afterCursor = currentLine.slice(cursorCol);

        newLines[cursorRow] = beforeCursor;
        newLines.splice(cursorRow + 1, 0, afterCursor);

        return {
          input: {
            ...state.input,
            lines: newLines,
            cursorRow: cursorRow + 1,
            cursorCol: 0,
            preferredCursorCol: 0,
          },
        };
      });
    },
    backspace: () => {
      set((state) => {
        const { lines, cursorRow, cursorCol } = state.input;

        if (cursorCol === 0 && cursorRow === 0) {
          return state;
        }

        const newLines = [...lines];

        if (cursorCol === 0) {
          // At start of line, merge with previous line
          const prevLine = newLines[cursorRow - 1] ?? '';
          const currentLine = newLines[cursorRow] ?? '';
          newLines[cursorRow - 1] = prevLine + currentLine;
          newLines.splice(cursorRow, 1);

          return {
            input: {
              ...state.input,
              lines: newLines,
              cursorRow: cursorRow - 1,
              cursorCol: prevLine.length,
              preferredCursorCol: prevLine.length,
            },
          };
        } else {
          // Delete character before cursor
          const line = newLines[cursorRow] ?? '';
          newLines[cursorRow] =
            line.slice(0, cursorCol - 1) + line.slice(cursorCol);

          return {
            input: {
              ...state.input,
              lines: newLines,
              cursorCol: cursorCol - 1,
              preferredCursorCol: cursorCol - 1,
            },
          };
        }
      });
    },
    delete: () => {},
    clearWord: () => {
      set((state) => {
        // todo
        return state;
      });
    },
    clearLine: () => {
      set((state) => {
        // todo
        return state;
      });
    },
    clearInput: () => {
      set(() => ({
        input: initialInputBufferState(),
      }));
    },
    moveCursor: (dir: MoveCursorDir) => {
      set((state) => {
        // todo
        return state;
      });
    },
    setViewport: (width: number, height: number) => {
      set((state) => {
        if (
          state.input.viewportWidth === width &&
          state.input.viewportHeight === height
        ) {
          return state;
        }
        return {
          input: {
            ...state.input,
            viewportWidth: width,
            viewportHeight: height,
          },
        };
      });
    },

    navigateHistory: (direction: 'up' | 'down') => {
      const history = CommandHistory.getInstance();
      const command = history.navigate(direction);
      return command;
    },

    // UI actions
    setMode: (mode) => set({ mode }),

    incrementExitSequence: () => {
      set((state) => {
        if (state.exitTimer) {
          clearTimeout(state.exitTimer);
        }

        const newSequence = state.exitSequence + 1;

        if (newSequence >= 2) {
          // Clean up kiro before exiting
          state.kiro.close();
          process.exit(0);
        }

        const timer = setTimeout(() => {
          set({ exitSequence: 0, exitTimer: null });
        }, 2000);

        return { exitSequence: newSequence, exitTimer: timer };
      });
    },

    resetExitSequence: () => {
      set((state) => {
        if (state.exitTimer) {
          clearTimeout(state.exitTimer);
        }
        return { exitSequence: 0, exitTimer: null };
      });
    },

    showTransientAlert: (alert) => {
      set({ transientAlert: alert });
    },

    dismissTransientAlert: () => {
      set({ transientAlert: null });
    },

    setLoadingMessage: (message) => {
      set({ loadingMessage: message });
    },

    // Context usage actions
    setContextUsage: (percent) => {
      set({ contextUsagePercent: percent });
    },

    setLastTurnTokens: (tokens) => {
      set({ lastTurnTokens: tokens });
    },

    toggleContextBreakdown: () => {
      set((state) => ({ showContextBreakdown: !state.showContextBreakdown }));
    },

    setShowContextBreakdown: (show) => {
      set({ showContextBreakdown: show });
    },

    setShowHelpPanel: (show, commands = []) => {
      set({ showHelpPanel: show, helpCommands: commands });
    },

    // File attachment actions
    attachFile: (path) => {
      set((state) => ({
        attachedFiles: state.attachedFiles.includes(path)
          ? state.attachedFiles
          : [...state.attachedFiles, path],
      }));
    },

    removeAttachedFile: (path) => {
      set((state) => ({
        attachedFiles: state.attachedFiles.filter((f) => f !== path),
      }));
    },

    clearAttachedFiles: () => {
      set({ attachedFiles: [] });
    },

    setPendingFileAttachment: (path, triggerPosition = 0) => {
      set({ pendingFileAttachment: path ? { path, triggerPosition } : null });
    },

    consumePendingFileAttachment: () => {
      const pending = get().pendingFileAttachment;
      set({ pendingFileAttachment: null });
      return pending;
    },

    toggleToolOutputsExpanded: () => {
      set((state) => ({ toolOutputsExpanded: !state.toolOutputsExpanded }));
    },

    setHasExpandableToolOutputs: (has: boolean) => {
      set({ hasExpandableToolOutputs: has });
    },

    // Theme actions
    setTheme: (theme) => {
      const finalTheme = process.env.NO_COLOR ? noColorTheme : theme;
      set({ theme: finalTheme });
    },

    // Main orchestrator
    handleUserInput: async (input: string) => {
      const trimmed = input.trim();
      if (!trimmed) return;

      const state = get();
      state.resetExitSequence();

      // Queue if processing
      if (state.isProcessing) {
        state.queueMessage(trimmed);
        state.clearInput();
        return;
      }

      // Clear all UI state before processing any input
      set({
        activeCommand: null,
        showContextBreakdown: false,
       commandInputValue: '',
        activeTrigger: null,
      });
      state.clearInput();

      // Handle slash commands via command registry
      if (trimmed.startsWith('/')) {
        const ctx: CommandContext = {
          kiro: state.kiro,
          slashCommands: state.slashCommands,
          showAlert: (message, status, autoHideMs = 3000) =>
            state.showTransientAlert({ message, status, autoHideMs }),
          setLoadingMessage: state.setLoadingMessage,
          setActiveCommand: state.setActiveCommand,
          setCurrentModel: state.setCurrentModel,
          setCurrentAgent: state.setCurrentAgent,
          setShowContextBreakdown: (show) =>
            set({ showContextBreakdown: show }),
          setShowHelpPanel: state.setShowHelpPanel,
          clearMessages: state.clearMessages,
          clearUIState: () =>
            set({ activeCommand: null, showContextBreakdown: false, showHelpPanel: false }),
        };
        await executeCommand(trimmed, ctx);
        return;
      }

      // Handle regular prompts
      await state.sendMessage(trimmed);
    },
  }));

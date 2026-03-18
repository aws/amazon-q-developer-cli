import { createStore, useStore } from 'zustand';
import { Kiro } from '../kiro';
import chalk from 'chalk';
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

export interface ContextBreakdownData {
  contextFiles: {
    percent: number;
    tokens: number;
    items?: Array<{
      name: string;
      tokens: number;
      matched: boolean;
      percent: number;
    }>;
  };
  tools: { percent: number; tokens: number };
  kiroResponses: { percent: number; tokens: number };
  yourPrompts: { percent: number; tokens: number };
  sessionFiles?: { percent: number; tokens: number };
}

export interface UsageBreakdownItem {
  displayName: string;
  used: number;
  limit: number;
  percentage: number;
  currentOverages: number;
  overageRate: number;
  overageCharges: number;
  currency: string;
}

export interface BonusCredit {
  name: string;
  used: number;
  total: number;
  daysUntilExpiry: number;
}

export interface UsageData {
  planName: string;
  overagesEnabled: boolean;
  isEnterprise: boolean;
  usageBreakdowns: UsageBreakdownItem[];
  bonusCredits: BonusCredit[];
}

export interface McpServerInfo {
  name: string;
  status: 'running' | 'loading' | 'failed' | 'disabled';
  toolCount: number;
}

export interface ToolInfo {
  name: string;
  source: string;
  description: string;
  status: 'allowed' | 'requires-approval' | 'denied';
}

export interface KnowledgeEntry {
  name: string;
  id: string;
  description: string;
  item_count: number;
  path: string | null;
  items_display?: string;
  indexing?: boolean;
}

import {
  executeCommand,
  executeCommandWithArg,
  type CommandContext,
} from '../commands/index.js';
import { formatImageLabel } from '../utils/image-label.js';
import { expandFileReferences, readFileContent } from '../utils/file-search.js';
import { logger } from '../utils/logger.js';
import {
  setTerminalProgressWarning,
  setTerminalProgressIndeterminate,
  setTerminalProgressError,
  clearTerminalProgress,
} from '../utils/terminal-capabilities.js';
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

/**
 * Tools that are known to be broken or unavailable in the current environment.
 * Tool calls matching these names are immediately marked as finished with an
 * error result so they don't block the flush state machine with a stuck spinner.
 */
export const NOT_READY_TOOLS: Set<string> = new Set([]);

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
  | {
      id: string;
      role: MessageRole.Model;
      content: string;
      agentName?: string;
      shellOutput?: boolean;
      standalone?: boolean;
    }
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

export type AppActions = BaseAppActions & InputBufferActions;

interface BaseAppActions {
  // Kiro actions
  sendMessage: (
    content: string,
    images?: Array<{ base64: string; mimeType: string }>
  ) => Promise<void>;
  createStreamEventHandler: () => (event: AgentStreamEvent) => void;
  processMessageStream: (
    stream: AsyncGenerator<AgentStreamEvent>
  ) => Promise<void>;
  cancelMessage: () => Promise<void>;
  setProcessing: (processing: boolean) => void;
  setAgentError: (error: string | null, guidance?: string | null) => void;
  respondToApproval: (optionId: string) => void;
  cancelApproval: () => void;
  setApprovalMode: (mode: 'dropdown' | 'drill-in') => void;
  setCurrentModel: (model: { id: string; name: string } | null) => void;
  setCurrentAgent: (
    agent: { name: string; welcomeMessage?: string } | null
  ) => void;
  setPreviousAgentName: (name: string | null) => void;
  handleCompactionEvent: (event: AgentStreamEvent) => void;

  // Chat actions
  clearMessages: () => void;
  queueMessage: (content: string) => void;
  processQueue: () => Promise<void>;
  clearQueue: () => void;
  setSlashCommands: (commands: SlashCommand[]) => void;
  setPrompts: (
    prompts: Array<{
      name: string;
      description?: string;
      arguments: Array<{
        name: string;
        description?: string;
        required?: boolean;
      }>;
      serverName: string;
    }>
  ) => void;

  // Command UI actions
  setActiveCommand: (command: ActiveCommand | null) => void;
  executeCommandWithArg: (arg: string) => Promise<void>;
  setCommandInput: (value: string) => void;
  setActiveTrigger: (
    trigger: { key: string; position: number; type: 'start' | 'inline' } | null
  ) => void;
  setFilePickerHasResults: (hasResults: boolean) => void;
  setPromptHint: (hint: string | null) => void;
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
  setShowContextBreakdown: (
    show: boolean,
    breakdown?: ContextBreakdownData
  ) => void;
  setShowHelpPanel: (
    show: boolean,
    commands?: Array<{ name: string; description: string; usage: string }>
  ) => void;
  setShowPromptsPanel: (show: boolean) => void;
  setShowIssuePanel: (show: boolean, url?: string) => void;
  setShowUsagePanel: (show: boolean, data?: any) => void;
  setShowMcpPanel: (show: boolean, servers?: McpServerInfo[]) => void;
  setShowToolsPanel: (show: boolean, tools?: ToolInfo[]) => void;
  setShowKnowledgePanel: (
    show: boolean,
    entries?: KnowledgeEntry[],
    status?: string
  ) => void;

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

  // Image attachment actions
  addPendingImage: (image: {
    base64: string;
    mimeType: string;
    width: number;
    height: number;
    sizeBytes: number;
  }) => void;
  removePendingImage: (index: number) => void;
  clearPendingImages: () => void;

  // Main orchestrator
  handleUserInput: (input: string) => Promise<void>;
}

export const AppStoreContext = createContext<AppStoreApi | null>(null);

export type AppStoreApi = ReturnType<typeof createAppStore>;

export interface AppState {
  // Chat state
  messages: MessageType[];
  queuedMessages: string[];
  slashCommands: SlashCommand[];
  prompts: Array<{
    name: string;
    description?: string;
    arguments: Array<{
      name: string;
      description?: string;
      required?: boolean;
    }>;
    serverName: string;
  }>;

  // Kiro/Agent state
  kiro: Kiro;
  onExit?: () => void;
  sessionId: string | null;
  isProcessing: boolean;
  isCompacting: boolean;
  agentError: string | null;
  agentErrorGuidance: string | null;
  pendingApproval: ApprovalRequestInfo | null;
  approvalQueue: ApprovalRequestInfo[];
  approvalMode: 'dropdown' | 'drill-in';
  currentModel: { id: string; name: string } | null;
  currentAgent: { name: string } | null;
  previousAgentName: string | null;

  // Command UI state
  activeCommand: ActiveCommand | null;
  commandInputValue: string;
  activeTrigger: {
    key: string;
    position: number;
    type: 'start' | 'inline';
  } | null;
  filePickerHasResults: boolean;
  promptHint: string | null;

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

  // Usage panel state
  showUsagePanel: boolean;
  usageData: UsageData | null;

  // File attachments
  attachedFiles: string[];
  pendingFileAttachment: { path: string; triggerPosition: number } | null;
  pendingImages: Array<{
    base64: string;
    mimeType: string;
    width: number;
    height: number;
    sizeBytes: number;
  }>;
  showContextBreakdown: boolean;
  contextBreakdown: ContextBreakdownData | null;
  showHelpPanel: boolean;
  helpCommands: Array<{ name: string; description: string; usage: string }>;
  showMcpPanel: boolean;
  mcpServers: McpServerInfo[];
  showToolsPanel: boolean;
  toolsList: ToolInfo[];
  showKnowledgePanel: boolean;
  knowledgeEntries: KnowledgeEntry[];
  knowledgeStatus: string | null;
  showPromptsPanel: boolean;
  showIssuePanel: boolean;
  issueUrl: string | null;
  // Abort controller for current stream
  currentAbortController: AbortController | null;
  cancelInProgress: Promise<void> | null;

  // Non-interactive mode
  noInteractive: boolean;

  // Streaming buffer control (typed properly instead of `any`)
  streamingBuffer: {
    startBuffering: (() => void) | null;
    stopBuffering: (() => void) | null;
  };
}

interface AppStoreProps {
  kiro: Kiro;
  noInteractive?: boolean;
  initialInput?: string;
}

interface AppStoreProps {
  kiro: Kiro;
  noInteractive?: boolean;
  initialInput?: string;
}

export const useAppStore = <T>(
  selector: (state: AppState & AppActions) => T
) => {
  const store = useContext(AppStoreContext);
  if (!store) throw new Error('Missing StoreContext.Provider in the tree');
  return useStore(store, selector);
};

const CONTEXT_WARNING_THRESHOLD = 60;

/**
 * Sync the OSC 9;4 terminal progress indicator to the current app state.
 *
 * When active (processing/compacting):
 *   - Spinning green                       — normal processing
 *   - Static yellow at 100%                — waiting for approval
 *   - Pulsing red                          — error during processing
 *
 * When idle:
 *   - Static yellow bar with context %     — context ≥ warning threshold
 *   - Pulsing red                          — error
 *   - Hidden                               — everything normal
 */
function syncTerminalProgress(
  state: Pick<
    AppState,
    'agentError' | 'pendingApproval' | 'isProcessing' | 'isCompacting' | 'contextUsagePercent'
  >,
): void {
  if (state.isProcessing || state.isCompacting) {
    // Active — always spinning unless paused for approval
    if (state.agentError) {
      setTerminalProgressError();                       // pulsing red
    } else if (state.pendingApproval) {
      setTerminalProgressWarning(100);                  // static yellow at 100%
    } else {
      setTerminalProgressIndeterminate();               // spinning green
    }
  } else {
    // Idle — static bar or hidden
    if (state.agentError) {
      setTerminalProgressError();                       // pulsing red
    } else if (
      state.contextUsagePercent != null &&
      state.contextUsagePercent >= CONTEXT_WARNING_THRESHOLD
    ) {
      setTerminalProgressWarning(state.contextUsagePercent);  // static yellow with %
    } else {
      clearTerminalProgress();                          // hidden
    }
  }
}

export const createAppStore = (props: AppStoreProps) => {
  const store = createStore<AppState & AppActions>((set, get) => ({
    // Initial state
    messages: [],
    queuedMessages: [],
    slashCommands: [
      {
        name: '/prompts',
        description: 'List available MCP prompts',
        source: 'local' as const,
        meta: { local: true, inputType: 'panel' as const },
      },
      {
        name: '/editor',
        description: 'Open $EDITOR to compose a prompt',
        source: 'local' as const,
        meta: { local: true },
      },
    ], // Backend sends all commands via CommandsUpdate
    prompts: [],
    kiro: props.kiro,
    sessionId: null,
    isProcessing: false,
    isCompacting: false,
    agentError: null,
    agentErrorGuidance: null,
    pendingApproval: null,
    approvalQueue: [],
    approvalMode: 'dropdown',
    currentModel: null,
    currentAgent: null,
    previousAgentName: null,

    activeCommand: null,
    commandInputValue: '',
    activeTrigger: null,
    filePickerHasResults: false,
    promptHint: null,

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
    contextBreakdown: null,
    showHelpPanel: false,
    helpCommands: [],
    showPromptsPanel: false,
    showIssuePanel: false,
    issueUrl: null,
    showUsagePanel: false,
    usageData: null,
    showMcpPanel: false,
    mcpServers: [],
    showToolsPanel: false,
    toolsList: [],
    showKnowledgePanel: false,
    knowledgeEntries: [],
    knowledgeStatus: null,
    attachedFiles: [],
    pendingFileAttachment: null,
    pendingImages: [],
    currentAbortController: null,
    cancelInProgress: null,
    streamingBuffer: { startBuffering: null, stopBuffering: null },

    noInteractive: props.noInteractive ?? false,

    sendMessage: async (
      content: string,
      images?: Array<{ base64: string; mimeType: string }>
    ) => {
      const { kiro, isProcessing, attachedFiles, pendingImages } = get();
      if (isProcessing) {
        return;
      }

      // Merge explicitly passed images with store pending images
      const allImages = [
        ...pendingImages.map(({ base64, mimeType }) => ({ base64, mimeType })),
        ...(images ?? []),
      ];

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

      // Build display content: use image labels with dimensions when no text
      const displayContent =
        content ||
        (pendingImages.length > 0
          ? pendingImages.map(formatImageLabel).join(' ')
          : allImages.length > 0
            ? '[pasted image]'
            : '');
      const userMessage: MessageType = {
        id: userMessageId,
        role: MessageRole.User,
        content: displayContent,
        agentName: get().currentAgent?.name,
      };

      set((state) => {
        return {
          isProcessing: true,
          agentError: null,
          agentErrorGuidance: null,
          messages: [...state.messages, userMessage],
          attachedFiles: [], // Clear attachments after sending
          pendingImages: [], // Clear pending images after sending
          // Reset expandable content flag for new turn (expanded state persists)
          hasExpandableToolOutputs: false,
        };
      });

      try {
        const eventHandler = get().createStreamEventHandler();
        await kiro.streamMessage(
          expandedContent,
          abortController.signal,
          eventHandler,
          allImages.length > 0 ? allImages : undefined
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
        await get().processQueue();
      } catch (error) {
        set({ currentAbortController: null });
        logger.error('[store] sendMessage: caught error', error);
        if (error instanceof DOMException && error.name === 'AbortError') {
          set({ isProcessing: false });
          await get().processQueue();
          return;
        }
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
          await get().processQueue();
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
          const messages = [...state.messages];

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
          const messages = [...state.messages];

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
          case AgentEventType.UserMessage:
            // Historical user message from a resumed session.
            // Flush any buffered assistant content from the previous turn
            // before adding the user message so turns don't bleed together.
            if (pendingContentFlush) {
              clearTimeout(pendingContentFlush);
              pendingContentFlush = null;
              flushContentToStore();
            }
            // Reset buffer for the next assistant turn
            bufferedContent = '';
            lastContentEventId = null;

            if (event.content.type === 'text') {
              const text = event.content.text;
              const id = event.id;
              set((state) => ({
                messages: [
                  ...state.messages,
                  {
                    id,
                    role: MessageRole.User,
                    content: text,
                    agentName: state.currentAgent?.name,
                  },
                ],
              }));
            }
            break;
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

              const isNotReady = NOT_READY_TOOLS.has(event.name);
              return {
                messages: [
                  ...state.messages,
                  {
                    id: event.id,
                    role: MessageRole.ToolUse,
                    name: event.name,
                    kind: event.kind,
                    content,
                    locations: event.locations,
                    agentName: state.currentAgent?.name,
                    ...(isNotReady && {
                      isFinished: true,
                      result: {
                        status: 'error' as const,
                        error: `Tool "${event.name}" is not available`,
                      },
                    }),
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
                    status: toolMsg.status,
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
            set((state) => {
              const newQueue = [...state.approvalQueue, event.value];
              const toolCallId = event.value.toolCall.toolCallId;
              return {
                approvalQueue: newQueue,
                pendingApproval: state.pendingApproval ?? event.value,
                // Mark the matching tool message as pending approval
                messages: state.messages.map((msg) =>
                  msg.role === MessageRole.ToolUse && msg.id === toolCallId
                    ? { ...msg, status: ToolUseStatus.Pending }
                    : msg
                ),
              };
            });
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
              set({ isCompacting: true, loadingMessage: null });
            } else if (event.status === 'completed') {
              set({ isCompacting: false, loadingMessage: null });
            } else if (event.status === 'failed') {
              set({ isCompacting: false, loadingMessage: null });
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
          case AgentEventType.AgentSwitched:
            get().setCurrentAgent({
              name: event.agentName,
              welcomeMessage: event.welcomeMessage,
            });
            if (event.previousAgentName) {
              set({ previousAgentName: event.previousAgentName });
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
      let resolveCancelPromise: () => void;
      const cancelPromise = new Promise<void>((resolve) => {
        resolveCancelPromise = resolve;
      });
      set({ cancelInProgress: cancelPromise });

      try {
        // Abort local stream first
        if (currentAbortController) {
          currentAbortController.abort();
          set({ currentAbortController: null });
        }

        // Cancel any pending approval
        get().cancelApproval();

        // Then notify backend — this must complete before a new prompt
        // can be sent, otherwise the backend rejects with
        // "Prompt already in progress".
        await kiro.cancel();

        // Mark any unfinished tool uses as finished with cancelled status.
        set((state) => {
          const hasUnfinishedToolCalls = state.messages.some(
            (msg) => msg.role === MessageRole.ToolUse && !msg.isFinished
          );

          if (hasUnfinishedToolCalls) {
            return {
              messages: state.messages.map((msg) =>
                msg.role === MessageRole.ToolUse && !msg.isFinished
                  ? {
                      ...msg,
                      isFinished: true,
                      result: { status: 'cancelled' },
                    }
                  : msg
              ),
            };
          }

          return {};
        });

        // Only show the alert when the queue is empty — if there are
        // queued messages the next one will start immediately and the
        // transient alert would just flash confusingly.
        if (get().queuedMessages.length === 0) {
          get().showTransientAlert({
            message: 'Cancelled streaming',
            status: 'info',
            autoHideMs: 2000,
          });
        }
      } catch (error) {
        const errorMessage =
          error instanceof Error ? error.message : 'Cancel failed';
        set({
          agentError: errorMessage,
          agentErrorGuidance: getErrorGuidance(errorMessage).message,
        });
      } finally {
        resolveCancelPromise!();
        set({ cancelInProgress: null });
      }
    },

    setProcessing: (isProcessing) => set({ isProcessing }),
    setAgentError: (agentError, guidance) =>
      set({ agentError, agentErrorGuidance: guidance ?? null }),
    setCurrentModel: (currentModel) => set({ currentModel }),
    setCurrentAgent: (agent) => {
      set({ currentAgent: agent ? { name: agent.name } : null });
      if (agent?.welcomeMessage) {
        set((state) => ({
          messages: [
            ...state.messages,
            {
              id: generateMessageId(),
              role: MessageRole.Model,
              content: agent.welcomeMessage!,
              agentName: agent.name,
              standalone: true,
            },
          ],
        }));
      }
    },
    setPreviousAgentName: (previousAgentName) => set({ previousAgentName }),

    handleCompactionEvent: (event) => {
      if (event.type === AgentEventType.ContextUsage) {
        logger.info(
          '[context-usage] ContextUsage event in compactionHandler, percent=',
          event.percent
        );
        get().setContextUsage(event.percent);
        return;
      }
      if (event.type !== AgentEventType.CompactionStatus) return;
      if (event.status === 'started') {
        set((state) => ({
          isCompacting: true,
          isProcessing: true,
          messages: [
            ...state.messages,
            { id: crypto.randomUUID(), role: MessageRole.User, content: '' },
          ],
        }));
      } else if (event.status === 'completed') {
        const summary = event.summary;
        set((state) => {
          const messages = [...state.messages];
          if (summary) {
            messages.push({
              id: crypto.randomUUID(),
              role: MessageRole.Model,
              content: summary,
            });
          }
          return { isCompacting: false, isProcessing: false, messages };
        });
      } else if (event.status === 'failed') {
        set({ isCompacting: false, isProcessing: false });
        get().showTransientAlert({
          message: `Compaction failed: ${event.error ?? 'unknown error'}`,
          status: 'error',
          autoHideMs: 5000,
        });
      }
    },

    respondToApproval: (optionId: string) => {
      const { pendingApproval, approvalQueue } = get();
      if (pendingApproval) {
        const toolCallId = pendingApproval.toolCall.toolCallId;
        const isRejected =
          optionId === ApprovalOptionId.RejectOnce ||
          optionId === ApprovalOptionId.RejectAlways;

        // Update the tool call status based on user response
        const remainingQueue = approvalQueue.filter(
          (a) => a !== pendingApproval
        );
        const nextApproval = remainingQueue[0] ?? null;

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
          approvalQueue: remainingQueue,
          pendingApproval: nextApproval,
          approvalMode: 'dropdown',
        }));

        pendingApproval.resolve({
          outcome: 'selected',
          optionId,
        });
      }
    },

    cancelApproval: () => {
      const { pendingApproval, approvalQueue } = get();
      if (pendingApproval) {
        const toolCallId = pendingApproval.toolCall.toolCallId;

        // Cancel all queued approvals, not just the current one
        const remainingQueue = approvalQueue.filter(
          (a) => a !== pendingApproval
        );

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

        // Cancel all remaining queued approvals too
        for (const queued of remainingQueue) {
          queued.resolve({ outcome: 'cancelled' });
        }

        set({
          pendingApproval: null,
          approvalQueue: [],
          approvalMode: 'dropdown',
        });
      }
    },

    setApprovalMode: (mode) => set({ approvalMode: mode }),

    // Clear conversation but keep last turn visible in UI
    clearMessages: () => {
      const msgs = get().messages;
      if (msgs.length < 2) return;

      // Find the last user message to keep the entire last turn
      let lastUserIndex = -1;
      for (let i = msgs.length - 1; i >= 0; i--) {
        if (msgs[i]?.role === MessageRole.User) {
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
          (cmd) => cmd.source === 'local' || cmd.meta?.type === 'prompt'
        );
        return { slashCommands: [...localCommands, ...commands] };
      });
    },

    setPrompts: (prompts) => {
      set({ prompts });
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

    setPromptHint: (hint) => {
      set({ promptHint: hint });
    },

    clearCommandInput: () => {
      set({
        commandInputValue: '',
        activeTrigger: null,
        filePickerHasResults: false,
        promptHint: null,
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
        prompts: state.prompts,
        showAlert: (message, status, autoHideMs = 3000) =>
          state.showTransientAlert({ message, status, autoHideMs }),
        setLoadingMessage: state.setLoadingMessage,
        setActiveCommand: state.setActiveCommand,
        setCurrentModel: state.setCurrentModel,
        setCurrentAgent: state.setCurrentAgent,
        setContextUsage: state.setContextUsage,
        setShowContextBreakdown: state.setShowContextBreakdown,
        setShowHelpPanel: state.setShowHelpPanel,
        setShowPromptsPanel: state.setShowPromptsPanel,
        setShowIssuePanel: state.setShowIssuePanel,
        setShowUsagePanel: state.setShowUsagePanel,
        setShowMcpPanel: state.setShowMcpPanel,
        setShowToolsPanel: state.setShowToolsPanel,
        setShowKnowledgePanel: state.setShowKnowledgePanel,
        clearMessages: state.clearMessages,
        sendMessage: state.sendMessage,
        clearUIState: () =>
          set({
            activeCommand: null,
            showContextBreakdown: false,
            showHelpPanel: false,
            showPromptsPanel: false,
            showIssuePanel: false,
            showUsagePanel: false,
            showMcpPanel: false,
            showToolsPanel: false,
            showKnowledgePanel: false,
            contextBreakdown: null,
            usageData: null,
          }),
      };

      await executeCommandWithArg(cmdName, arg, ctx);
    },

    queueMessage: (content: string) => {
      const trimmed = content.trim();
      if (!trimmed) return;
      set((state) => ({ queuedMessages: [...state.queuedMessages, trimmed] }));
    },

    processQueue: async () => {
      const { cancelInProgress } = get();
      if (cancelInProgress) {
        await cancelInProgress;
      }

      const { queuedMessages } = get();
      const nextMessage = queuedMessages[0];
      if (!nextMessage) return;

      set((state) => ({ queuedMessages: state.queuedMessages.slice(1) }));
      await get().sendMessage(nextMessage);
    },

    clearQueue: () => {
      set({ queuedMessages: [] });
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
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    moveCursor: (_dir: MoveCursorDir) => {
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
          // Clean up kiro and renderer before exiting
          state.kiro.close();
          state.onExit?.();
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
      logger.info('[context-usage] setContextUsage called, percent=', percent);
      set({ contextUsagePercent: percent });
    },

    setLastTurnTokens: (tokens) => {
      set({ lastTurnTokens: tokens });
    },

    toggleContextBreakdown: () => {
      set((state) => ({ showContextBreakdown: !state.showContextBreakdown }));
    },

    setShowContextBreakdown: (show, breakdown) => {
      set({ showContextBreakdown: show, contextBreakdown: breakdown ?? null });
    },

    setShowHelpPanel: (show, commands = []) => {
      set({ showHelpPanel: show, helpCommands: commands });
    },

    setShowPromptsPanel: (show) => {
      set({ showPromptsPanel: show });
    },

    setShowIssuePanel: (show, url) => {
      set({ showIssuePanel: show, issueUrl: url ?? null });
    },

    setShowUsagePanel: (show, data) => {
      set({ showUsagePanel: show, usageData: data ?? null });
    },

    setShowMcpPanel: (show, servers = []) => {
      set({ showMcpPanel: show, mcpServers: servers });
    },

    setShowToolsPanel: (show, tools = []) => {
      set({ showToolsPanel: show, toolsList: tools });
    },

    setShowKnowledgePanel: (show, entries = [], status) => {
      set({
        showKnowledgePanel: show,
        knowledgeEntries: entries,
        knowledgeStatus: status ?? null,
      });
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

    addPendingImage: (image) => {
      set((state) => ({ pendingImages: [...state.pendingImages, image] }));
    },

    removePendingImage: (index) => {
      set((state) => ({
        pendingImages: state.pendingImages.filter((_, i) => i !== index),
      }));
    },

    clearPendingImages: () => {
      set({ pendingImages: [] });
    },

    toggleToolOutputsExpanded: () => {
      set((state) => ({ toolOutputsExpanded: !state.toolOutputsExpanded }));
    },

    setHasExpandableToolOutputs: (has: boolean) => {
      set({ hasExpandableToolOutputs: has });
    },

    // Main orchestrator
    handleUserInput: async (input: string) => {
      const trimmed = input.trim();
      const hasPendingImages = get().pendingImages.length > 0;
      if (!trimmed && !hasPendingImages) return;

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
        showHelpPanel: false,
        showPromptsPanel: false,
        showIssuePanel: false,
        showUsagePanel: false,
        commandInputValue: '',
        activeTrigger: null,
        promptHint: null,
      });
      state.clearInput();

      // Handle slash commands via command registry
      if (trimmed.startsWith('/')) {
        CommandHistory.getInstance().add(trimmed);
        const ctx: CommandContext = {
          kiro: state.kiro,
          slashCommands: state.slashCommands,
          prompts: state.prompts,
          showAlert: (message, status, autoHideMs = 3000) =>
            state.showTransientAlert({ message, status, autoHideMs }),
          setLoadingMessage: state.setLoadingMessage,
          setActiveCommand: state.setActiveCommand,
          setCurrentModel: state.setCurrentModel,
          setCurrentAgent: state.setCurrentAgent,
          setContextUsage: state.setContextUsage,
          setShowContextBreakdown: state.setShowContextBreakdown,
          setShowHelpPanel: state.setShowHelpPanel,
          setShowPromptsPanel: state.setShowPromptsPanel,
          setShowIssuePanel: state.setShowIssuePanel,
          setShowUsagePanel: state.setShowUsagePanel,
          setShowMcpPanel: state.setShowMcpPanel,
          setShowToolsPanel: state.setShowToolsPanel,
          setShowKnowledgePanel: state.setShowKnowledgePanel,
          clearMessages: state.clearMessages,
          sendMessage: state.sendMessage,
          clearUIState: () =>
            set({
              activeCommand: null,
              showContextBreakdown: false,
              showHelpPanel: false,
              showPromptsPanel: false,
              showIssuePanel: false,
              showUsagePanel: false,
              showMcpPanel: false,
              showToolsPanel: false,
              showKnowledgePanel: false,
              contextBreakdown: null,
              usageData: null,
            }),
        };
        await executeCommand(trimmed, ctx);
        return;
      }

      // Handle shell escape commands
      if (trimmed.startsWith('!')) {
        const command = trimmed.slice(1).trim();
        if (!command) return;

        const {
          needsTTY,
          isClearCommand,
          executeClearCommand,
          executeShellEscapeTTY,
          executeShellEscapeStreaming,
        } = await import('../utils/shell-escape.js');

        // Clear/reset: clear messages and terminal, no user message needed
        if (isClearCommand(command)) {
          set({ messages: [] });
          executeClearCommand();
          return;
        }

        // Add user message showing the command
        const userMsgId = generateMessageId();
        set((state) => ({
          messages: [
            ...state.messages,
            {
              id: userMsgId,
              role: MessageRole.User,
              content: chalk.hex('#C19AFF')('!') + command,
              agentName: state.currentAgent?.name,
            },
          ],
        }));

        if (needsTTY(command)) {
          // TTY mode: direct terminal access
          const result = executeShellEscapeTTY(command);
          if (result.exitCode !== 0) {
            const msg = result.error || `Exited with status ${result.exitCode}`;
            set((state) => ({
              messages: [
                ...state.messages,
                {
                  id: generateMessageId(),
                  role: MessageRole.System,
                  content: msg,
                  success: false,
                },
              ],
            }));
          }
        } else {
          // Streaming mode: pipe output into conversation
          const outputMsgId = generateMessageId();
          let accumulated = '';

          // Add initial empty model message and set processing
          set((state) => ({
            isProcessing: true,
            messages: [
              ...state.messages,
              {
                id: outputMsgId,
                role: MessageRole.Model,
                content: '',
                agentName: state.currentAgent?.name,
                shellOutput: true,
              },
            ],
          }));

          const { promise, kill } = executeShellEscapeStreaming(
            command,
            (chunk) => {
              accumulated += chunk;
              // Update the model message with accumulated output
              set((state) => ({
                messages: state.messages.map((msg) =>
                  msg.id === outputMsgId
                    ? { ...msg, content: accumulated }
                    : msg
                ),
              }));
            }
          );

          // Store kill function for Ctrl+C cancellation
          const abortController = new AbortController();
          const origKill = kill;
          abortController.signal.addEventListener('abort', () => origKill());
          set({ currentAbortController: abortController });

          const result = await promise;

          // Finalize
          const finalContent = accumulated || '(no output)';
          const exitSuffix =
            result.exitCode !== 0 ? `\n\n[exit code: ${result.exitCode}]` : '';
          set((state) => ({
            isProcessing: false,
            currentAbortController: null,
            messages: state.messages.map((msg) =>
              msg.id === outputMsgId
                ? { ...msg, content: finalContent + exitSuffix }
                : msg
            ),
          }));
        }
        return;
      }

      // Handle regular prompts
      await state.sendMessage(trimmed);
    },
  }));

  // Track the last progress state we wrote to the terminal so we only
  // emit an OSC 9;4 escape when the derived indicator actually changes.
  let lastProgressKey: string | null = null;

  store.subscribe((state) => {
    // Derive a cache key from the fields that affect the progress indicator
    const key = `${state.agentError ?? ''}|${state.pendingApproval != null}|${state.isProcessing}|${state.isCompacting}|${state.contextUsagePercent}`;
    if (key !== lastProgressKey) {
      lastProgressKey = key;
      syncTerminalProgress(state);
    }
  });

  return store;
};

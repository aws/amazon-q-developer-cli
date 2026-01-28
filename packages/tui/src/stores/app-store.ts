import { createStore, useStore } from 'zustand';
import { Kiro } from '../kiro';
import { type Theme, defaultTheme, noColorTheme } from '../types/theme';
import { createContext, useContext } from 'react';
import { AgentEventType, ApprovalOptionId, type AgentStreamEvent, type ApprovalRequestInfo } from '../types/agent-events';
import type {
  InputBufferState,
  InputBufferActions,
  MoveCursorDir,
} from '../types/input-buffer';
import type { AvailableCommand, CommandOption } from '../types/commands';
import type { StatusType } from '../types/componentTypes';

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
  | { id: string; role: MessageRole.User; content: string }
  | { id: string; role: MessageRole.Model; content: string }
  | { id: string; role: MessageRole.ToolUse; name: string; content: string; isFinished?: boolean; status?: ToolUseStatus; result?: ToolResult }
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
  processMessageStream: (
    stream: AsyncGenerator<AgentStreamEvent>
  ) => Promise<void>;
  cancelMessage: () => Promise<void>;
  setProcessing: (processing: boolean) => void;
  setAgentError: (error: string | null) => void;
  respondToApproval: (optionId: string) => void;
  cancelApproval: () => void;
  setCurrentModel: (model: { id: string; name: string } | null) => void;

  // Chat actions
  clearMessages: () => void;
  queueMessage: (content: string) => void;
  processQueue: () => Promise<void>;
  setSlashCommands: (commands: SlashCommand[]) => void;

  // Command UI actions
  setActiveCommand: (command: ActiveCommand | null) => void;
  executeCommandWithArg: (arg: string) => Promise<void>;
  setCommandInput: (value: string) => void;
  setActiveTrigger: (trigger: { key: string; position: number; type: 'start' | 'inline' } | null) => void;
  clearCommandInput: () => void;

  navigateHistory: (direction: 'up' | 'down') => void;

  // UI actions
  setMode: (mode: 'inline' | 'expanded') => void;
  incrementExitSequence: () => void;
  resetExitSequence: () => void;
  showTransientAlert: (alert: TransientAlert) => void;
  dismissTransientAlert: () => void;
  toggleToolOutputsExpanded: () => void;
  setHasExpandableToolOutputs: (has: boolean) => void;

  // Context usage actions
  setContextUsage: (percent: number) => void;
  setLastTurnTokens: (tokens: LastTurnTokens) => void;
  toggleContextBreakdown: () => void;
  setShowContextBreakdown: (show: boolean) => void;

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
  agentError: string | null;
  pendingApproval: ApprovalRequestInfo | null;
  currentModel: { id: string; name: string } | null;

  // Command UI state
  activeCommand: ActiveCommand | null;
  commandInputValue: string;
  activeTrigger: { key: string; position: number; type: 'start' | 'inline' } | null;

  // Input state
  input: InputBufferState;

  // Input history
  inputHistory: string[];
  historyIndex: number;

  // UI state
  mode: 'inline' | 'expanded';
  exitSequence: number;
  exitTimer: NodeJS.Timeout | null;
  transientAlert: TransientAlert | null;
  toolOutputsExpanded: boolean; // Global toggle for all tool outputs
  hasExpandableToolOutputs: boolean; // Whether there are any tool outputs that can be expanded

  // Context usage state
  contextUsagePercent: number | null;
  lastTurnTokens: LastTurnTokens | null;
  showContextBreakdown: boolean;

  // Theme state
  theme: Theme;
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
    slashCommands: [
      { name: '/exit', description: 'Exit the application', source: 'local' },
      { name: '/clear', description: 'Clear chat history', source: 'local' },
    ],
    kiro: props.kiro,
    sessionId: null,
    isProcessing: false,
    agentError: null,
    pendingApproval: null,
    currentModel: null,

    activeCommand: null,
    commandInputValue: '',
    activeTrigger: null,

    input: initialInputBufferState(),

    inputHistory: [],
    historyIndex: -1,

    mode: 'inline',

    exitSequence: 0,
    exitTimer: null,
    transientAlert: null,
    toolOutputsExpanded: false,
    hasExpandableToolOutputs: false,

    contextUsagePercent: null,
    lastTurnTokens: null,
    showContextBreakdown: false,

    theme: getInitialTheme(),

    sendMessage: async (content: string) => {
      const { kiro, isProcessing } = get();
      if (isProcessing) return;

      const abortController = new AbortController();
      const userMessageId = generateMessageId();
      const userMessage: MessageType = {
        id: userMessageId,
        role: MessageRole.User,
        content,
      };

      set((state) => ({
        isProcessing: true,
        agentError: null,
        messages: [...state.messages, userMessage],
      }));

      try {
        const stream = kiro.sendMessageStream(content, abortController.signal);
        await get().processMessageStream(stream);

        // Mark any remaining tool calls as finished and mark turn as complete
        set((state) => {
          const messages = state.messages.map((msg) => {
            if (msg.role === MessageRole.ToolUse && !msg.isFinished) {
              return { ...msg, isFinished: true };
            }
            return msg;
          });
          return {
            messages,
            isProcessing: false,
          };
        });
      } catch (error) {
        if (error instanceof DOMException && error.name === 'AbortError')
          return;
        set({
          agentError: error instanceof Error ? error.message : 'Unknown error',
          isProcessing: false,
        });
      }
    },

    processMessageStream: async (stream: AsyncGenerator<AgentStreamEvent>) => {
      for await (const event of stream) {
        switch (event.type) {
          case AgentEventType.Content:
            if (event.content.type === 'text') {
              const text = event.content.text;
              set((state) => {
                // Mark any pending tool calls as finished when we get content
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
                        content: lastMsg.content + text,
                      },
                    ],
                  };
                } else {
                  return {
                    messages: [
                      ...messages,
                      { id: event.id, role: MessageRole.Model, content: text },
                    ],
                  };
                }
              });
            }
            break;
          case AgentEventType.ToolCall:
            set((state) => {
              // Check if this tool call already exists (avoid duplicates)
              const existingToolCall = state.messages.find(
                (msg) => msg.role === MessageRole.ToolUse && msg.id === event.id
              );
              if (existingToolCall) {
                return state;
              }
              
              // Mark any pending tool calls as finished when a new one starts
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
                    content: JSON.stringify(event.args),
                  },
                ],
              };
            });
            break;
          case AgentEventType.ToolCallUpdate:
            if (event.content.type === 'text') {
              const text = event.content.text;
              set((state) => {
                const lastMsg = state.messages[state.messages.length - 1];
                if (lastMsg?.role === MessageRole.ToolUse) {
                  return {
                    messages: [
                      ...state.messages.slice(0, -1),
                      {
                        id: lastMsg.id,
                        name: lastMsg.name,
                        role: MessageRole.ToolUse,
                        content: lastMsg.content + text,
                      },
                    ],
                  };
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
                    content: toolMsg.content,
                    isFinished: true,
                    result: event.result,
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
            if (event.inputTokens !== undefined || event.outputTokens !== undefined) {
              get().setLastTurnTokens({
                input: event.inputTokens ?? 0,
                output: event.outputTokens ?? 0,
                cached: event.cachedTokens ?? 0,
              });
            }
            break;
        }
      }
    },

    cancelMessage: async () => {
      const { kiro } = get();
      if (!kiro) return;

      try {
        await kiro.cancel();
        set({ isProcessing: false });
      } catch (error) {
        set({
          agentError: error instanceof Error ? error.message : 'Cancel failed',
        });
      }
    },

    setProcessing: (isProcessing) => set({ isProcessing }),
    setAgentError: (agentError) => set({ agentError }),
    setCurrentModel: (currentModel) => set({ currentModel }),

    respondToApproval: (optionId: string) => {
      const { pendingApproval } = get();
      if (pendingApproval) {
        const toolCallId = pendingApproval.toolCall.toolCallId;
        const isRejected = optionId === ApprovalOptionId.RejectOnce || optionId === ApprovalOptionId.RejectAlways;
        
        // Update the tool call status based on user response
        set((state) => ({
          messages: state.messages.map((msg) => {
            if (msg.role === MessageRole.ToolUse && msg.id === toolCallId) {
              return { 
                ...msg, 
                status: isRejected ? ToolUseStatus.Rejected : ToolUseStatus.Approved, 
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
        pendingApproval.resolve({ outcome: 'cancelled' });
        set({ pendingApproval: null });
      }
    },

    // Chat actions
    clearMessages: () => set({ messages: [] }),

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

    clearCommandInput: () => {
      set({ commandInputValue: '', activeTrigger: null });
    },

    executeCommandWithArg: async (arg: string) => {
      const { activeCommand, kiro } = get();
      if (!activeCommand) return;

      const cmdName = activeCommand.command.name.replace(/^\//, '');
      set({ activeCommand: null });

      // Send generic { command, args: { value } } - backend handles mapping
      const result = await kiro.executeCommand({
        command: cmdName,
        args: { value: arg },
      } as import('../types/commands').TuiCommand);

      get().showTransientAlert({
        message: result.message,
        status: result.success ? 'success' : 'error',
        autoHideMs: 3000,
      });
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
      set((state) => {
        // todo
        return state;
      });
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

      // Add to history
      set((prevState) => ({
        inputHistory: [...prevState.inputHistory.slice(-49), trimmed],
      }));

      // Queue if processing
      if (state.isProcessing) {
        state.queueMessage(trimmed);
        state.clearInput();
        return;
      }

      state.clearInput();

      // Handle slash commands
      if (trimmed.startsWith('/')) {
        // Local commands
        if (trimmed === '/exit') {
          process.exit(0);
        } else if (trimmed === '/clear') {
          state.clearMessages();
          return;
        }

        // Check for backend commands with selection input
        const cmdName = trimmed.split(' ')[0] ?? '';
        const cmd = state.slashCommands.find((c) => c.name === cmdName);
        
        // Show selection UI for commands with inputType: 'selection'
        if (cmd?.meta?.inputType === 'selection' && !trimmed.includes(' ')) {
          // Fetch options from backend and show selection UI
          try {
            const response = await state.kiro.getCommandOptions(cmdName, '');
            if (response.options.length > 0) {
              state.setActiveCommand({ command: cmd, options: response.options });
              return;
            }
          } catch {
            // Fall through to execute directly if options fetch fails
          }
        }

        // Panel commands retain input without showing options menu
        if (cmd?.meta?.inputType === 'panel' && !trimmed.includes(' ')) {
          state.setCommandInput(trimmed);
          state.setActiveCommand({ command: cmd, options: [] });
          // Show context breakdown for /context command
          if (cmdName === '/context') {
            set({ showContextBreakdown: true });
          }
          // Execute command but don't show transient alert for panel commands
          if (cmd?.source === 'backend') {
            const cmdNameOnly = cmdName.replace(/^\//, '');
            try {
              await state.kiro.executeCommand({
                command: cmdNameOnly,
                args: {},
              } as import('../types/commands').TuiCommand);
            } catch {
              // Silently ignore errors for panel commands
            }
          }
          return;
        }

        // For other backend commands, execute directly
        if (cmd?.source === 'backend') {
          const cmdNameOnly = cmdName.replace(/^\//, '');
          const args = trimmed.slice(cmdName.length).trim();
          
          try {
            const result = await state.kiro.executeCommand({
              command: cmdNameOnly,
              args: args ? { value: args } : {},
            } as import('../types/commands').TuiCommand);
            
            state.showTransientAlert({
              message: result.message,
              status: result.success ? 'success' : 'error',
              autoHideMs: 3000,
            });
          } catch {
            state.showTransientAlert({
              message: `Command ${cmdName} failed - backend unavailable`,
              status: 'error',
              autoHideMs: 3000,
            });
          }
          return;
        }

        return;
      }

      // Handle regular prompts
      await state.sendMessage(trimmed);
    },
  }));

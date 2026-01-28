import { createContext, useContext } from 'react';
import { type ContentBlock } from '../utils/message-parser';

export interface UIActions {
  // Chat actions
  addUserMessage: (content: string) => void;
  setCurrentMessage: (message: any) => void;
  appendToCurrentMessage: (blocks: ContentBlock[]) => void;
  finalizeCurrentMessage: () => void;
  clearMessages: () => void;
  setSlashCommands: (commands: any[]) => void;

  // Agent actions
  setConnected: (connected: boolean) => void;
  setProcessing: (processing: boolean) => void;
  setError: (error: string | null) => void;
  submitPrompt: (content: string) => Promise<void>;
  cancel: () => Promise<void>;

  // Input actions
  setInputValue: (value: string) => void;
  setCursorPosition: (position: number) => void;
  insertText: (text: string) => void;
  deleteText: (start: number, end: number) => void;
  clearInput: () => void;
  clearWord: () => void;
  clearLine: () => void;
  moveCursor: (delta: number) => void;
  insertNewline: () => void;
  setShowSlashCommands: (show: boolean) => void;

  // UI actions
  setMode: (mode: 'inline' | 'expanded') => void;
  incrementExitSequence: () => void;
  resetExitSequence: () => void;
}

export const UIActionsContext = createContext<UIActions | null>(null);

export const useUIActions = () => {
  const context = useContext(UIActionsContext);
  if (!context) {
    throw new Error('useUIActions must be used within a UIActionsProvider');
  }
  return context;
};

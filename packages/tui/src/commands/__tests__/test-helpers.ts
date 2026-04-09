/**
 * Shared test helpers for command tests.
 *
 * Centralizes the mock CommandContext factory so new fields only need
 * to be added in one place.
 */

import { mock } from 'bun:test';
import type { CommandContext } from '../types.js';

export type MockCommandContext = CommandContext & {
  _spies: Record<string, ReturnType<typeof mock>>;
};

export interface CreateMockCtxOptions {
  /** Messages returned by getMessages(). Default: [] */
  messages?: Array<{ id: string; role: string; content: string }>;
  /** Slash commands available in context. Default: [] */
  slashCommands?: CommandContext['slashCommands'];
  /** Override the kiro client mock. Default: bare {} */
  kiro?: Partial<CommandContext['kiro']>;
}

/**
 * Create a mock CommandContext with spies on all methods.
 *
 * Every method is a tracked spy accessible via `ctx._spies[name]`.
 */
export function createMockCommandContext(
  opts: CreateMockCtxOptions = {}
): MockCommandContext {
  const spies: Record<string, ReturnType<typeof mock>> = {};
  const spy = (name: string) => {
    const fn = mock(() => {});
    spies[name] = fn;
    return fn;
  };

  const defaultKiro = {
    executeCommand: mock(() =>
      Promise.resolve({ success: true, message: '', data: undefined })
    ),
    getCommandOptions: mock(() => Promise.resolve({ options: [] })),
  };

  return {
    kiro: { ...defaultKiro, ...opts.kiro } as any,
    slashCommands: opts.slashCommands ?? [],
    showAlert: spy('showAlert') as any,
    setLoadingMessage: spy('setLoadingMessage') as any,
    setActiveCommand: spy('setActiveCommand') as any,
    setCurrentModel: spy('setCurrentModel') as any,
    setCurrentAgent: spy('setCurrentAgent') as any,
    setContextUsage: spy('setContextUsage') as any,
    setShowContextBreakdown: spy('setShowContextBreakdown') as any,
    setShowHelpPanel: spy('setShowHelpPanel') as any,
    setShowTuiPanel: spy('setShowTuiPanel') as any,
    setShowUsagePanel: spy('setShowUsagePanel') as any,
    setShowMcpPanel: spy('setShowMcpPanel') as any,
    setShowToolsPanel: spy('setShowToolsPanel') as any,
    setShowHooksPanel: spy('setShowHooksPanel') as any,
    setShowKnowledgePanel: spy('setShowKnowledgePanel') as any,
    setShowCodePanel: spy('setShowCodePanel') as any,
    clearMessages: spy('clearMessages') as any,
    resetMessages: spy('resetMessages') as any,
    sendMessage: spy('sendMessage') as any,
    clearUIState: spy('clearUIState') as any,
    createStreamEventHandler: spy('createStreamEventHandler') as any,
    setSessionId: spy('setSessionId') as any,
    addSystemMessage: spy('addSystemMessage') as any,
    addSession: spy('addSession') as any,
    setActiveSession: spy('setActiveSession') as any,
    sessions: new Map(),
    setMode: spy('setMode') as any,
    getMessages: mock(() => opts.messages ?? []) as any,
    setUserColors: spy('setUserColors') as any,
    setThemePreview: spy('setThemePreview') as any,
    getThemeDiffHex: mock(() => ({
      added: { background: '', bar: '', highlight: '' },
      removed: { background: '', bar: '', highlight: '' },
    })) as any,
    getAutoPreview: mock(() => '') as any,
    _spies: spies,
  };
}

/**
 * Shared test helpers for command tests.
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
  /** KAS-side commands. Default: [] */
  kasCommands?: CommandContext['kasCommands'];
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
    agentEngine: 'rust',
    slashCommands: opts.slashCommands ?? [],
    kasCommands: opts.kasCommands ?? [],
    showAlert: spy('showAlert') as any,
    setLoadingMessage: spy('setLoadingMessage') as any,
    setActiveCommand: spy('setActiveCommand') as any,
    setCurrentModel: spy('setCurrentModel') as any,
    setCurrentAgent: spy('setCurrentAgent') as any,
    setContextUsage: spy('setContextUsage') as any,
    setShowContextBreakdown: spy('setShowContextBreakdown') as any,
    setShowHelpPanel: spy('setShowHelpPanel') as any,
    setShowTuiPanel: spy('setShowTuiPanel') as any,
    setShowChangelogPanel: spy('setShowChangelogPanel') as any,
    setShowUsagePanel: spy('setShowUsagePanel') as any,
    setShowRewindExplorer: spy('setShowRewindExplorer') as any,
    setShowMcpPanel: spy('setShowMcpPanel') as any,
    setShowToolsPanel: spy('setShowToolsPanel') as any,
    setShowStatsPanel: spy('setShowStatsPanel') as any,
    setShowHooksPanel: spy('setShowHooksPanel') as any,
    setShowKeybindingsPanel: spy('setShowKeybindingsPanel') as any,
    setShowDisplaySettingsPanel: spy('setShowDisplaySettingsPanel') as any,
    setSettingsReturnOnEscape: spy('setSettingsReturnOnEscape') as any,
    setShowKnowledgePanel: spy('setShowKnowledgePanel') as any,
    setShowCodePanel: spy('setShowCodePanel') as any,
    openArtifactView: spy('openArtifactView') as any,
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
    setBaseTheme: spy('setBaseTheme') as any,
    setThemePreview: spy('setThemePreview') as any,
    getThemeDiffHex: mock(() => ({
      added: {
        background: { named: 'default' as const },
        bar: { named: 'green' as const },
        highlight: { named: 'default' as const },
      },
      removed: {
        background: { named: 'default' as const },
        bar: { named: 'red' as const },
        highlight: { named: 'default' as const },
      },
    })) as any,
    getAutoPreview: mock(() => '') as any,
    setVoiceStop: spy('setVoiceStop') as any,
    setVoiceCancel: spy('setVoiceCancel') as any,
    setVoiceLevel: spy('setVoiceLevel') as any,
    setVoicePartialText: spy('setVoicePartialText') as any,
    voiceAutoSubmit: false,
    toggleVoiceAutoSubmit: spy('toggleVoiceAutoSubmit') as any,
    voiceHintIndex: 0,
    incrementVoiceHint: spy('incrementVoiceHint') as any,
    setPendingVoiceText: spy('setPendingVoiceText') as any,
    _spies: spies,
  };
}

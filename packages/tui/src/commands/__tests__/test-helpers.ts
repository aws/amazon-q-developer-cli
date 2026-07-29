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
  /** Prompts slice. Default: [] */
  prompts?: CommandContext['prompts'];
  /** Skills slice. Default: [] */
  skills?: CommandContext['skills'];
  /** Steering slice. Default: [] */
  steering?: CommandContext['steering'];
  /** KAS available-option slices. Default: [] */
  kasAvailableModels?: CommandContext['kasAvailableModels'];
  kasAvailableAgents?: CommandContext['kasAvailableAgents'];
  kasAvailableEfforts?: CommandContext['kasAvailableEfforts'];
  /** Override the kiro client mock. Default: bare {} */
  kiro?: Partial<CommandContext['kiro']>;
  /**
   * Initial value for `settingsReturnOnEscape`. Defaults to `false`. Set to
   * `true` to simulate flows that were launched via /settings and should
   * therefore return to the /settings picker on completion.
   */
  settingsReturnOnEscape?: boolean;
  /** Current-agent snapshot used by some effects. Default: null */
  currentAgent?: CommandContext['currentAgent'];
  /** Current model returned by getCurrentModel(). Default: null */
  currentModel?: { id: string; name: string } | null;
  /** Current effort returned by getCurrentEffort(). Default: null */
  currentEffort?: string | null;
  /** Cached session tool listing snapshot. Default: [] */
  toolsList?: CommandContext['toolsList'];
  /** Cached hook registry snapshot. Default: [] */
  hooksList?: CommandContext['hooksList'];
  /** Read the latest cached context breakdown. Default: null */
  getContextBreakdownCache?: CommandContext['getContextBreakdownCache'];
  /** Cached KAS configured-server snapshot. Default: [] */
  mcpServerCache?: CommandContext['mcpServerCache'];
  /** Cached KAS MCP registry snapshot. Default: [] */
  mcpRegistryCache?: CommandContext['mcpRegistryCache'];
  /** Whether the current session is a cloud session. Default: false */
  cloudSessionActive?: boolean;
  /** Locally observed workflow runs. Default: [] */
  localWorkflowRuns?: ReturnType<CommandContext['getLocalWorkflowRuns']>;
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
    isCloudSessionActive: mock(() => false),
    getSessionRepositories: mock(() => null),
    getCommandOptions: mock(() => Promise.resolve({ options: [] })),
    setSetting: mock(() => Promise.resolve()),
    setConfigOption: mock(() => Promise.resolve()),
    setSessionMode: mock(() => Promise.resolve()),
    sendModeChanged: mock(() => undefined),
    sendChatSlashCommandTelemetry: mock(() => undefined),
    sendUiModeSessionStart: mock(() => undefined),
    sendUiModeChanged: mock(() => undefined),
    sendUiModeDefaultChanged: mock(() => undefined),
  };

  return {
    kiro: { ...defaultKiro, ...opts.kiro } as any,
    agentEngine: 'v2',
    cloudSessionActive: opts.cloudSessionActive ?? false,
    slashCommands: opts.slashCommands ?? [],
    kasCommands: opts.kasCommands ?? [],
    prompts: opts.prompts ?? [],
    skills: opts.skills ?? [],
    steering: opts.steering ?? [],
    kasAvailableModels: opts.kasAvailableModels ?? [],
    kasAvailableAgents: opts.kasAvailableAgents ?? [],
    kasAvailableEfforts: opts.kasAvailableEfforts ?? [],
    showAlert: spy('showAlert') as any,
    setVoiceDownloadConfirm: spy('setVoiceDownloadConfirm') as any,
    announceSystem: spy('announceSystem') as any,
    announceWorkflowLifecycle: spy('announceWorkflowLifecycle') as any,
    setLoadingMessage: spy('setLoadingMessage') as any,
    setActiveCommand: spy('setActiveCommand') as any,
    setCurrentModel: spy('setCurrentModel') as any,
    beginKasSession: (() => {
      const fn = mock(() => () => {});
      spies['beginKasSession'] = fn;
      return fn;
    })() as any,
    getCurrentModel: (() => opts.currentModel ?? null) as any,
    setCurrentEffort: spy('setCurrentEffort') as any,
    getCurrentEffort: (() => opts.currentEffort ?? null) as any,
    setCurrentAgent: spy('setCurrentAgent') as any,
    getCurrentAgent: (() => opts.currentAgent ?? null) as any,
    currentAgent: opts.currentAgent ?? null,
    setContextUsage: spy('setContextUsage') as any,
    setShowContextBreakdown: spy('setShowContextBreakdown') as any,
    getContextBreakdownCache: opts.getContextBreakdownCache ?? (() => null),
    setShowHelpPanel: spy('setShowHelpPanel') as any,
    setShowTuiPanel: spy('setShowTuiPanel') as any,
    setShowChangelogPanel: spy('setShowChangelogPanel') as any,
    setShowUsagePanel: spy('setShowUsagePanel') as any,
    setShowRewindExplorer: spy('setShowRewindExplorer') as any,
    setShowTangentExplorer: spy('setShowTangentExplorer') as any,
    setTangentName: spy('setTangentName') as any,
    setShowWorkflowHistory: spy(
      'setShowWorkflowHistory'
    ) as CommandContext['setShowWorkflowHistory'],
    getLocalWorkflowRuns: () => opts.localWorkflowRuns ?? [],
    setUpgradeDiagnostics: spy('setUpgradeDiagnostics') as any,
    setUpgradeRunPreview: spy('setUpgradeRunPreview') as any,
    setShowMcpPanel: spy('setShowMcpPanel') as any,
    mcpServerCache: opts.mcpServerCache ?? [],
    mcpRegistryCache: opts.mcpRegistryCache ?? [],
    setShowToolsPanel: spy('setShowToolsPanel') as any,
    toolsList: opts.toolsList ?? [],
    setShowGoalPanel: spy('setShowGoalPanel') as any,
    setGoalStatus: spy('setGoalStatus') as any,
    setShowStatsPanel: spy('setShowStatsPanel') as any,
    setShowHooksPanel: spy('setShowHooksPanel') as any,
    hooksList: opts.hooksList ?? [],
    setShowRepoPicker: spy('setShowRepoPicker') as any,
    setShowSessionPicker: spy('setShowSessionPicker') as any,
    resetCloudSessionScope: spy('resetCloudSessionScope') as any,
    stashCloudSessionScope: spy('stashCloudSessionScope') as any,
    restoreCloudSessionScope: spy('restoreCloudSessionScope') as any,
    applyRepoFooter: spy('applyRepoFooter') as any,
    setCloudNewSessionChecklist: spy('setCloudNewSessionChecklist') as any,
    setCloudSessionActive: spy('setCloudSessionActive') as any,
    setShowKeybindingsPanel: spy('setShowKeybindingsPanel') as any,
    setShowDisplaySettingsPanel: spy('setShowDisplaySettingsPanel') as any,
    setShowThemePanel: spy('setShowThemePanel') as any,
    setShowCloudQuitPrompt: spy('setShowCloudQuitPrompt') as any,
    setShowSettingsPanel: spy('setShowSettingsPanel') as any,
    setSettingsReturnOnEscape: spy('setSettingsReturnOnEscape') as any,
    setVerboseReturnOnEscape: spy('setVerboseReturnOnEscape') as any,
    setThemeReturnOnEscape: spy('setThemeReturnOnEscape') as any,
    setActiveInterruptMode: spy('setActiveInterruptMode') as any,
    settingsReturnOnEscape: opts.settingsReturnOnEscape ?? false,
    reopenSettingsMenu: spy('reopenSettingsMenu') as any,
    setShowKnowledgePanel: spy('setShowKnowledgePanel') as any,
    setShowCodePanel: spy('setShowCodePanel') as any,
    openArtifactView: spy('openArtifactView') as any,
    clearMessages: spy('clearMessages') as any,
    resetMessages: spy('resetMessages') as any,
    bumpLiteScrollbackClear: spy('bumpLiteScrollbackClear') as any,
    sendMessage: spy('sendMessage') as any,
    clearUIState: spy('clearUIState') as any,
    resetClientDisplayCaches: (() => {
      const fn = mock(() => ({
        contextBreakdownCache: null,
        mcpServerCache: [],
        mcpRegistryCache: [],
        toolsList: [],
        hooksList: [],
        cloudSnapshotReadiness: {
          mcp: 'awaiting-sandbox' as const,
          tools: 'awaiting-sandbox' as const,
        },
      }));
      spies['resetClientDisplayCaches'] = fn;
      return fn;
    })() as any,
    restoreClientDisplayCaches: spy('restoreClientDisplayCaches') as any,
    stashDisplaySnapshot: spy('stashDisplaySnapshot') as any,
    restoreDisplaySnapshotFor: spy('restoreDisplaySnapshotFor') as any,
    createStreamEventHandler: spy('createStreamEventHandler') as any,
    setSessionId: spy('setSessionId') as any,
    addSystemMessage: spy('addSystemMessage') as any,
    setPendingSpecDescription: spy('setPendingSpecDescription') as any,
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
    setUiMode: spy('setUiMode') as any,
    getUiMode: mock(() => 'tui') as any,
    processQueue: mock(() => Promise.resolve()) as any,

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

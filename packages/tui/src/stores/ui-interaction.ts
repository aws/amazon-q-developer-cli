import type { AppState } from './app-store.js';

export function hasOpenBackendPanel(state: AppState): boolean {
  return (
    state.showContextBreakdown ||
    state.showHelpPanel ||
    state.showUsagePanel ||
    state.showMcpPanel ||
    state.showToolsPanel ||
    state.showGoalPanel ||
    state.showTuiPanel ||
    state.showStatsPanel ||
    state.showHooksPanel ||
    state.showRepoPicker ||
    state.showKnowledgePanel ||
    state.showCodePanel ||
    state.showChangelogPanel ||
    state.showMemoriesPanel ||
    state.showRewindExplorer ||
    state.showTangentExplorer ||
    state.showKeybindingsPanel ||
    state.showDisplaySettingsPanel ||
    state.showStatusLinePanel ||
    state.showThemePanel ||
    state.showSettingsPanel ||
    state.artifactViewOpen != null ||
    state.showSurveyPanel ||
    state.showSessionPicker ||
    state.showCloudQuitPrompt
  );
}

export function hasBlockingCommandInteraction(state: AppState): boolean {
  return state.activeCommand != null || hasOpenBackendPanel(state);
}

/**
 * Backend-driven panel cluster shared by InlineLayout and LiteLayout. Reads
 * show-flag + data state from the store directly (no 30+ prop-drill); each
 * layout supplies its own wrapper and the shared useBackendPanelHandlers.
 */
import React, { useCallback, useMemo } from 'react';
import { useStore } from 'zustand';
import { ContextBreakdown } from '../../ui/ContextBreakdown.js';
import { HelpPanel } from '../../ui/HelpPanel.js';
import { TuiPanel } from '../../ui/TuiPanel.js';
import { McpPanel } from '../../ui/McpPanel.js';
import { ToolsPanel } from '../../ui/ToolsPanel.js';
import { GoalPanel } from '../../ui/GoalPanel.js';
import { StatsPanel } from '../../ui/StatsPanel.js';
import { HooksPanel } from '../../ui/HooksPanel.js';
import { RepoPickerPanel } from '../../ui/RepoPickerPanel.js';
import { SessionPickerPanel } from '../../ui/SessionPickerPanel.js';
import { KnowledgePanel } from '../../ui/KnowledgePanel.js';
import { CodePanel } from '../../ui/CodePanel.js';
import { UsagePanel } from '../../ui/UsagePanel.js';
import { ChangelogPanel } from '../../ui/ChangelogPanel.js';
import { Explorer } from '../../ui/Explorer.js';
import { KeybindingsPanel } from '../../ui/KeybindingsPanel.js';
import { DisplaySettingsPanel } from '../../ui/DisplaySettingsPanel.js';
import { ThemePanel } from '../../ui/ThemePanel.js';
import { StatusLineSettingsPanel } from '../../ui/StatusLineSettingsPanel.js';
import { SettingsPanel } from '../../ui/SettingsPanel.js';
import { ArtifactView } from '../../ui/ArtifactView/index.js';
import { SurveyPanel } from '../../ui/SurveyPanel.js';
import { CloudQuitPrompt } from '../../ui/CloudQuitPrompt.js';
import {
  quitCloudSessionKeepRunning,
  quitCloudSessionTurnOff,
} from '../../../utils/cloud-detach-notice.js';
import {
  useUIState,
  useUIActions,
  useNotificationState,
  useNotificationActions,
  useContextState,
  useKiroClient,
} from '../../../stores/selectors.js';
import { useAppStore } from '../../../stores/app-store.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { startMcpOAuth } from '../../../utils/mcp-oauth.js';
import { copyToSystemClipboard } from '../../../commands/effects.js';
import { engineSupportsMcpCommandActions } from '../../../agent-engine.js';
import type { BackendPanelHandlers } from './useBackendPanelHandlers.js';
import { runMcpPanelAction } from './mcp-panel-actions.js';
import { workflowStore } from '../../../stores/workflow-store.js';
import { WorkflowHistoryPanel } from '../workflow-monitor/WorkflowHistoryPanel.js';
import type { UiMode } from '../../../types/ui-mode.js';

interface BackendPanelsProps {
  handlers: BackendPanelHandlers;
  /** The surface the mounting layout paints, for panels that edit per-surface state. */
  surface: UiMode;
}

export const BackendPanels: React.FC<BackendPanelsProps> = ({
  handlers,
  surface,
}) => {
  const glyphs = useGlyphs();
  const {
    showContextBreakdown,
    contextBreakdown,
    showHelpPanel,
    helpCommands,
    showUsagePanel,
    usageData,
    showRewindExplorer,
    rewindRows,
    showTangentExplorer,
    tangentRows,
    showMcpPanel,
    mcpServers,
    mcpRegistryServers,
    mcpMode,
    showToolsPanel,
    showGoalPanel,
    showTuiPanel,
    toolsList,
    showStatsPanel,
    statsList,
    statsSummary,
    showHooksPanel,
    hooksList,
    showRepoPicker,
    repoPickerResources,
    attachedRepos,
    showSessionPicker,
    sessionPickerRows,
    sessionPickerTitle,
    showKeybindingsPanel,
    showDisplaySettingsPanel,
    showThemePanel,
    showStatusLinePanel,
    showSettingsPanel,
    showKnowledgePanel,
    knowledgeEntries,
    knowledgeStatus,
    showCodePanel,
    codeData,
    showChangelogPanel,
    artifactViewOpen,
    showCloudQuitPrompt,
  } = useUIState();
  const { setShowMcpPanel, setShowCloudQuitPrompt, submitRepoPicker } =
    useUIActions();
  const { initErrors, pendingOAuthServers } = useNotificationState();
  const { showTransientAlert } = useNotificationActions();
  const { contextUsagePercent, currentModel, currentAgent } = useContextState();
  const { kiro } = useKiroClient();
  const agentEngine = useAppStore((s) => s.agentEngine);
  const cloudSessionActive = useAppStore((s) => s.cloudSessionActive);
  const cloudSnapshotReadiness = useAppStore((s) => s.cloudSnapshotReadiness);
  const supportsMcpCommandActions =
    engineSupportsMcpCommandActions(agentEngine);
  const showSurveyPanel = useAppStore((s) => s.showSurveyPanel);
  const closeSurveyPanel = useAppStore((s) => s.closeSurveyPanel);
  const submitSurvey = useAppStore((s) => s.submitSurvey);
  const workflowHistoryOpen = useStore(
    workflowStore,
    (state) => state.history.isOpen
  );

  // Overlay auth-required status onto MCP servers pending OAuth or with a forced
  // (re-)authentication in progress — same shaping both layouts had locally.
  const mcpServersWithAuth = useMemo(() => {
    if (
      pendingOAuthServers.size === 0 &&
      !mcpServers.some((s) => s.authenticating)
    )
      return mcpServers;
    return mcpServers.map((s) =>
      pendingOAuthServers.has(s.name) || s.authenticating
        ? { ...s, status: 'auth-required' as const }
        : s
    );
  }, [mcpServers, pendingOAuthServers]);

  const startMcpServerOAuth = useCallback(
    (serverName: string) => {
      startMcpOAuth({
        agentEngine,
        serverName,
        url: pendingOAuthServers.get(serverName) ?? null,
        resetMcpServer: (name, startOAuth) =>
          kiro.resetMcpServer(name, startOAuth),
        copyToClipboard: copyToSystemClipboard,
        showAlert: (message, status, autoHideMs) =>
          showTransientAlert({ message, status, autoHideMs }),
      });
    },
    [agentEngine, kiro, pendingOAuthServers, showTransientAlert]
  );

  // V2 owns the command-backed mutation surface. KAS exposes only its
  // supported reset-server OAuth operation.
  const runMcpServerAction = useCallback(
    (value: string) =>
      runMcpPanelAction({
        kiro,
        value,
        refreshValue: '',
        panelMode: 'status',
        setShowMcpPanel,
      }),
    [kiro, setShowMcpPanel]
  );

  return (
    <>
      {showContextBreakdown && (
        <ContextBreakdown
          percent={contextUsagePercent}
          breakdown={contextBreakdown ?? undefined}
          model={currentModel?.name ?? null}
          agentName={currentAgent?.name ?? null}
          initialExpanded={contextBreakdown?.initialExpanded}
          onClose={handlers.handleCloseContextBreakdown}
          onTabSwitch={handlers.handleTabFromContext}
        />
      )}
      {showUsagePanel && (
        <UsagePanel
          data={usageData}
          onClose={handlers.handleCloseUsagePanel}
          onTabSwitch={handlers.handleTabFromUsage}
        />
      )}
      {showRewindExplorer && (
        <Explorer
          title="/rewind"
          description="Fork from a previous prompt in this session"
          columns={[
            { key: 'label', label: 'User Prompt' },
            { key: 'group', label: 'Context used', align: 'right' },
          ]}
          rows={rewindRows.map((turn) => ({
            id: String(turn.logIndex),
            values: {
              label: turn.label,
              group: turn.group ?? '',
            },
            preview: turn.responseSnippet
              ? { body: turn.responseSnippet }
              : undefined,
          }))}
          previewHeading="Response Snippet"
          keyHints={[
            { key: `${glyphs.arrowUp}${glyphs.arrowDown}`, label: 'navigate' },
            { key: 'Enter', label: 'to fork' },
          ]}
          onSelect={(row) => handlers.handleRewindSelect(row.id)}
          onClose={handlers.handleCloseRewindExplorer}
        />
      )}
      {showTangentExplorer && (
        <Explorer
          title="/tangent ls"
          description="Switch to a tangent"
          columns={[
            { key: 'label', label: 'Tangent', align: 'left' },
            { key: 'lastActive', label: 'Last active', align: 'right' },
          ]}
          rows={tangentRows.map((row) => ({
            id: row.id,
            values: { label: row.label, lastActive: row.lastActive ?? '' },
            tag: row.isCurrent ? '[current]' : undefined,
          }))}
          initialSelectedIndex={Math.max(
            0,
            tangentRows.findIndex((row) => row.isCurrent)
          )}
          keyHints={[
            { key: `${glyphs.arrowUp}${glyphs.arrowDown}`, label: 'navigate' },
            { key: 'Enter', label: 'to switch' },
          ]}
          onSelect={(row) => {
            // Every row carries its sessionId as `id`; switch to that exact
            // session (the handler no-ops if it's the current one).
            const tangentRow = tangentRows.find((r) => r.id === row.id);
            handlers.handleTangentSelect(row.id, tangentRow?.title ?? row.id);
          }}
          onClose={handlers.handleCloseTangentExplorer}
        />
      )}
      {showHelpPanel && (
        <HelpPanel
          commands={helpCommands}
          onClose={handlers.handleCloseHelpPanel}
        />
      )}
      {showChangelogPanel && (
        <ChangelogPanel onClose={handlers.handleCloseChangelogPanel} />
      )}
      {showMcpPanel && (
        <McpPanel
          servers={mcpServersWithAuth}
          registryServers={mcpRegistryServers}
          initErrors={initErrors}
          pendingOAuthUrls={pendingOAuthServers}
          mode={mcpMode}
          cloudSessionActive={cloudSessionActive}
          cloudSnapshotReadiness={cloudSnapshotReadiness.mcp}
          onClose={handlers.handleCloseMcpPanel}
          onAuthenticate={startMcpServerOAuth}
          onForceAuth={
            supportsMcpCommandActions
              ? (serverName) => {
                  void runMcpServerAction(`auth ${serverName}`);
                }
              : startMcpServerOAuth
          }
          onAbortAuth={
            supportsMcpCommandActions
              ? (serverName) => {
                  void runMcpServerAction(`cancel-auth ${serverName}`);
                }
              : undefined
          }
          onRemoveCredentials={
            supportsMcpCommandActions
              ? (serverName) => {
                  void runMcpServerAction(`logout ${serverName}`);
                }
              : undefined
          }
          onAction={
            supportsMcpCommandActions
              ? async (serverNames: string[]) => {
                  const action = mcpMode === 'add' ? 'add' : 'remove';
                  await runMcpPanelAction({
                    kiro,
                    value: `${action} ${serverNames.join(',')}`,
                    refreshValue: action,
                    panelMode: action,
                    setShowMcpPanel,
                  });
                }
              : undefined
          }
        />
      )}
      {showToolsPanel && (
        <ToolsPanel
          tools={toolsList}
          initErrors={initErrors}
          cloudSessionActive={cloudSessionActive}
          cloudSnapshotReadiness={cloudSnapshotReadiness.tools}
          onClose={handlers.handleCloseToolsPanel}
        />
      )}
      {workflowHistoryOpen && (
        <WorkflowHistoryPanel onClose={handlers.handleCloseWorkflowHistory} />
      )}
      {showGoalPanel && <GoalPanel onClose={handlers.handleCloseGoalPanel} />}
      {showTuiPanel && <TuiPanel onClose={handlers.handleCloseTuiPanel} />}
      {showStatsPanel && (
        <StatsPanel
          stats={statsList}
          summary={statsSummary}
          onClose={handlers.handleCloseStatsPanel}
        />
      )}
      {showHooksPanel && (
        <HooksPanel
          hooks={hooksList}
          cloudSessionActive={cloudSessionActive}
          onClose={handlers.handleCloseHooksPanel}
        />
      )}
      {showRepoPicker && (
        <RepoPickerPanel
          resources={repoPickerResources}
          initialSelected={attachedRepos}
          onSubmit={(selected) => void submitRepoPicker(selected)}
          onClose={handlers.handleCloseRepoPicker}
        />
      )}
      {showSessionPicker && (
        <SessionPickerPanel
          rows={sessionPickerRows}
          title={sessionPickerTitle}
          onSelect={handlers.handleSessionSelect}
          onClose={handlers.handleCloseSessionPicker}
        />
      )}
      {showKeybindingsPanel && (
        <KeybindingsPanel onClose={handlers.handleCloseKeybindingsPanel} />
      )}
      {showDisplaySettingsPanel && (
        <DisplaySettingsPanel
          onClose={handlers.handleCloseDisplaySettingsPanel}
          onDismiss={handlers.handleDismissDisplaySettingsPanel}
          onOpenStatusLine={handlers.handleOpenStatusLinePanel}
        />
      )}
      {showThemePanel && (
        <ThemePanel onClose={handlers.handleCloseThemePanel} />
      )}
      {showStatusLinePanel && (
        <StatusLineSettingsPanel
          surface={surface}
          onClose={handlers.handleCloseStatusLinePanel}
          onDismiss={handlers.handleDismissStatusLinePanel}
        />
      )}
      {showSettingsPanel && (
        <SettingsPanel onClose={handlers.handleCloseSettingsPanel} />
      )}
      {showKnowledgePanel && (
        <KnowledgePanel
          entries={knowledgeEntries}
          status={knowledgeStatus}
          onClose={handlers.handleCloseKnowledgePanel}
        />
      )}
      {showCodePanel && (
        <CodePanel
          data={codeData}
          onClose={handlers.handleCloseCodePanel}
          onRefresh={handlers.handleRefreshCodePanel}
        />
      )}
      {artifactViewOpen && <ArtifactView />}
      {showSurveyPanel && (
        <SurveyPanel onClose={closeSurveyPanel} onSubmit={submitSurvey} />
      )}
      {showCloudQuitPrompt && (
        <CloudQuitPrompt
          onKeepRunning={() => quitCloudSessionKeepRunning(kiro)}
          onTurnOff={() => quitCloudSessionTurnOff(kiro)}
          onCancel={() => setShowCloudQuitPrompt(false)}
        />
      )}
    </>
  );
};

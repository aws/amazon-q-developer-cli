/**
 * Backend-driven panel cluster shared by InlineLayout and LiteLayout. Reads
 * show-flag + data state from the store directly (no 30+ prop-drill); each
 * layout supplies its own wrapper and the shared useBackendPanelHandlers.
 */
import React, { useCallback, useMemo } from 'react';
import { ContextBreakdown } from '../../ui/ContextBreakdown.js';
import { HelpPanel } from '../../ui/HelpPanel.js';
import { TuiPanel } from '../../ui/TuiPanel.js';
import { McpPanel } from '../../ui/McpPanel.js';
import { ToolsPanel } from '../../ui/ToolsPanel.js';
import { GoalPanel } from '../../ui/GoalPanel.js';
import { StatsPanel } from '../../ui/StatsPanel.js';
import { HooksPanel } from '../../ui/HooksPanel.js';
import { RepoPickerPanel } from '../../ui/RepoPickerPanel.js';
import { KnowledgePanel } from '../../ui/KnowledgePanel.js';
import { CodePanel } from '../../ui/CodePanel.js';
import { UsagePanel } from '../../ui/UsagePanel.js';
import { ChangelogPanel } from '../../ui/ChangelogPanel.js';
import { Explorer } from '../../ui/Explorer.js';
import { KeybindingsPanel } from '../../ui/KeybindingsPanel.js';
import { DisplaySettingsPanel } from '../../ui/DisplaySettingsPanel.js';
import { ThemePanel } from '../../ui/ThemePanel.js';
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
import { useAppStore, type McpServerInfo } from '../../../stores/app-store.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { startMcpOAuth } from '../../../utils/mcp-oauth.js';
import { copyToSystemClipboard } from '../../../commands/effects.js';
import type { BackendPanelHandlers } from './useBackendPanelHandlers.js';

interface BackendPanelsProps {
  handlers: BackendPanelHandlers;
}

export const BackendPanels: React.FC<BackendPanelsProps> = ({ handlers }) => {
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
    showKeybindingsPanel,
    showDisplaySettingsPanel,
    showThemePanel,
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
  const { setShowMcpPanel, setShowCloudQuitPrompt } = useUIActions();
  const { submitRepoPicker } = useUIActions();
  const attachedRepos = useAppStore((s) => s.attachedRepos);
  const { initErrors, pendingOAuthServers } = useNotificationState();
  const { showTransientAlert } = useNotificationActions();
  const { contextUsagePercent, currentModel, currentAgent } = useContextState();
  const { kiro } = useKiroClient();
  const agentEngine = useAppStore((s) => s.agentEngine);
  const showSurveyPanel = useAppStore((s) => s.showSurveyPanel);
  const closeSurveyPanel = useAppStore((s) => s.closeSurveyPanel);
  const submitSurvey = useAppStore((s) => s.submitSurvey);

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

  // Run a single-server /mcp action (e.g. "auth <name>") then refresh the
  // panel's status snapshot. Live OAuth/init events update pendingOAuthServers
  // separately.
  const runMcpServerAction = useCallback(
    async (value: string) => {
      await kiro.executeCommand({
        command: 'mcp',
        args: { value },
      } as any);
      const result = await kiro.executeCommand({
        command: 'mcp',
        args: { value: '' },
      } as any);
      if (result?.data) {
        const data = result.data as {
          servers?: McpServerInfo[];
          mode?: string;
        };
        setShowMcpPanel(true, data.servers ?? [], data.mode ?? 'status');
      }
    },
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
          onClose={handlers.handleCloseMcpPanel}
          onAuthenticate={(serverName) => {
            // Mirror the Ctrl+Y path so the panel shows the same
            // notification: KAS resets the server to (re)start OAuth;
            // V2 copies the (already valid) URL to the clipboard.
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
          }}
          onForceAuth={(serverName) => {
            void runMcpServerAction(`auth ${serverName}`);
          }}
          onAbortAuth={(serverName) => {
            void runMcpServerAction(`cancel-auth ${serverName}`);
          }}
          onRemoveCredentials={(serverName) => {
            void runMcpServerAction(`logout ${serverName}`);
          }}
          onAction={async (serverNames: string[]) => {
            const action = mcpMode === 'add' ? 'add' : 'remove';
            await kiro.executeCommand({
              command: 'mcp',
              args: { value: `${action} ${serverNames.join(',')}` },
            } as any);
            const result = await kiro.executeCommand({
              command: 'mcp',
              args: { value: action },
            } as any);
            if (result?.data) {
              const data = result.data as {
                servers?: McpServerInfo[];
                mode?: string;
              };
              setShowMcpPanel(true, data.servers ?? [], data.mode ?? action);
            }
          }}
        />
      )}
      {showToolsPanel && (
        <ToolsPanel
          tools={toolsList}
          initErrors={initErrors}
          onClose={handlers.handleCloseToolsPanel}
        />
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
      {showKeybindingsPanel && (
        <KeybindingsPanel onClose={handlers.handleCloseKeybindingsPanel} />
      )}
      {showDisplaySettingsPanel && (
        <DisplaySettingsPanel
          onClose={handlers.handleCloseDisplaySettingsPanel}
          onDismiss={handlers.handleDismissDisplaySettingsPanel}
        />
      )}
      {showThemePanel && (
        <ThemePanel onClose={handlers.handleCloseThemePanel} />
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

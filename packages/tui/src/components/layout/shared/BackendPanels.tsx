/**
 * Backend-driven panel cluster — shared between InlineLayout and LiteLayout.
 *
 * Both layouts mount the same set of panels in response to the same set of
 * show-flags. This component returns a fragment of `{showX && <Panel />}`
 * conditionals; each layout decides its own wrapper (LiteLayout wraps in a
 * <Box> inside its !showApproval && anyPanelOpen gate; InlineLayout mounts
 * directly inside <PromptBar>'s children).
 *
 * Show-flag and data state is read from the store directly so the parent
 * layouts don't have to prop-drill 30+ values. Handlers (the close + tab +
 * refresh callbacks) are the only injection point — they're shared via the
 * useBackendPanelHandlers hook in the same folder.
 */
import React, { useMemo } from 'react';
import { ContextBreakdown } from '../../ui/ContextBreakdown.js';
import { HelpPanel } from '../../ui/HelpPanel.js';
import { McpPanel } from '../../ui/McpPanel.js';
import { ToolsPanel } from '../../ui/ToolsPanel.js';
import { StatsPanel } from '../../ui/StatsPanel.js';
import { HooksPanel } from '../../ui/HooksPanel.js';
import { KnowledgePanel } from '../../ui/KnowledgePanel.js';
import { CodePanel } from '../../ui/CodePanel.js';
import { UsagePanel } from '../../ui/UsagePanel.js';
import { ChangelogPanel } from '../../ui/ChangelogPanel.js';
import { Explorer } from '../../ui/Explorer.js';
import { KeybindingsPanel } from '../../ui/KeybindingsPanel.js';
import { DisplaySettingsPanel } from '../../ui/DisplaySettingsPanel.js';
import { ArtifactView } from '../../ui/ArtifactView/index.js';
import { SurveyPanel } from '../../ui/SurveyPanel.js';
import {
  useUIState,
  useUIActions,
  useNotificationState,
  useContextState,
  useKiroClient,
} from '../../../stores/selectors.js';
import { useAppStore, type McpServerInfo } from '../../../stores/app-store.js';
import type { BackendPanelHandlers } from './useBackendPanelHandlers.js';

interface BackendPanelsProps {
  handlers: BackendPanelHandlers;
}

export const BackendPanels: React.FC<BackendPanelsProps> = ({ handlers }) => {
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
    toolsList,
    showStatsPanel,
    statsList,
    statsSummary,
    showHooksPanel,
    hooksList,
    showKeybindingsPanel,
    showDisplaySettingsPanel,
    showKnowledgePanel,
    knowledgeEntries,
    knowledgeStatus,
    showCodePanel,
    codeData,
    showChangelogPanel,
    artifactViewOpen,
  } = useUIState();
  const { setShowMcpPanel } = useUIActions();
  const { initErrors, pendingOAuthServers } = useNotificationState();
  const { contextUsagePercent, currentModel, currentAgent } = useContextState();
  const { kiro } = useKiroClient();
  const showSurveyPanel = useAppStore((s) => s.showSurveyPanel);
  const closeSurveyPanel = useAppStore((s) => s.closeSurveyPanel);
  const submitSurvey = useAppStore((s) => s.submitSurvey);

  // Overlay auth-required status onto MCP servers pending OAuth — same
  // shaping both layouts had locally.
  const mcpServersWithAuth = useMemo(() => {
    if (pendingOAuthServers.size === 0) return mcpServers;
    return mcpServers.map((s) =>
      pendingOAuthServers.has(s.name)
        ? { ...s, status: 'auth-required' as const }
        : s
    );
  }, [mcpServers, pendingOAuthServers]);

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
            { key: '↑↓', label: 'navigate' },
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
          onClose={handlers.handleCloseToolsPanel}
        />
      )}
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
      {showKeybindingsPanel && (
        <KeybindingsPanel onClose={handlers.handleCloseKeybindingsPanel} />
      )}
      {showDisplaySettingsPanel && (
        <DisplaySettingsPanel
          onClose={handlers.handleCloseDisplaySettingsPanel}
        />
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
    </>
  );
};

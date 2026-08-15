/**
 * Backend-driven panel cluster shared by InlineLayout and LiteLayout. Reads
 * show-flag + data state from the store directly (no 30+ prop-drill); each
 * layout supplies its own wrapper and the shared useBackendPanelHandlers.
 */
import React, { useCallback, useMemo } from 'react';
import { useStore } from 'zustand';
import { useShallow } from 'zustand/react/shallow';
import { ContextBreakdown } from '../../ui/ContextBreakdown.js';
import { HelpPanel } from '../../ui/HelpPanel.js';
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
import { MemoriesPanel } from '../../ui/MemoriesPanel.js';
import { Explorer } from '../../ui/Explorer.js';
import { KeybindingsPanel } from '../../ui/KeybindingsPanel.js';
import { DisplaySettingsPanel } from '../../ui/DisplaySettingsPanel.js';
import { ThemePanel } from '../../ui/ThemePanel.js';
import { StatusLineSettingsPanel } from '../../ui/StatusLineSettingsPanel.js';
import { SettingsPanel } from '../../ui/SettingsPanel.js';
import { ConfigPanel } from '../../ui/ConfigPanel.js';
import { snapshotReportsSources } from '../../ui/config-panel-model.js';
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
import { useAppStore, type AppState } from '../../../stores/app-store.js';
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
  surface: UiMode;
}

type InlinePanelGate = 'header' | 'input' | 'copyHint';

const ALL_INLINE_GATES = [
  'header',
  'input',
  'copyHint',
] as const satisfies readonly InlinePanelGate[];

interface BackendPanelDefinition {
  stateKey: keyof AppState;
  componentName: string;
  inlineGates: readonly InlinePanelGate[];
}

const BACKEND_PANEL_DEFINITIONS = [
  ['showContextBreakdown', 'ContextBreakdown', ALL_INLINE_GATES],
  ['showUsagePanel', 'UsagePanel', ALL_INLINE_GATES],
  ['showRewindExplorer', 'Explorer', ALL_INLINE_GATES],
  ['showTangentExplorer', 'Explorer', ALL_INLINE_GATES],
  ['showHelpPanel', 'HelpPanel', ALL_INLINE_GATES],
  ['showChangelogPanel', 'ChangelogPanel', ALL_INLINE_GATES],
  ['showMemoriesPanel', 'MemoriesPanel', ALL_INLINE_GATES],
  ['showMcpPanel', 'McpPanel', ALL_INLINE_GATES],
  ['showToolsPanel', 'ToolsPanel', ALL_INLINE_GATES],
  ['showGoalPanel', 'GoalPanel', ['header']],
  ['showStatsPanel', 'StatsPanel', ['header', 'input']],
  ['showHooksPanel', 'HooksPanel', ALL_INLINE_GATES],
  ['showRepoPicker', 'RepoPickerPanel', ALL_INLINE_GATES],
  ['showSessionPicker', 'SessionPickerPanel', ALL_INLINE_GATES],
  ['showKeybindingsPanel', 'KeybindingsPanel', ALL_INLINE_GATES],
  ['showDisplaySettingsPanel', 'DisplaySettingsPanel', ALL_INLINE_GATES],
  ['showStatusLinePanel', 'StatusLineSettingsPanel', ['input', 'copyHint']],
  ['showThemePanel', 'ThemePanel', ALL_INLINE_GATES],
  ['showSettingsPanel', 'SettingsPanel', ALL_INLINE_GATES],
  ['showConfigPanel', 'ConfigPanel', ALL_INLINE_GATES],
  ['showKnowledgePanel', 'KnowledgePanel', ALL_INLINE_GATES],
  ['showCodePanel', 'CodePanel', ALL_INLINE_GATES],
  ['artifactViewOpen', 'ArtifactView', ALL_INLINE_GATES],
  ['showSurveyPanel', 'SurveyPanel', ALL_INLINE_GATES],
  ['showCloudQuitPrompt', 'CloudQuitPrompt', ALL_INLINE_GATES],
] as const satisfies readonly (readonly [
  keyof AppState,
  string,
  readonly InlinePanelGate[],
])[];

export const BACKEND_PANEL_REGISTRY = BACKEND_PANEL_DEFINITIONS.map(
  ([stateKey, componentName, inlineGates]) => ({
    stateKey,
    componentName,
    inlineGates,
  })
) satisfies readonly BackendPanelDefinition[];

export type BackendPanelStateKey =
  (typeof BACKEND_PANEL_REGISTRY)[number]['stateKey'];
export type BackendPanelId = BackendPanelStateKey | 'workflowHistory';

export const BACKEND_PANEL_STATE_KEYS = BACKEND_PANEL_REGISTRY.map(
  ({ stateKey }) => stateKey
);
export const BACKEND_PANEL_COMPONENT_NAMES = BACKEND_PANEL_REGISTRY.map(
  ({ componentName }) => componentName
);

export interface BackendPanelVisibility {
  any: boolean;
  inlineHeader: boolean;
  inlineInput: boolean;
  inlineCopyHint: boolean;
}

export function selectBackendPanelVisibility(
  state: Pick<AppState, BackendPanelStateKey>
): BackendPanelVisibility {
  const visibility: BackendPanelVisibility = {
    any: false,
    inlineHeader: false,
    inlineInput: false,
    inlineCopyHint: false,
  };

  for (const { stateKey, inlineGates } of BACKEND_PANEL_REGISTRY) {
    if (!state[stateKey]) continue;
    visibility.any = true;
    for (const gate of inlineGates as readonly InlinePanelGate[]) {
      if (gate === 'header') visibility.inlineHeader = true;
      if (gate === 'input') visibility.inlineInput = true;
      if (gate === 'copyHint') visibility.inlineCopyHint = true;
    }
  }

  return visibility;
}

export function useBackendPanelVisibility(): BackendPanelVisibility {
  const visibility = useAppStore(useShallow(selectBackendPanelVisibility));
  const workflowHistoryOpen = useStore(
    workflowStore,
    (state) => state.history.isOpen
  );
  return workflowHistoryOpen
    ? {
        any: true,
        inlineHeader: true,
        inlineInput: true,
        inlineCopyHint: true,
      }
    : visibility;
}

export const BackendPanels: React.FC<BackendPanelsProps> = ({
  handlers,
  surface,
}) => {
  const glyphs = useGlyphs();
  // Wide terminals render the session dashboard as a pinned left panel (the
  // layout owns it); the overlay here is the narrow-terminal fallback only.
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
    showMemoriesPanel,
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
  const addSystemMessage = useAppStore((s) => s.addSystemMessage);
  const cloudSessionActive = useAppStore((s) => s.cloudSessionActive);
  const cloudSnapshotReadiness = useAppStore((s) => s.cloudSnapshotReadiness);
  const supportsMcpCommandActions =
    engineSupportsMcpCommandActions(agentEngine);
  const showConfigPanel = useAppStore((s) => s.showConfigPanel);
  const configPanelCategory = useAppStore((s) => s.configPanelCategory);
  const setShowConfigPanel = useAppStore((s) => s.setShowConfigPanel);
  const setConfigReturnOnEscape = useAppStore((s) => s.setConfigReturnOnEscape);
  const endConfigHandoff = useAppStore((s) => s.endConfigHandoff);
  const dispatchSlashCommand = useAppStore((s) => s.dispatchSlashCommand);
  // Top-level /config close. Always clears the back-flag so the next overlay
  // open starts fresh (twin of handleCloseSettingsPanel's stale-flag guard).
  const handleCloseConfigPanel = useCallback(() => {
    setShowConfigPanel(false);
    setConfigReturnOnEscape(false);
  }, [setShowConfigPanel, setConfigReturnOnEscape]);
  const configAgents = useAppStore((s) => s.kas.availableAgents);
  const mcpServerCache = useAppStore((s) => s.mcpServerCache);
  const configSteering = useAppStore((s) => s.steering);
  const configSteeringDocs = useAppStore((s) => s.steeringDocs);
  const configSkills = useAppStore((s) => s.skills);
  const configPowers = useAppStore((s) => s.powersList);
  const configDiagnostics = useAppStore((s) => s.cloudConfigDiagnostics);
  const showSurveyPanel = useAppStore((s) => s.showSurveyPanel);
  const closeSurveyPanel = useAppStore((s) => s.closeSurveyPanel);
  const submitSurvey = useAppStore((s) => s.submitSurvey);
  const workflowHistoryOpen = useStore(
    workflowStore,
    (state) => state.history.isOpen
  );
  const panelState = {
    showContextBreakdown,
    showUsagePanel,
    showRewindExplorer,
    showTangentExplorer,
    showHelpPanel,
    showChangelogPanel,
    showMemoriesPanel,
    showMcpPanel,
    showToolsPanel,
    showGoalPanel,
    showStatsPanel,
    showHooksPanel,
    showRepoPicker,
    showSessionPicker,
    showKeybindingsPanel,
    showDisplaySettingsPanel,
    showStatusLinePanel,
    showThemePanel,
    showSettingsPanel,
    showConfigPanel,
    showKnowledgePanel,
    showCodePanel,
    artifactViewOpen,
    showSurveyPanel,
    showCloudQuitPrompt,
  } satisfies Pick<AppState, BackendPanelStateKey>;

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
        addSystemMessage,
      });
    },
    [
      addSystemMessage,
      agentEngine,
      kiro,
      pendingOAuthServers,
      showTransientAlert,
    ]
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

  const panelRenderers = {
    showContextBreakdown: () => (
      <ContextBreakdown
        percent={contextUsagePercent}
        breakdown={contextBreakdown ?? undefined}
        model={currentModel?.name ?? null}
        agentName={currentAgent?.name ?? null}
        initialExpanded={contextBreakdown?.initialExpanded}
        onClose={handlers.handleCloseContextBreakdown}
        onTabSwitch={handlers.handleTabFromContext}
      />
    ),
    showUsagePanel: () => (
      <UsagePanel
        data={usageData}
        onClose={handlers.handleCloseUsagePanel}
        onTabSwitch={handlers.handleTabFromUsage}
      />
    ),
    showRewindExplorer: () => (
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
    ),
    showTangentExplorer: () => (
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
          const tangentRow = tangentRows.find((item) => item.id === row.id);
          handlers.handleTangentSelect(row.id, tangentRow?.title ?? row.id);
        }}
        onClose={handlers.handleCloseTangentExplorer}
      />
    ),
    showHelpPanel: () => (
      <HelpPanel
        commands={helpCommands}
        onClose={handlers.handleCloseHelpPanel}
      />
    ),
    showChangelogPanel: () => (
      <ChangelogPanel onClose={handlers.handleCloseChangelogPanel} />
    ),
    showMemoriesPanel: () => (
      <MemoriesPanel onClose={handlers.handleCloseMemoriesPanel} />
    ),
    showMcpPanel: () => (
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
    ),
    showToolsPanel: () => (
      <ToolsPanel
        tools={toolsList}
        initErrors={initErrors}
        cloudSessionActive={cloudSessionActive}
        cloudSnapshotReadiness={cloudSnapshotReadiness.tools}
        onClose={handlers.handleCloseToolsPanel}
      />
    ),
    showGoalPanel: () => <GoalPanel onClose={handlers.handleCloseGoalPanel} />,
    showStatsPanel: () => (
      <StatsPanel
        stats={statsList}
        summary={statsSummary}
        onClose={handlers.handleCloseStatsPanel}
      />
    ),
    showHooksPanel: () => (
      <HooksPanel
        hooks={hooksList}
        cloudSessionActive={cloudSessionActive}
        onClose={handlers.handleCloseHooksPanel}
      />
    ),
    showRepoPicker: () => (
      <RepoPickerPanel
        resources={repoPickerResources}
        initialSelected={attachedRepos}
        onSubmit={(selected) => void submitRepoPicker(selected)}
        onClose={handlers.handleCloseRepoPicker}
      />
    ),
    showSessionPicker: () => (
      <SessionPickerPanel
        rows={sessionPickerRows}
        title={sessionPickerTitle}
        onSelect={handlers.handleSessionSelect}
        onClose={handlers.handleCloseSessionPicker}
      />
    ),
    showKeybindingsPanel: () => (
      <KeybindingsPanel onClose={handlers.handleCloseKeybindingsPanel} />
    ),
    showDisplaySettingsPanel: () => (
      <DisplaySettingsPanel
        surface={surface}
        onClose={handlers.handleCloseDisplaySettingsPanel}
        onDismiss={handlers.handleDismissDisplaySettingsPanel}
        onOpenStatusLine={handlers.handleOpenStatusLinePanel}
      />
    ),
    showThemePanel: () => (
      <ThemePanel onClose={handlers.handleCloseThemePanel} />
    ),
    showStatusLinePanel: () => (
      <StatusLineSettingsPanel
        surface={surface}
        onClose={handlers.handleCloseStatusLinePanel}
        onDismiss={handlers.handleDismissStatusLinePanel}
      />
    ),
    showSettingsPanel: () => (
      <SettingsPanel onClose={handlers.handleCloseSettingsPanel} />
    ),
    showConfigPanel: () => (
      <ConfigPanel
        snapshot={(() => {
          const base = {
            cloudSession: cloudSessionActive,
            agents: configAgents,
            mcpServers: mcpServerCache,
            steering: configSteering,
            steeringDocs: configSteeringDocs,
            skills: configSkills,
            hooks: hooksList,
            powers: configPowers,
            diagnostics: configDiagnostics,
          };
          // Fact-based, not engine-based: Source columns render only when
          // some item carries a descriptor origin or the session is cloud —
          // the same rule McpPanel/HooksPanel use. A descriptor-free local
          // session (V2, or KAS without #2141) shows no Source columns.
          return { ...base, sourcesReported: snapshotReportsSources(base) };
        })()}
        initialCategory={configPanelCategory ?? undefined}
        onClose={handleCloseConfigPanel}
        // Row-select handoff: dispatch the typed subcommand so row select
        // and `/config mcp` are one code path (config-subcommands.ts). Via
        // dispatchSlashCommand, not handleUserInput — the latter is the
        // user-input gate and rejects/queues slash commands while a turn is
        // in flight, which would close /config and swallow the action; a
        // row select on an already-open panel is a UI navigation, not new
        // input, so it bypasses the busy gate the way /settings sub-panel
        // openings do. recordAs:null keeps UI navigation out of Up-arrow
        // history; rejections surface via the effect's catch → alert, and
        // the catch here only guards the dispatch plumbing itself.
        // No .finally ending the handoff here: the showConfigMenu effect
        // fire-and-forgets the subcommand handler, so this promise resolves
        // BEFORE the routed RPC does — ending it from here would break the
        // inert window mid-flight. The handler's own finally owns the end;
        // a rejection that never reaches it ends via .catch, and ESC (which
        // cancels the handoff) recovers any residue.
        onOpenMcp={() => {
          dispatchSlashCommand('/config mcp', null).catch(() =>
            endConfigHandoff()
          );
        }}
        onOpenHooks={() => {
          dispatchSlashCommand('/config hooks', null).catch(() =>
            endConfigHandoff()
          );
        }}
        onOpenAgent={() => {
          dispatchSlashCommand('/config agents', null).catch(() =>
            endConfigHandoff()
          );
        }}
      />
    ),
    showKnowledgePanel: () => (
      <KnowledgePanel
        entries={knowledgeEntries}
        status={knowledgeStatus}
        onClose={handlers.handleCloseKnowledgePanel}
      />
    ),
    showCodePanel: () => (
      <CodePanel
        data={codeData}
        onClose={handlers.handleCloseCodePanel}
        onRefresh={handlers.handleRefreshCodePanel}
      />
    ),
    artifactViewOpen: () => <ArtifactView />,
    showSurveyPanel: () => (
      <SurveyPanel onClose={closeSurveyPanel} onSubmit={submitSurvey} />
    ),
    showCloudQuitPrompt: () => (
      <CloudQuitPrompt
        onKeepRunning={() => quitCloudSessionKeepRunning(kiro)}
        onTurnOff={() => quitCloudSessionTurnOff(kiro)}
        onCancel={() => setShowCloudQuitPrompt(false)}
      />
    ),
  } satisfies Record<BackendPanelStateKey, () => React.ReactNode>;

  return (
    <>
      {BACKEND_PANEL_REGISTRY.map(({ stateKey }) =>
        panelState[stateKey] ? (
          <React.Fragment key={stateKey}>
            {panelRenderers[stateKey]()}
          </React.Fragment>
        ) : null
      )}
      {workflowHistoryOpen && (
        <WorkflowHistoryPanel onClose={handlers.handleCloseWorkflowHistory} />
      )}
    </>
  );
};

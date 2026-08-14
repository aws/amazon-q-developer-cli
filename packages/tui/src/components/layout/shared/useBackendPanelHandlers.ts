/**
 * Shared close + tab handlers for the backend-driven panel cluster (used by
 * InlineLayout and LiteLayout). Close = flip show flag off, drop activeCommand,
 * clearCommandInput. `returnToSettings` panels also honor settingsReturnOnEscape
 * so /settings → sub-panel → Esc returns to the /settings menu.
 */
import { useCallback, useContext } from 'react';
import {
  useUIActions,
  useCommandActions,
  useKiroClient,
  useInputActions,
} from '../../../stores/selectors.js';
import {
  AppStoreContext,
  useAppStore,
  type CodePanelData,
} from '../../../stores/app-store.js';
import { resolveTangentSelection } from '../../../utils/tangent-nav.js';
import { workflowStore } from '../../../stores/workflow-store.js';

export function useBackendPanelHandlers() {
  const {
    setShowContextBreakdown,
    setShowHelpPanel,
    setShowGoalPanel,
    setShowUsagePanel,
    setShowMcpPanel,
    setShowToolsPanel,
    setShowStatsPanel,
    setShowHooksPanel,
    setShowRepoPicker,
    retrySourceProviderConnection,
    setShowSessionPicker,
    setShowSessionDashboard,
    setShowKnowledgePanel,
    setShowCodePanel,
    setShowChangelogPanel,
    setShowMemoriesPanel,
    setShowRewindExplorer,
    setShowTangentExplorer,
    setShowKeybindingsPanel,
    setShowDisplaySettingsPanel,
    setShowThemePanel,
    setShowStatusLinePanel,
    setShowSettingsPanel,
    setSettingsReturnOnEscape,
    reopenSettingsMenu,
  } = useUIActions();
  const { setActiveCommand, clearCommandInput, resumeSession } =
    useCommandActions();
  const { handleUserInput, dispatchSlashCommand } = useInputActions();
  const { kiro } = useKiroClient();
  const store = useContext(AppStoreContext);
  if (!store) throw new Error('Missing StoreContext.Provider in the tree');
  const settingsReturnOnEscape = useAppStore((s) => s.settingsReturnOnEscape);
  const configReturnOnEscape = useAppStore((s) => s.configReturnOnEscape);
  const setConfigReturnOnEscape = useAppStore((s) => s.setConfigReturnOnEscape);
  const reopenConfigMenu = useAppStore((s) => s.reopenConfigMenu);

  // makeClose wraps useCallback; it's called unconditionally and in fixed
  // order each render, so React hook ordering holds (hence the suppression).
  const makeClose = (
    setShow: (open: boolean) => void,
    opts?: {
      returnToSettings?: boolean;
      returnToConfig?: boolean;
      clearReturnFlag?: boolean;
    }
  ) =>
    // eslint-disable-next-line react-hooks/rules-of-hooks
    useCallback(() => {
      setShow(false);
      setActiveCommand(null);
      clearCommandInput();
      if (opts?.returnToSettings && settingsReturnOnEscape) {
        setSettingsReturnOnEscape(false);
        reopenSettingsMenu();
      } else if (opts?.clearReturnFlag) {
        setSettingsReturnOnEscape(false);
      }
      // /config twin of the settings branch: a panel the /config table
      // routed to (MCP, hooks) walks back to the table on ESC.
      if (opts?.returnToConfig && configReturnOnEscape) {
        setConfigReturnOnEscape(false);
        reopenConfigMenu();
      }
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [
      setShow,
      setActiveCommand,
      clearCommandInput,
      settingsReturnOnEscape,
      setSettingsReturnOnEscape,
      reopenSettingsMenu,
      configReturnOnEscape,
      setConfigReturnOnEscape,
      reopenConfigMenu,
    ]);

  // makeClose calls run unconditionally in fixed order inside this object
  // literal, so React hook ordering holds (see makeClose suppression above).
  const closeHandlers = {
    handleCloseContextBreakdown: makeClose(setShowContextBreakdown),
    handleCloseUsagePanel: makeClose(setShowUsagePanel),
    handleCloseHelpPanel: makeClose(setShowHelpPanel),
    handleCloseGoalPanel: makeClose(setShowGoalPanel),
    handleCloseMcpPanel: makeClose(setShowMcpPanel, { returnToConfig: true }),
    handleCloseToolsPanel: makeClose(setShowToolsPanel),
    handleCloseStatsPanel: makeClose(setShowStatsPanel),
    handleCloseHooksPanel: makeClose(setShowHooksPanel, {
      returnToConfig: true,
    }),
    handleCloseRepoPicker: makeClose(setShowRepoPicker),
    handleCloseSessionPicker: makeClose(setShowSessionPicker),
    handleCloseSessionDashboard: makeClose(setShowSessionDashboard),
    handleCloseKnowledgePanel: makeClose(setShowKnowledgePanel),
    handleCloseCodePanel: makeClose(setShowCodePanel),
    handleCloseChangelogPanel: makeClose(setShowChangelogPanel),
    handleCloseMemoriesPanel: makeClose(setShowMemoriesPanel),
    handleCloseRewindExplorer: makeClose(setShowRewindExplorer),
    handleCloseTangentExplorer: makeClose(setShowTangentExplorer),
    handleCloseKeybindingsPanel: makeClose(setShowKeybindingsPanel, {
      returnToSettings: true,
    }),
    handleCloseDisplaySettingsPanel: makeClose(setShowDisplaySettingsPanel, {
      returnToSettings: true,
    }),
    // Enter-to-confirm path: plain close, never bounce back to /settings.
    handleDismissDisplaySettingsPanel: makeClose(setShowDisplaySettingsPanel),
    handleCloseThemePanel: makeClose(setShowThemePanel, {
      returnToSettings: true,
    }),
    // Display steps aside rather than stacking, so ESC out of the status-line
    // panel returns to /settings the same way any other sub-panel does.
    handleOpenStatusLinePanel: useCallback(() => {
      setShowDisplaySettingsPanel(false);
      setShowStatusLinePanel(true);
    }, [setShowDisplaySettingsPanel, setShowStatusLinePanel]),
    handleCloseStatusLinePanel: makeClose(setShowStatusLinePanel, {
      returnToSettings: true,
    }),
    // Enter confirms and leaves for the chat, so the back-flag has to go with it.
    // Left set, the next unrelated panel would bounce into /settings on ESC.
    handleDismissStatusLinePanel: makeClose(setShowStatusLinePanel, {
      clearReturnFlag: true,
    }),
  };

  const handleCloseSettingsPanel = useCallback(() => {
    // Top-level /settings close. Always clears the back-flag so the next
    // overlay open starts fresh — avoids a stale `settingsReturnOnEscape`
    // bouncing the user into /settings unexpectedly.
    setShowSettingsPanel(false);
    setActiveCommand(null);
    clearCommandInput();
    setSettingsReturnOnEscape(false);
  }, [
    setShowSettingsPanel,
    setActiveCommand,
    clearCommandInput,
    setSettingsReturnOnEscape,
  ]);

  const handleCloseWorkflowHistory = useCallback(() => {
    workflowStore.getState().closeWorkflowHistory();
    setActiveCommand(null);
    clearCommandInput();
  }, [setActiveCommand, clearCommandInput]);

  const handleTabFromContext = useCallback(async () => {
    try {
      const result = await kiro.executeCommand({
        command: 'usage',
        args: {},
      } as any);
      if (result?.data) {
        setShowUsagePanel(true, result.data);
        setShowContextBreakdown(false);
      }
    } catch {
      /* ignore */
    }
  }, [kiro, setShowUsagePanel, setShowContextBreakdown]);

  const handleTabFromUsage = useCallback(async () => {
    try {
      // KAS: prefer the store-owned pushed breakdown — no round-trip required.
      const isKas = store.getState().agentEngine === 'kas';
      const contextBreakdownCache = isKas
        ? store.getState().contextBreakdownCache
        : null;
      if (contextBreakdownCache) {
        setShowContextBreakdown(true, contextBreakdownCache);
        setShowUsagePanel(false);
        return;
      }
      // V2 Rust: fall back to the engine-specific executeCommand path,
      // which returns the breakdown inline.
      const result = await kiro.executeCommand({
        command: 'context',
        args: {},
      } as any);
      const responseBreakdown =
        result?.data &&
        typeof result.data === 'object' &&
        'breakdown' in result.data
          ? (result.data as any).breakdown
          : null;
      // KAS previously re-read its client cache after contextShow resolved.
      // Preserve that race behavior now that the cache belongs to the store.
      const breakdown =
        responseBreakdown ??
        (isKas ? store.getState().contextBreakdownCache : null);
      if (breakdown) {
        setShowContextBreakdown(true, breakdown);
        setShowUsagePanel(false);
      }
    } catch {
      /* ignore */
    }
  }, [kiro, setShowContextBreakdown, setShowUsagePanel, store]);

  const handleRefreshCodePanel = useCallback(async () => {
    try {
      const result = await kiro.executeCommand({
        command: 'code',
        args: {},
      } as any);
      if (result?.data) {
        setShowCodePanel(true, result.data as CodePanelData);
      }
    } catch {
      /* ignore */
    }
  }, [kiro, setShowCodePanel]);

  const handleRewindSelect = useCallback(
    async (rowId: string) => {
      setShowRewindExplorer(false);
      store.setState({ activeCommand: null });
      clearCommandInput();
      await handleUserInput(`/rewind ${rowId}`);
      await store.getState().processQueue();
    },
    [setShowRewindExplorer, clearCommandInput, handleUserInput, store]
  );

  // Resume the chosen session via the store's resumeSession action, which fires
  // the synthetic `/chat <id>` dispatch so the KAS handler's loadExistingSession
  // flow resolves + loads it (native or cross-engine), same as the old menu path.
  // `targetCwd` (set for cross-workspace loads) switches the working directory
  // first so the session loads in its own project.
  const handleSessionSelect = useCallback(
    (sessionId: string, environment: 'local' | 'cloud', targetCwd?: string) => {
      void resumeSession(sessionId, environment, targetCwd);
    },
    [resumeSession]
  );

  // Cloud-entry source-provider gate. Open-browser keeps the gate up; retry
  // re-probes the connection (dismisses on success, stays up otherwise); quit
  // detaches and exits cleanly.
  const handleSourceProviderRetry = useCallback(async () => {
    await retrySourceProviderConnection();
  }, [retrySourceProviderConnection]);
  const handleSourceProviderQuit = useCallback(() => {
    kiro.close();
    process.exit(0);
  }, [kiro]);

  const handleTangentSelect = useCallback(
    async (sessionId: string, title: string) => {
      setShowTangentExplorer(false);
      store.setState({ activeCommand: null });
      clearCommandInput();
      // A picker selects a session, so switch to that exact session id (root,
      // sibling, or descendant) — never round-trip through a title/bare command.
      // Selecting the row you're already on is a no-op.
      const decision = resolveTangentSelection(sessionId, kiro.sessionId);
      if (decision.action === 'noop') {
        await store.getState().processQueue();
        return;
      }
      await dispatchSlashCommand(
        `/tangent ${decision.sessionId}`,
        `/tangent ${title}`
      );
      await store.getState().processQueue();
    },
    [
      setShowTangentExplorer,
      clearCommandInput,
      dispatchSlashCommand,
      kiro,
      store,
    ]
  );

  return {
    ...closeHandlers,
    handleCloseWorkflowHistory,
    handleCloseSettingsPanel,
    handleTabFromContext,
    handleTabFromUsage,
    handleRefreshCodePanel,
    handleRewindSelect,
    handleSessionSelect,
    handleSourceProviderRetry,
    handleSourceProviderQuit,
    handleTangentSelect,
  };
}

export type BackendPanelHandlers = ReturnType<typeof useBackendPanelHandlers>;

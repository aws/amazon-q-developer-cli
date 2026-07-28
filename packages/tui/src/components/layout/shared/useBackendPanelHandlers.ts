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
    setShowTuiPanel,
    setShowGoalPanel,
    setShowUsagePanel,
    setShowMcpPanel,
    setShowToolsPanel,
    setShowStatsPanel,
    setShowHooksPanel,
    setShowRepoPicker,
    retrySourceProviderConnection,
    setShowSessionPicker,
    setShowKnowledgePanel,
    setShowCodePanel,
    setShowChangelogPanel,
    setShowRewindExplorer,
    setShowTangentExplorer,
    setShowKeybindingsPanel,
    setShowDisplaySettingsPanel,
    setShowThemePanel,
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

  // makeClose wraps useCallback; it's called unconditionally and in fixed
  // order each render, so React hook ordering holds (hence the suppression).
  const makeClose = (
    setShow: (open: boolean) => void,
    opts?: { returnToSettings?: boolean }
  ) =>
    // eslint-disable-next-line react-hooks/rules-of-hooks
    useCallback(() => {
      setShow(false);
      setActiveCommand(null);
      clearCommandInput();
      if (opts?.returnToSettings && settingsReturnOnEscape) {
        setSettingsReturnOnEscape(false);
        reopenSettingsMenu();
      }
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [
      setShow,
      setActiveCommand,
      clearCommandInput,
      settingsReturnOnEscape,
      setSettingsReturnOnEscape,
      reopenSettingsMenu,
    ]);

  // makeClose calls run unconditionally in fixed order inside this object
  // literal, so React hook ordering holds (see makeClose suppression above).
  const closeHandlers = {
    handleCloseContextBreakdown: makeClose(setShowContextBreakdown),
    handleCloseUsagePanel: makeClose(setShowUsagePanel),
    handleCloseHelpPanel: makeClose(setShowHelpPanel),
    handleCloseTuiPanel: makeClose(setShowTuiPanel),
    handleCloseGoalPanel: makeClose(setShowGoalPanel),
    handleCloseMcpPanel: makeClose(setShowMcpPanel),
    handleCloseToolsPanel: makeClose(setShowToolsPanel),
    handleCloseStatsPanel: makeClose(setShowStatsPanel),
    handleCloseHooksPanel: makeClose(setShowHooksPanel),
    handleCloseRepoPicker: makeClose(setShowRepoPicker),
    handleCloseSessionPicker: makeClose(setShowSessionPicker),
    handleCloseKnowledgePanel: makeClose(setShowKnowledgePanel),
    handleCloseCodePanel: makeClose(setShowCodePanel),
    handleCloseChangelogPanel: makeClose(setShowChangelogPanel),
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
    (rowId: string) => {
      setShowRewindExplorer(false);
      setActiveCommand(null);
      clearCommandInput();
      // Fire `/rewind <idx>` through the normal command pipeline so the
      // rewindAction effect handles the clone + session load.
      void handleUserInput(`/rewind ${rowId}`);
    },
    [
      setShowRewindExplorer,
      setActiveCommand,
      clearCommandInput,
      handleUserInput,
    ]
  );

  // Resume the chosen session via the store's resumeSession action, which fires
  // the synthetic `/chat <id>` dispatch so the KAS handler's loadExistingSession
  // flow resolves + loads it (native or cross-engine), same as the old menu path.
  const handleSessionSelect = useCallback(
    (sessionId: string, environment: 'local' | 'cloud') => {
      void resumeSession(sessionId, environment);
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
    (sessionId: string, title: string) => {
      setShowTangentExplorer(false);
      setActiveCommand(null);
      clearCommandInput();
      // A picker selects a session, so switch to that exact session id (root,
      // sibling, or descendant) — never round-trip through a title/bare command.
      // Selecting the row you're already on is a no-op.
      const decision = resolveTangentSelection(sessionId, kiro.sessionId);
      if (decision.action === 'noop') return;
      void dispatchSlashCommand(
        `/tangent ${decision.sessionId}`,
        `/tangent ${title}`
      );
    },
    [
      setShowTangentExplorer,
      setActiveCommand,
      clearCommandInput,
      dispatchSlashCommand,
      kiro,
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

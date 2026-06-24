/**
 * Shared close + tab handlers for the backend-driven panel cluster (used by
 * InlineLayout and LiteLayout). Close = flip show flag off, drop activeCommand,
 * clearCommandInput. `returnToSettings` panels also honor settingsReturnOnEscape
 * so /settings → sub-panel → Esc returns to the /settings menu.
 */
import { useCallback } from 'react';
import {
  useUIActions,
  useCommandActions,
  useKiroClient,
  useInputActions,
} from '../../../stores/selectors.js';
import { useAppStore, type CodePanelData } from '../../../stores/app-store.js';

export function useBackendPanelHandlers() {
  const {
    setShowContextBreakdown,
    setShowHelpPanel,
    setShowUsagePanel,
    setShowMcpPanel,
    setShowToolsPanel,
    setShowStatsPanel,
    setShowHooksPanel,
    setShowKnowledgePanel,
    setShowCodePanel,
    setShowChangelogPanel,
    setShowRewindExplorer,
    setShowKeybindingsPanel,
    setShowDisplaySettingsPanel,
    setShowThemePanel,
    setShowSettingsPanel,
    setSettingsReturnOnEscape,
    reopenSettingsMenu,
  } = useUIActions();
  const { setActiveCommand, clearCommandInput } = useCommandActions();
  const { handleUserInput } = useInputActions();
  const { kiro } = useKiroClient();
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
    handleCloseMcpPanel: makeClose(setShowMcpPanel),
    handleCloseToolsPanel: makeClose(setShowToolsPanel),
    handleCloseStatsPanel: makeClose(setShowStatsPanel),
    handleCloseHooksPanel: makeClose(setShowHooksPanel),
    handleCloseKnowledgePanel: makeClose(setShowKnowledgePanel),
    handleCloseCodePanel: makeClose(setShowCodePanel),
    handleCloseChangelogPanel: makeClose(setShowChangelogPanel),
    handleCloseRewindExplorer: makeClose(setShowRewindExplorer),
    handleCloseKeybindingsPanel: makeClose(setShowKeybindingsPanel, {
      returnToSettings: true,
    }),
    handleCloseDisplaySettingsPanel: makeClose(setShowDisplaySettingsPanel, {
      returnToSettings: true,
    }),
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
      const result = await kiro.executeCommand({
        command: 'context',
        args: {},
      } as any);
      if (
        result?.data &&
        typeof result.data === 'object' &&
        'breakdown' in result.data
      ) {
        setShowContextBreakdown(true, (result.data as any).breakdown);
        setShowUsagePanel(false);
      }
    } catch {
      /* ignore */
    }
  }, [kiro, setShowContextBreakdown, setShowUsagePanel]);

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

  return {
    ...closeHandlers,
    handleCloseSettingsPanel,
    handleTabFromContext,
    handleTabFromUsage,
    handleRefreshCodePanel,
    handleRewindSelect,
  };
}

export type BackendPanelHandlers = ReturnType<typeof useBackendPanelHandlers>;

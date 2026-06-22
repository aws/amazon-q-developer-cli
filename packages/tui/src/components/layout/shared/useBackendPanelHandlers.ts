/**
 * Shared close + tab handlers for the backend-driven panel cluster.
 *
 * Both InlineLayout and LiteLayout open the same set of panels (/help, /mcp,
 * /tools, /context, /usage, /stats, /hooks, /knowledge, /code, /tui,
 * /changelog, /rewind, /settings keybindings, /settings display) and close
 * them with the same 3-step pattern:
 *
 *   1. flip the show flag off
 *   2. drop the activeCommand wrapper (so the input re-enables)
 *   3. clearCommandInput (so a half-typed slash stops echoing)
 *
 * The keybindings/display close handlers also branch on
 * settingsReturnOnEscape so /settings → keybindings → Esc returns to the
 * /settings menu instead of the bare prompt.
 *
 * Layout-local concerns (subagent panel, approvals, the always-armed Esc
 * handler that gates on anyPanelOpen) stay in their respective layouts.
 */
import { useCallback } from 'react';
import {
  useUIActions,
  useCommandActions,
  useKiroClient,
  useInputActions,
} from '../../../stores/selectors.js';
import { useAppStore, type CodePanelData } from '../../../stores/app-store.js';

export interface BackendPanelHandlers {
  handleCloseContextBreakdown: () => void;
  handleCloseUsagePanel: () => void;
  handleCloseHelpPanel: () => void;
  handleCloseMcpPanel: () => void;
  handleCloseToolsPanel: () => void;
  handleCloseStatsPanel: () => void;
  handleCloseHooksPanel: () => void;
  handleCloseKnowledgePanel: () => void;
  handleCloseCodePanel: () => void;
  handleCloseChangelogPanel: () => void;
  handleCloseRewindExplorer: () => void;
  handleCloseKeybindingsPanel: () => void;
  handleCloseDisplaySettingsPanel: () => void;
  handleCloseThemePanel: () => void;
  handleCloseSettingsPanel: () => void;
  handleTabFromContext: () => Promise<void>;
  handleTabFromUsage: () => Promise<void>;
  handleRefreshCodePanel: () => Promise<void>;
  handleRewindSelect: (rowId: string) => void;
}

export function useBackendPanelHandlers(): BackendPanelHandlers {
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

  // Every close handler flips its show flag off, drops the activeCommand
  // wrapper, and clears any half-typed slash. `returnToSettings` additionally
  // honors settingsReturnOnEscape so a panel opened from /settings bounces back
  // there. makeClose is called unconditionally and in fixed order each render
  // (it wraps useCallback), so React hook ordering holds.
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

  const handleCloseContextBreakdown = makeClose(setShowContextBreakdown);
  const handleCloseHelpPanel = makeClose(setShowHelpPanel);
  const handleCloseUsagePanel = makeClose(setShowUsagePanel);
  const handleCloseMcpPanel = makeClose(setShowMcpPanel);
  const handleCloseToolsPanel = makeClose(setShowToolsPanel);
  const handleCloseStatsPanel = makeClose(setShowStatsPanel);
  const handleCloseHooksPanel = makeClose(setShowHooksPanel);
  const handleCloseKnowledgePanel = makeClose(setShowKnowledgePanel);
  const handleCloseCodePanel = makeClose(setShowCodePanel);
  const handleCloseChangelogPanel = makeClose(setShowChangelogPanel);
  const handleCloseRewindExplorer = makeClose(setShowRewindExplorer);
  const handleCloseKeybindingsPanel = makeClose(setShowKeybindingsPanel, {
    returnToSettings: true,
  });
  const handleCloseDisplaySettingsPanel = makeClose(
    setShowDisplaySettingsPanel,
    { returnToSettings: true }
  );
  const handleCloseThemePanel = makeClose(setShowThemePanel, {
    returnToSettings: true,
  });

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
    handleCloseContextBreakdown,
    handleCloseUsagePanel,
    handleCloseHelpPanel,
    handleCloseMcpPanel,
    handleCloseToolsPanel,
    handleCloseStatsPanel,
    handleCloseHooksPanel,
    handleCloseKnowledgePanel,
    handleCloseCodePanel,
    handleCloseChangelogPanel,
    handleCloseRewindExplorer,
    handleCloseKeybindingsPanel,
    handleCloseDisplaySettingsPanel,
    handleCloseThemePanel,
    handleCloseSettingsPanel,
    handleTabFromContext,
    handleTabFromUsage,
    handleRefreshCodePanel,
    handleRewindSelect,
  };
}

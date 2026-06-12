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

  const handleCloseContextBreakdown = useCallback(() => {
    setShowContextBreakdown(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowContextBreakdown, setActiveCommand, clearCommandInput]);

  const handleCloseHelpPanel = useCallback(() => {
    setShowHelpPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowHelpPanel, setActiveCommand, clearCommandInput]);

  const handleCloseUsagePanel = useCallback(() => {
    setShowUsagePanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowUsagePanel, setActiveCommand, clearCommandInput]);

  const handleCloseMcpPanel = useCallback(() => {
    setShowMcpPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowMcpPanel, setActiveCommand, clearCommandInput]);

  const handleCloseToolsPanel = useCallback(() => {
    setShowToolsPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowToolsPanel, setActiveCommand, clearCommandInput]);

  const handleCloseStatsPanel = useCallback(() => {
    setShowStatsPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowStatsPanel, setActiveCommand, clearCommandInput]);

  const handleCloseHooksPanel = useCallback(() => {
    setShowHooksPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowHooksPanel, setActiveCommand, clearCommandInput]);

  const handleCloseKnowledgePanel = useCallback(() => {
    setShowKnowledgePanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowKnowledgePanel, setActiveCommand, clearCommandInput]);

  const handleCloseCodePanel = useCallback(() => {
    setShowCodePanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowCodePanel, setActiveCommand, clearCommandInput]);

  const handleCloseChangelogPanel = useCallback(() => {
    setShowChangelogPanel(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowChangelogPanel, setActiveCommand, clearCommandInput]);

  const handleCloseRewindExplorer = useCallback(() => {
    setShowRewindExplorer(false);
    setActiveCommand(null);
    clearCommandInput();
  }, [setShowRewindExplorer, setActiveCommand, clearCommandInput]);

  const handleCloseKeybindingsPanel = useCallback(() => {
    setShowKeybindingsPanel(false);
    setActiveCommand(null);
    clearCommandInput();
    if (settingsReturnOnEscape) {
      setSettingsReturnOnEscape(false);
      reopenSettingsMenu();
    }
  }, [
    setShowKeybindingsPanel,
    setActiveCommand,
    clearCommandInput,
    settingsReturnOnEscape,
    setSettingsReturnOnEscape,
    reopenSettingsMenu,
  ]);

  const handleCloseDisplaySettingsPanel = useCallback(() => {
    setShowDisplaySettingsPanel(false);
    setActiveCommand(null);
    clearCommandInput();
    if (settingsReturnOnEscape) {
      setSettingsReturnOnEscape(false);
      reopenSettingsMenu();
    }
  }, [
    setShowDisplaySettingsPanel,
    setActiveCommand,
    clearCommandInput,
    settingsReturnOnEscape,
    setSettingsReturnOnEscape,
    reopenSettingsMenu,
  ]);

  const handleCloseThemePanel = useCallback(() => {
    setShowThemePanel(false);
    setActiveCommand(null);
    clearCommandInput();
    // Return to /settings menu if this panel was opened from there.
    if (settingsReturnOnEscape) {
      setSettingsReturnOnEscape(false);
      reopenSettingsMenu();
    }
  }, [
    setShowThemePanel,
    setActiveCommand,
    clearCommandInput,
    settingsReturnOnEscape,
    setSettingsReturnOnEscape,
    reopenSettingsMenu,
  ]);

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

/**
 * /settings overlay.
 *
 * Built on `<Explorer>` (the same selectable-overlay primitive as
 * /rewind and /settings → theme), this is the top-level picker for
 * preference flows. Each row dispatches to either:
 *
 *   - another panel (theme, keybindings, display) — opened via store
 *     setters with `settingsReturnOnEscape=true` so ESC walks the user
 *     back here rather than dismissing the whole overlay.
 *   - an inline async action (terminal) — runs setup, surfaces the
 *     result as a transient alert.
 *   - a sub-screen (history) — Explorer is re-mounted with a different
 *     row set; ESC backs out to the top screen rather than closing.
 *
 * Tracks `screen` in local state to drive the history sub-screen. CLI
 * args (`/settings <sub>`) bypass this panel and route through
 * `settings-subcommands.ts` handlers in the dispatcher.
 */

import React, { useCallback, useMemo, useState } from 'react';
import { Explorer, type ExplorerRow } from './Explorer.js';
import { useAppStore } from '../../stores/app-store.js';
import { Settings } from '../../constants/settings.js';
import {
  readCliSettings,
  readStringSetting,
  writeCliSettings,
} from '../../utils/cli-settings.js';
import { setupTerminal } from '../../utils/terminal-setup.js';

interface SettingsPanelProps {
  onClose: () => void;
}

type Screen = { type: 'top' } | { type: 'history' };

type TopChoice = 'display' | 'theme' | 'terminal' | 'keybindings' | 'history';
type HistoryChoice = 'session' | 'global';

interface TopItem {
  id: TopChoice;
  label: string;
  description: string;
}

const TOP_ITEMS: readonly TopItem[] = [
  {
    id: 'display',
    label: 'Display',
    description: 'Control animations, ASCII art, and icons',
  },
  {
    id: 'theme',
    label: 'Theme',
    description: 'Colors, prompt style, diff styling',
  },
  {
    id: 'terminal',
    label: 'Terminal',
    description: 'Shift+Enter / Option+Enter for newlines',
  },
  {
    id: 'keybindings',
    label: 'Keybindings',
    description: 'Customize keyboard shortcuts',
  },
  {
    id: 'history',
    label: 'History',
    description: 'Prompt history scope (session or global)',
  },
];

export const SettingsPanel: React.FC<SettingsPanelProps> = ({ onClose }) => {
  const setShowSettingsPanel = useAppStore(
    (state) => state.setShowSettingsPanel
  );
  const setSettingsReturnOnEscape = useAppStore(
    (state) => state.setSettingsReturnOnEscape
  );
  const setShowDisplaySettingsPanel = useAppStore(
    (state) => state.setShowDisplaySettingsPanel
  );
  const setShowThemePanel = useAppStore((state) => state.setShowThemePanel);
  const setShowKeybindingsPanel = useAppStore(
    (state) => state.setShowKeybindingsPanel
  );
  const setLoadingMessage = useAppStore((state) => state.setLoadingMessage);
  const showAlert = useAppStore((state) => state.showTransientAlert);
  const kiro = useAppStore((state) => state.kiro);

  const [screen, setScreen] = useState<Screen>({ type: 'top' });
  const screenKey = screen.type;

  // ─── Action helpers ─────────────────────────────────────────────
  const openSubPanel = useCallback(
    (open: () => void) => {
      // Sub-panel takes over: close /settings overlay first, prime the
      // back-flag so ESC inside the sub-panel returns here rather than
      // dismissing the whole overlay stack.
      setSettingsReturnOnEscape(true);
      setShowSettingsPanel(false);
      open();
    },
    [setSettingsReturnOnEscape, setShowSettingsPanel]
  );

  const runTerminalSetup = useCallback(async () => {
    setLoadingMessage('Configuring terminal…');
    try {
      const result = await setupTerminal();
      setLoadingMessage(null);
      if (result.message) {
        showAlert({
          message: result.message,
          status: result.success ? 'success' : 'error',
          autoHideMs: alertDurationFor(result.message),
        });
      }
    } catch (error) {
      setLoadingMessage(null);
      const message =
        error instanceof Error ? error.message : 'Terminal setup failed';
      showAlert({
        message,
        status: 'error',
        autoHideMs: alertDurationFor(message),
      });
    }
  }, [setLoadingMessage, showAlert]);

  const applyHistoryMode = useCallback(
    async (mode: HistoryChoice) => {
      const settings = readCliSettings();
      settings[Settings.CHAT_HISTORY_MODE] = mode;
      writeCliSettings(settings);
      await kiro.setSetting(Settings.CHAT_HISTORY_MODE, mode).catch(() => {});
      showAlert({
        message:
          mode === 'session'
            ? 'History: per-session (takes effect next session)'
            : 'History: global (takes effect next session)',
        status: 'success',
        autoHideMs: 5000,
      });
      onClose();
    },
    [kiro, showAlert, onClose]
  );

  // ─── Rows ──────────────────────────────────────────────────────
  const rows: ExplorerRow[] = useMemo(() => {
    if (screen.type === 'top') {
      return TOP_ITEMS.map((item) => ({
        id: item.id,
        values: { label: item.label, description: item.description },
      }));
    }
    // History sub-screen.
    // The active marker uses a `●` dot suffix on the label rather than
    // theme's `[active]` description-column marker — both rows here have
    // their own descriptions ("Each session has its own…" / "All sessions
    // share one…"), so we can't fold the active marker into the
    // description column without losing that text.
    const current = readStringSetting(Settings.CHAT_HISTORY_MODE, 'session');
    return [
      {
        id: 'session',
        values: {
          label: current === 'session' ? 'Session ●' : 'Session',
          description: 'Each session has its own prompt history',
        },
      },
      {
        id: 'global',
        values: {
          label: current === 'global' ? 'Global ●' : 'Global',
          description: 'All sessions share one prompt history',
        },
      },
    ];
  }, [screen]);

  // ─── Selection ──────────────────────────────────────────────────
  const handleTopSelect = useCallback(
    (id: TopChoice) => {
      switch (id) {
        case 'display':
          openSubPanel(() => setShowDisplaySettingsPanel(true));
          return;
        case 'theme':
          openSubPanel(() => setShowThemePanel(true));
          return;
        case 'keybindings':
          openSubPanel(() => setShowKeybindingsPanel(true));
          return;
        case 'terminal':
          // Terminal setup is a self-contained async flow — close the
          // overlay first so the user sees the resulting alert, then run.
          onClose();
          void runTerminalSetup();
          return;
        case 'history':
          setScreen({ type: 'history' });
          return;
      }
    },
    [
      openSubPanel,
      setShowDisplaySettingsPanel,
      setShowThemePanel,
      setShowKeybindingsPanel,
      onClose,
      runTerminalSetup,
    ]
  );

  const handleSelect = useCallback(
    (row: ExplorerRow) => {
      if (screen.type === 'top') {
        handleTopSelect(row.id as TopChoice);
      } else {
        void applyHistoryMode(row.id as HistoryChoice);
      }
    },
    [screen, handleTopSelect, applyHistoryMode]
  );

  // ─── Back-navigation ────────────────────────────────────────────
  // ESC backs the user out one level. From the top screen we close the
  // overlay; from the history sub-screen we return to the top.
  const handleEsc = useCallback(() => {
    if (screen.type === 'history') {
      setScreen({ type: 'top' });
      return;
    }
    onClose();
  }, [screen, onClose]);

  // ─── Render ─────────────────────────────────────────────────────
  const isTop = screen.type === 'top';
  const title = isTop ? '/settings' : '/settings – history';
  // Top-level /settings has no subtitle per spec — the items speak for
  // themselves. Sub-screens (e.g. history) get a short prompt because
  // they're a deeper navigation step the user just landed on.
  const description = isTop
    ? undefined
    : 'Choose where prompt history is stored';

  // Both screens use the same [label, description] layout. The history
  // sub-screen folds the [active] marker into the description column so
  // it sits in the same column as the description text on non-active
  // rows — matching the spec's "[active] aligns with the description"
  // layout instead of getting pushed to the right edge.
  const columns = [
    { key: 'label', label: '' },
    { key: 'description', label: '' },
  ];

  return (
    <Explorer
      key={screenKey}
      title={title}
      description={description}
      columns={columns}
      rows={rows}
      searchable={false}
      closeHintLabel={isTop ? 'to cancel' : 'to go back'}
      onSelect={handleSelect}
      onClose={handleEsc}
    />
  );
};

/**
 * Pick an alert duration based on message length. Mirrors the ctx-based
 * helper in settings-subcommands.ts so terminal setup output fits on
 * screen long enough to read.
 */
function alertDurationFor(message: string): number {
  return message.length > 180 ? 10000 : 5000;
}

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
 *   - an inline async action (terminal → newlines) — runs setup,
 *     surfaces the result as a transient alert.
 *   - a sub-screen (terminal, terminal → interrupt, history) — Explorer
 *     is re-mounted with a different row set; ESC backs out one level
 *     rather than closing.
 *
 * The menu structure, row contents, selection routing, and ESC
 * back-navigation are pure functions in `settings-panel-model.ts` so
 * they can be unit tested without rendering. This component is the thin
 * shell that wires those decisions to the store/effects and tracks the
 * current `screen` in local state. CLI args (`/settings <sub>`) bypass
 * this panel and route through `settings-subcommands.ts` handlers.
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
import { useGlyphs } from '../../hooks/useGlyphs.js';
import {
  type Screen,
  type HistoryChoice,
  type InterruptChoice,
  type PanelAction,
  DEFAULT_INTERRUPT_MODE,
  buildRows,
  resolveSelect,
  resolveBack,
  appliesOnSelect,
  screenTitle,
  screenDescription,
} from './settings-panel-model.js';

interface SettingsPanelProps {
  onClose: () => void;
}

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
  const handleUserInput = useAppStore((state) => state.handleUserInput);
  const glyphs = useGlyphs();

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

  const applyInterruptMode = useCallback(
    (mode: InterruptChoice) => {
      const settings = readCliSettings();
      settings[Settings.CHAT_DEFAULT_INTERRUPT_BEHAVIOR] = mode;
      writeCliSettings(settings);
      showAlert({
        message:
          mode === 'steer'
            ? 'Interrupt behaviour: steer (takes effect next session)'
            : 'Interrupt behaviour: queue (takes effect next session)',
        status: 'success',
        autoHideMs: 5000,
      });
      onClose();
    },
    [showAlert, onClose]
  );

  // Run a named action produced by `resolveSelect`. Keeping the
  // store/effect wiring here means the routing logic stays pure and
  // testable in `settings-panel-model.ts`.
  const runAction = useCallback(
    (action: PanelAction) => {
      switch (action.type) {
        case 'open-panel':
          if (action.panel === 'display') {
            openSubPanel(() => setShowDisplaySettingsPanel(true));
          } else if (action.panel === 'theme') {
            openSubPanel(() => setShowThemePanel(true));
          } else {
            openSubPanel(() => setShowKeybindingsPanel(true));
          }
          return;
        case 'open-verbosity':
          // Lite-only: close this panel, prime the back-flag so ESC out of the
          // verbosity command-menu returns here (CommandMenu reads
          // settingsReturnOnEscape → reopenSettingsMenu), then dispatch
          // /verbosity through the normal command pipeline.
          setSettingsReturnOnEscape(true);
          setShowSettingsPanel(false);
          void handleUserInput('/verbosity');
          return;
        case 'run-terminal-setup':
          // Terminal setup is a self-contained async flow — close the
          // overlay first so the user sees the resulting alert, then run.
          onClose();
          void runTerminalSetup();
          return;
        case 'apply-history':
          void applyHistoryMode(action.mode);
          return;
        case 'apply-interrupt':
          applyInterruptMode(action.mode);
          return;
      }
    },
    [
      openSubPanel,
      setShowDisplaySettingsPanel,
      setShowThemePanel,
      setShowKeybindingsPanel,
      setShowSettingsPanel,
      setSettingsReturnOnEscape,
      handleUserInput,
      onClose,
      runTerminalSetup,
      applyHistoryMode,
      applyInterruptMode,
    ]
  );

  // ─── Rows ──────────────────────────────────────────────────────
  // Side effect (reading persisted settings) lives here; `buildRows`
  // itself is pure and takes the resolved snapshot.
  const rows: ExplorerRow[] = useMemo(
    () =>
      buildRows(
        screen,
        {
          historyMode: readStringSetting(Settings.CHAT_HISTORY_MODE, 'session'),
          interruptMode: readStringSetting(
            Settings.CHAT_DEFAULT_INTERRUPT_BEHAVIOR,
            DEFAULT_INTERRUPT_MODE
          ),
        },
        process.env.KIRO_LITE_ROLLOUT_ENABLED === '1',
        glyphs.dotFilled
      ),
    [screen, glyphs.dotFilled]
  );

  // ─── Selection ──────────────────────────────────────────────────
  const handleSelect = useCallback(
    (row: ExplorerRow) => {
      const result = resolveSelect(screen, row.id);
      if (!result) return;
      if (result.kind === 'navigate') {
        setScreen(result.screen);
      } else {
        runAction(result.action);
      }
    },
    [screen, runAction]
  );

  // ─── Back-navigation ────────────────────────────────────────────
  // ESC backs the user out one level. From the top screen we close the
  // overlay; sub-screens return to their parent.
  const handleEsc = useCallback(() => {
    const back = resolveBack(screen);
    if (back === 'close') {
      onClose();
    } else {
      setScreen(back);
    }
  }, [screen, onClose]);

  // ─── Render ─────────────────────────────────────────────────────
  const isTop = screen.type === 'top';
  const title = screenTitle(screen);
  const description = screenDescription(screen);

  // Both screens use the same [label, description] layout. The active
  // marker is folded into the label column (with a `●` suffix) by
  // `buildRows`.
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
      keyHints={
        appliesOnSelect(screen)
          ? [
              // Rows apply immediately and dismiss the overlay — surface
              // that in the footer rather than the generic "select".
              {
                key: `${glyphs.arrowUp}${glyphs.arrowDown}`,
                label: 'to navigate',
              },
              { key: glyphs.enter, label: 'to apply and close' },
            ]
          : undefined // Use Explorer's defaults: navigate · select.
      }
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

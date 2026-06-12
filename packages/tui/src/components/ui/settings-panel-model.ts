/**
 * Pure navigation + row model for the `/settings` overlay (`SettingsPanel`).
 *
 * The visual panel (`SettingsPanel.tsx`) is hard to unit test directly, so the
 * menu structure, row contents, selection routing, and ESC back-navigation all
 * live here as pure functions. The component is a thin shell that renders
 * {@link buildRows}, dispatches {@link resolveSelect}, and walks back with
 * {@link resolveBack}.
 *
 * This is the layer that regressed once before: selecting **Terminal** jumped
 * straight into the newlines setup and the **interrupt behaviour** option
 * became unreachable from the panel. The unit tests around these functions
 * exist to catch exactly that — every menu entry must remain reachable.
 */

import type { ExplorerRow } from './Explorer.js';
import {
  Settings,
  DISPLAY_SETTINGS_DESCRIPTION,
} from '../../constants/settings.js';
import {
  InterruptMode,
  DEFAULT_INTERRUPT_MODE,
} from '../../constants/interrupt-mode.js';

/** Screens (top-level + sub-screens) the panel can display. */
export type Screen =
  | { type: 'top' }
  | { type: 'history' }
  | { type: 'terminal' }
  | { type: 'terminal:interrupt' };

export type ScreenType = Screen['type'];

export type TopChoice =
  | 'display'
  | 'verbosity'
  | 'theme'
  | 'terminal'
  | 'keybindings'
  | 'history';

/** UI mode — gates lite-only rows (e.g. verbosity) in {@link buildRows}. */
export type UiMode = 'tui' | 'lite';
export type HistoryChoice = 'session' | 'global';
export type TerminalChoice = 'newlines' | 'interrupt';
export type InterruptChoice = 'steer' | 'queue';

/**
 * The current persisted setting values the row builders need to render the
 * active (`●`) marker. Reading the settings file is a side effect, so callers
 * pass the resolved values in to keep {@link buildRows} pure.
 */
export interface SettingsSnapshot {
  /** `chat.historyMode` — defaults to `'session'`. */
  historyMode: string;
  /** `chat.defaultInterruptBehavior` — defaults to {@link DEFAULT_INTERRUPT_MODE}. */
  interruptMode: string;
}

/**
 * Outcome of selecting a row. Either we navigate to another screen within the
 * panel, or we fire a named action the component knows how to run.
 */
export type SelectResult =
  | { kind: 'navigate'; screen: Screen }
  | { kind: 'action'; action: PanelAction };

/** Named side-effecting actions the component wires up to store/effects. */
export type PanelAction =
  // Open a different overlay (theme/keybindings/display) — ESC returns here.
  | { type: 'open-panel'; panel: 'display' | 'theme' | 'keybindings' }
  // Open the lite-only /verbosity command-menu (dispatched via handleUserInput);
  // ESC out of that menu returns here. Lite-only row.
  | { type: 'open-verbosity' }
  // Run the async terminal newline setup flow, then close.
  | { type: 'run-terminal-setup' }
  // Persist the history scope, then close.
  | { type: 'apply-history'; mode: HistoryChoice }
  // Persist the default interrupt behaviour, then close.
  | { type: 'apply-interrupt'; mode: InterruptChoice };

/**
 * Per-screen metadata, declared once so the screen names aren't repeated
 * across the title/description/back/footer helpers below. Add a screen here
 * and the helpers pick it up automatically.
 */
export interface ScreenConfig {
  /** Breadcrumb title shown in the panel header. */
  title: string;
  /** Sub-screen prompt under the title; `undefined` for the top screen. */
  description?: string;
  /**
   * Whether selecting a row on this screen applies a setting and immediately
   * dismisses the overlay (drives the "apply and close" footer hint).
   */
  appliesOnSelect: boolean;
  /**
   * Where ESC goes from this screen: the parent {@link ScreenType}, or
   * `'close'` to dismiss the overlay entirely (top screen only).
   */
  back: ScreenType | 'close';
}

export const SCREEN_CONFIG: Record<ScreenType, ScreenConfig> = {
  top: {
    title: '/settings',
    // Top-level /settings has no subtitle per spec — the items speak for
    // themselves.
    description: undefined,
    appliesOnSelect: false,
    back: 'close',
  },
  terminal: {
    title: '/settings – terminal',
    description: 'Configure terminal preferences',
    appliesOnSelect: false,
    back: 'top',
  },
  'terminal:interrupt': {
    title: '/settings – interrupt behaviour',
    description: 'Choose what happens when you type while Kiro is working',
    appliesOnSelect: true,
    back: 'terminal',
  },
  history: {
    title: '/settings – history',
    description: 'Choose where prompt history is stored',
    appliesOnSelect: true,
    back: 'top',
  },
};

export interface TopItem {
  id: TopChoice;
  label: string;
  description: string;
}

/**
 * Top-level menu rows shown in BOTH modes.
 *
 * The lite-only `verbosity` row is NOT here — it's spliced in by
 * {@link buildRows} when `uiMode === 'lite'` (see {@link VERBOSITY_ITEM}), so
 * TUI users never see a row whose handler errors with "lite mode only".
 */
export const TOP_ITEMS: readonly TopItem[] = [
  {
    id: 'display',
    label: 'Display',
    description: DISPLAY_SETTINGS_DESCRIPTION,
  },
  {
    id: 'theme',
    label: 'Theme',
    description: 'Colors, prompt style, diff styling',
  },
  {
    id: 'terminal',
    label: 'Terminal',
    description: 'Newlines, interrupt behaviour',
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

/**
 * Lite-only verbosity row, inserted right after Display in lite mode. Its
 * renderer controls only run inside <LiteLayout>, so it must not surface in
 * TUI. Selecting it opens the rich /verbosity command-menu (see the
 * `open-verbosity` action in {@link resolveSelect}).
 */
export const VERBOSITY_ITEM: TopItem = {
  id: 'verbosity',
  label: 'Verbosity',
  description: 'Tool args, reasoning, output filters, density (lite mode only)',
};

/** Terminal sub-screen rows. */
export const TERMINAL_ITEMS: readonly {
  id: TerminalChoice;
  label: string;
  description: string;
}[] = [
  {
    id: 'newlines',
    label: 'Newlines',
    description: 'Shift+Enter / Option+Enter for newlines',
  },
  {
    id: 'interrupt',
    label: 'Default Interrupt behaviour',
    description: 'What happens when you type while Kiro is working',
  },
];

/** Append the active-marker dot when `isActive`. */
function withActiveMarker(label: string, isActive: boolean): string {
  return isActive ? `${label} ●` : label;
}

/**
 * Build the Explorer rows for the given screen. Pure: the active marker is
 * derived from the passed-in {@link SettingsSnapshot} rather than read from
 * disk here.
 */
export function buildRows(
  screen: Screen,
  settings: SettingsSnapshot,
  uiMode?: UiMode
): ExplorerRow[] {
  switch (screen.type) {
    case 'top': {
      // Splice the lite-only verbosity row in after Display; TUI never sees it.
      const topItems =
        uiMode === 'lite'
          ? [TOP_ITEMS[0]!, VERBOSITY_ITEM, ...TOP_ITEMS.slice(1)]
          : TOP_ITEMS;
      return topItems.map((item) => ({
        id: item.id,
        values: { label: item.label, description: item.description },
      }));
    }
    case 'terminal':
      return TERMINAL_ITEMS.map((item) => ({
        id: item.id,
        values: { label: item.label, description: item.description },
      }));
    case 'terminal:interrupt':
      return [
        {
          id: 'steer',
          values: {
            label: withActiveMarker(
              'Steer',
              settings.interruptMode === InterruptMode.STEER
            ),
            description: 'Inject your message mid-turn at tool boundaries',
          },
        },
        {
          id: 'queue',
          values: {
            label: withActiveMarker(
              'Queue',
              settings.interruptMode === InterruptMode.QUEUE
            ),
            description: 'Buffer your message and send after turn ends',
          },
        },
      ];
    case 'history':
      return [
        {
          id: 'session',
          values: {
            label: withActiveMarker(
              'Session',
              settings.historyMode === 'session'
            ),
            description: 'Each session has its own prompt history',
          },
        },
        {
          id: 'global',
          values: {
            label: withActiveMarker(
              'Global',
              settings.historyMode === 'global'
            ),
            description: 'All sessions share one prompt history',
          },
        },
      ];
  }
}

/** Route a row selection to the next screen or a named action. */
export function resolveSelect(screen: Screen, id: string): SelectResult | null {
  switch (screen.type) {
    case 'top':
      switch (id as TopChoice) {
        case 'display':
          return {
            kind: 'action',
            action: { type: 'open-panel', panel: 'display' },
          };
        case 'verbosity':
          return { kind: 'action', action: { type: 'open-verbosity' } };
        case 'theme':
          return {
            kind: 'action',
            action: { type: 'open-panel', panel: 'theme' },
          };
        case 'keybindings':
          return {
            kind: 'action',
            action: { type: 'open-panel', panel: 'keybindings' },
          };
        case 'terminal':
          return { kind: 'navigate', screen: { type: 'terminal' } };
        case 'history':
          return { kind: 'navigate', screen: { type: 'history' } };
        default:
          return null;
      }
    case 'terminal':
      switch (id as TerminalChoice) {
        case 'newlines':
          return { kind: 'action', action: { type: 'run-terminal-setup' } };
        case 'interrupt':
          return { kind: 'navigate', screen: { type: 'terminal:interrupt' } };
        default:
          return null;
      }
    case 'terminal:interrupt':
      if (id === 'steer' || id === 'queue') {
        return {
          kind: 'action',
          action: { type: 'apply-interrupt', mode: id },
        };
      }
      return null;
    case 'history':
      if (id === 'session' || id === 'global') {
        return { kind: 'action', action: { type: 'apply-history', mode: id } };
      }
      return null;
  }
}

/**
 * Resolve ESC back-navigation. Returns the screen to move to, or `'close'`
 * when the overlay should dismiss entirely (only from the top screen).
 */
export function resolveBack(screen: Screen): Screen | 'close' {
  const back = SCREEN_CONFIG[screen.type].back;
  return back === 'close' ? 'close' : { type: back };
}

/**
 * Whether selecting a row on this screen applies a setting and immediately
 * dismisses the overlay (drives the "apply and close" footer hint).
 */
export function appliesOnSelect(screen: Screen): boolean {
  return SCREEN_CONFIG[screen.type].appliesOnSelect;
}

/** Breadcrumb title for the given screen. */
export function screenTitle(screen: Screen): string {
  return SCREEN_CONFIG[screen.type].title;
}

/**
 * Breadcrumb title for the lite `/verbosity` menu, derived from the menu's
 * `previewKey`. Verbosity is rendered by CommandMenu (not the Explorer-based
 * SettingsPanel) because of its live preview + truncation editor, so it can't
 * use {@link screenTitle}; this maps each sub-screen to the same
 * `/settings – verbosity – <sub>` breadcrumb the other settings show in their
 * panel header, deepening one level per drilldown.
 *
 * `previewKey` values come from the verbosity effect's `openMenuWith(..., key)`
 * calls: `top` (root), `density`, `tool`, `subagent`, `output`, `truncation`
 * (and `truncation:args` / `truncation:output`), plus the numeric-editor keys
 * `truncation:<field>:edit`. Unknown / undefined keys fall back to the root
 * breadcrumb so the header never reads as a bare command name.
 */
export function verbosityBreadcrumb(previewKey?: string): string {
  const ROOT = '/settings – verbosity';
  if (!previewKey || previewKey === 'top') return ROOT;
  const SUB_LABEL: Record<string, string> = {
    density: 'density',
    tool: 'tool calls',
    subagent: 'subagent',
    output: 'output',
    truncation: 'truncation',
    'truncation:args': 'truncation',
    'truncation:output': 'truncation',
  };
  // Numeric-editor keys look like `truncation:argsLines:edit` — they live under
  // the truncation screen, so show the truncation breadcrumb.
  if (/^truncation:.*:edit$/.test(previewKey)) {
    return `${ROOT} – truncation`;
  }
  const sub = SUB_LABEL[previewKey];
  return sub ? `${ROOT} – ${sub}` : ROOT;
}

/** Sub-screen prompt shown under the title (top screen has none). */
export function screenDescription(screen: Screen): string | undefined {
  return SCREEN_CONFIG[screen.type].description;
}

/** Setting keys the panel reads, re-exported for the component + tests. */
export const PANEL_SETTING_KEYS = {
  historyMode: Settings.CHAT_HISTORY_MODE,
  interruptMode: Settings.CHAT_DEFAULT_INTERRUPT_BEHAVIOR,
} as const;

/** The default interrupt mode token, re-exported for convenience. */
export { DEFAULT_INTERRUPT_MODE };

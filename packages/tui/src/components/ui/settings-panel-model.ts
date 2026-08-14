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
import { DISPLAY_SETTINGS_DESCRIPTION } from '../../constants/settings.js';
import {
  InterruptMode,
  DEFAULT_INTERRUPT_MODE,
} from '../../constants/interrupt-mode.js';

/** Screens (top-level + sub-screens) the panel can display. */
export type Screen =
  | { type: 'top' }
  | { type: 'features' }
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
  | 'history'
  | 'features';

export type HistoryChoice = 'session' | 'global';
export type TerminalChoice = 'newlines' | 'interrupt';
export type InterruptChoice = 'steer' | 'queue';
export type FeaturesChoice = 'workflows';

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
  /** Whether the workflows rollout reaches this user. Gates the Features row. */
  workflowsAvailable: boolean;
  /** `chat.enableWorkflows` — opt-in, so `false` until the user turns it on. */
  workflowsEnabled: boolean;
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
  // Open the /verbosity command-menu (dispatched via handleUserInput); ESC out
  // of that menu returns here. Available in both lite and TUI.
  | { type: 'open-verbosity' }
  // Run the async terminal newline setup flow, then close.
  | { type: 'run-terminal-setup' }
  // Persist the history scope, then close.
  | { type: 'apply-history'; mode: HistoryChoice }
  // Persist the default interrupt behaviour, then close.
  | { type: 'apply-interrupt'; mode: InterruptChoice }
  // Toggle a feature setting (persists to cli.json, stays on screen).
  | { type: 'toggle-feature'; feature: FeaturesChoice };

/**
 * Per-screen metadata, declared once so the screen names aren't repeated
 * across the title/description/back/footer helpers below. Add a screen here
 * and the helpers pick it up automatically.
 */
export interface ScreenConfig {
  title: string;
  /** `undefined` for the top screen (no subtitle). */
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
  features: {
    title: '/settings – features',
    description: 'Toggle experimental and preview features',
    appliesOnSelect: false,
    back: 'top',
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

/** Top-level rows shown in BOTH modes. `verbosity` is spliced in after Display
 *  by {@link buildRows} (see {@link VERBOSITY_ITEM}) in both modes. */
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

/** Verbosity row, inserted after Display. Available in both lite and TUI —
 *  the /verbosity menu and its rendering controls are peers in both. */
export const VERBOSITY_ITEM: TopItem = {
  id: 'verbosity',
  label: 'Verbosity',
  description:
    'Tool args, reasoning, output filters, density, subagent sections',
};

/** Features row — only shown to users the workflows rollout reaches. */
export const FEATURES_ITEM: TopItem = {
  id: 'features',
  label: 'Features',
  description: 'Experimental and preview feature toggles',
};

/** Features sub-screen rows. */
export interface FeatureItem {
  id: FeaturesChoice;
  label: string;
  description: string;
}

export const FEATURES_ITEMS: readonly FeatureItem[] = [
  {
    id: 'workflows',
    label: 'Workflows',
    description:
      'Enables multi-step asynchronous workflows with reusable recipes',
  },
];

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
function withActiveMarker(
  label: string,
  isActive: boolean,
  dot: string
): string {
  return isActive ? `${label} ${dot}` : label;
}

/**
 * Build the Explorer rows for the given screen. Pure: the active marker is
 * derived from the passed-in {@link SettingsSnapshot} rather than read from
 * disk here.
 */
export function buildRows(
  screen: Screen,
  settings: SettingsSnapshot,
  rolloutEnabled: boolean = true,
  dotFilled: string = '●'
): ExplorerRow[] {
  switch (screen.type) {
    case 'top': {
      // Verbosity row splices in after Display. Always present in lite (lite
      // implies the rollout); on the TUI only inside the cohort — the port is
      // invisible off-rollout, matching /verbosity's command gate.
      const topItems = rolloutEnabled
        ? [TOP_ITEMS[0]!, VERBOSITY_ITEM, ...TOP_ITEMS.slice(1)]
        : TOP_ITEMS;
      // Features row is only shown when the workflows rollout reaches the user.
      const withFeatures = settings.workflowsAvailable
        ? [...topItems, FEATURES_ITEM]
        : topItems;
      return withFeatures.map((item) => ({
        id: item.id,
        values: {
          label: item.label,
          // "Default UI at startup" is rollout-gated (selectDisplayItems); drop
          // it from the preview too when off the cohort.
          description:
            item.id === 'display' && !rolloutEnabled
              ? 'Animations, ASCII art, icons, and thinking'
              : item.description,
        },
      }));
    }
    case 'features':
      return FEATURES_ITEMS.map((item) => ({
        id: item.id,
        values: {
          label: `${item.label} ${settings.workflowsEnabled ? 'on' : 'off'}`,
          description: item.description,
        },
      }));
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
              settings.interruptMode === InterruptMode.STEER,
              dotFilled
            ),
            description: 'Inject your message mid-turn at tool boundaries',
          },
        },
        {
          id: 'queue',
          values: {
            label: withActiveMarker(
              'Queue',
              settings.interruptMode === InterruptMode.QUEUE,
              dotFilled
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
              settings.historyMode === 'session',
              dotFilled
            ),
            description: 'Each session has its own prompt history',
          },
        },
        {
          id: 'global',
          values: {
            label: withActiveMarker(
              'Global',
              settings.historyMode === 'global',
              dotFilled
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
        case 'features':
          return { kind: 'navigate', screen: { type: 'features' } };
        default:
          return null;
      }
    case 'features':
      if (id === 'workflows') {
        return {
          kind: 'action',
          action: { type: 'toggle-feature', feature: 'workflows' },
        };
      }
      return null;
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

export function appliesOnSelect(screen: Screen): boolean {
  return SCREEN_CONFIG[screen.type].appliesOnSelect;
}

export function screenTitle(screen: Screen): string {
  return SCREEN_CONFIG[screen.type].title;
}

/** `/settings – verbosity – <sub>` breadcrumb for the lite /verbosity menu
 *  (CommandMenu renders it, not SettingsPanel). Unknown keys fall back to root. */
export function verbosityBreadcrumb(previewKey?: string): string {
  const ROOT = '/settings – verbosity';
  if (!previewKey || previewKey === 'top') return ROOT;
  // All truncation flavors collapse to the single truncation screen.
  if (previewKey === 'truncation' || previewKey.startsWith('truncation:')) {
    return `${ROOT} – truncation`;
  }
  const sub = VERBOSITY_BREADCRUMB_LABELS[previewKey];
  return sub ? `${ROOT} – ${sub}` : ROOT;
}

const VERBOSITY_BREADCRUMB_LABELS: Record<string, string> = {
  density: 'density',
  tool: 'tool calls',
  subagent: 'subagent',
  output: 'output',
};

export function screenDescription(screen: Screen): string | undefined {
  return SCREEN_CONFIG[screen.type].description;
}

/** The default interrupt mode token, re-exported for convenience. */
export { DEFAULT_INTERRUPT_MODE };

/**
 * User-configurable keybindings for V2 TUI.
 *
 * Settings are stored in the Rust settings store (see
 * `crates/chat-cli/src/database/settings.rs`) and surfaced to the TUI via
 * the ACP `listSettings()` call. The TUI reads the merged map from
 * `appStore.settings` and parses each binding through {@link parseKeybinding}.
 *
 * Syntax examples:
 *   "esc", "escape"
 *   "ctrl+c"
 *   "ctrl+shift+q"
 *   "alt+x", "meta+x"
 *
 * Invalid or unset values fall back to the binding's built-in default.
 */
import type { Key } from '../hooks/useKeypress.js';
import { Settings } from '../constants/settings.js';
import { logger } from './logger.js';
import { getActiveGlyphs } from '../hooks/useGlyphs.js';

/** Parsed, normalized keybinding. `key` is always lowercase. */
export interface Keybinding {
  ctrl: boolean;
  shift: boolean;
  meta: boolean;
  /** Named key (e.g. "escape", "tab") or single-char ascii (e.g. "c"). */
  key: string;
}

/** Binding slots exposed to the TUI. */
export type KeybindingName =
  | 'cancelStream'
  | 'closeMenu'
  | 'quit'
  | 'toggleInterruptMode'
  | 'toggleSessionDashboard';

const DEFAULTS: Record<KeybindingName, string> = {
  cancelStream: 'esc',
  closeMenu: 'esc',
  quit: 'ctrl+c',
  toggleInterruptMode: 'ctrl+s',
  toggleSessionDashboard: 'ctrl+e',
};

const SETTING_KEYS: Record<KeybindingName, string> = {
  cancelStream: Settings.CHAT_KEYBINDINGS_CANCEL_STREAM,
  closeMenu: Settings.CHAT_KEYBINDINGS_CLOSE_MENU,
  quit: Settings.CHAT_KEYBINDINGS_QUIT,
  toggleInterruptMode: Settings.CHAT_KEYBINDINGS_TOGGLE_INTERRUPT_BEHAVIOR,
  toggleSessionDashboard: Settings.CHAT_KEYBINDINGS_TOGGLE_SESSION_DASHBOARD,
};

// Key aliases: user-facing → canonical
const KEY_ALIASES: Record<string, string> = {
  esc: 'escape',
  return: 'enter',
  enter: 'enter',
  escape: 'escape',
  tab: 'tab',
  space: 'space',
  backspace: 'backspace',
  delete: 'delete',
  up: 'up',
  down: 'down',
  left: 'left',
  right: 'right',
  pageup: 'pageup',
  pagedown: 'pagedown',
  home: 'home',
  end: 'end',
};

/**
 * Parse a user-supplied keybinding string into a {@link Keybinding}.
 * Returns null when the string is not parseable.
 */
export function parseKeybinding(spec: string): Keybinding | null {
  if (typeof spec !== 'string') return null;
  const parts = spec
    .trim()
    .toLowerCase()
    .split('+')
    .map((p) => p.trim())
    .filter(Boolean);
  if (parts.length === 0) return null;

  let ctrl = false;
  let shift = false;
  let meta = false;
  let key: string | null = null;

  for (const part of parts) {
    if (part === 'ctrl' || part === 'control') {
      ctrl = true;
    } else if (part === 'shift') {
      shift = true;
    } else if (part === 'meta' || part === 'alt' || part === 'cmd') {
      meta = true;
    } else if (key !== null) {
      // Multiple non-modifier tokens — unsupported.
      return null;
    } else {
      key = KEY_ALIASES[part] ?? part;
    }
  }

  if (key === null) return null;
  // Sanity: reject multi-char keys that aren't a known named key.
  if (key.length > 1 && !Object.values(KEY_ALIASES).includes(key)) {
    return null;
  }
  return { ctrl, shift, meta, key };
}

export function keybindingsEqual(a: Keybinding, b: Keybinding): boolean {
  return (
    a.ctrl === b.ctrl &&
    a.shift === b.shift &&
    a.meta === b.meta &&
    a.key === b.key
  );
}

function isReservedDashboardBinding(binding: Keybinding): boolean {
  return (
    binding.ctrl &&
    !binding.meta &&
    ['c', 'd', 'g', 'y', 'z'].includes(binding.key)
  );
}

/**
 * Resolve a binding by name, falling back to the default when the setting is
 * missing, not a string, not parseable, or reserved by a global action.
 */
export function resolveKeybinding(
  settings: Record<string, unknown> | null | undefined,
  name: KeybindingName
): Keybinding {
  const raw = settings?.[SETTING_KEYS[name]];
  if (typeof raw === 'string') {
    const parsed = parseKeybinding(raw);
    if (
      parsed &&
      (name !== 'toggleSessionDashboard' || !isReservedDashboardBinding(parsed))
    ) {
      return parsed;
    }
    logger.warn(
      `[keybindings] Invalid or reserved binding for ${SETTING_KEYS[name]}: ${JSON.stringify(raw)}. Using default "${DEFAULTS[name]}".`
    );
  }
  // parseKeybinding on the default is guaranteed to succeed.
  return parseKeybinding(DEFAULTS[name])!;
}

/** Does the current keypress match the binding? */
export function matchesKeybinding(
  binding: Keybinding,
  input: string,
  key: Key
): boolean {
  if (binding.ctrl !== !!key.ctrl) return false;
  if (binding.meta !== !!key.meta) return false;
  // Shift is only checked when explicitly required; terminals don't reliably
  // report key.shift for plain ascii, so we stay permissive there.
  if (binding.shift && !key.shift) return false;

  switch (binding.key) {
    case 'escape':
      return !!key.escape;
    case 'enter':
      return !!key.return;
    case 'tab':
      return !!key.tab;
    case 'backspace':
      return !!key.backspace;
    case 'delete':
      return !!key.delete;
    case 'up':
      return !!key.upArrow;
    case 'down':
      return !!key.downArrow;
    case 'left':
      return !!key.leftArrow;
    case 'right':
      return !!key.rightArrow;
    case 'pageup':
      return !!key.pageUp;
    case 'pagedown':
      return !!key.pageDown;
    case 'home':
      return !!key.home;
    case 'end':
      return !!key.end;
    case 'space':
      return input === ' ';
    default:
      return input.toLowerCase() === binding.key;
  }
}

/**
 * Human-readable label for a binding, used in on-screen hints.
 * Examples: "esc", "Ctrl+C", "Ctrl+Shift+Q".
 */
export function formatKeybinding(binding: Keybinding): string {
  const parts: string[] = [];
  if (binding.ctrl) parts.push('Ctrl');
  if (binding.shift) parts.push('Shift');
  if (binding.meta) parts.push('Alt');

  const keyLabel = formatKey(binding.key);
  parts.push(keyLabel);
  // When there are no modifiers and the key is a named key, lowercase reads
  // better ("esc" instead of "Esc"); for modifier combos, title-case.
  if (parts.length === 1) return keyLabel.toLowerCase();
  return parts.join('+');
}

function formatKey(key: string): string {
  const glyphs = getActiveGlyphs();
  switch (key) {
    case 'escape':
      return 'Esc';
    case 'enter':
      return 'Enter';
    case 'tab':
      return 'Tab';
    case 'backspace':
      return 'Backspace';
    case 'delete':
      return 'Delete';
    case 'up':
      return glyphs.arrowUp;
    case 'down':
      return glyphs.arrowDown;
    case 'left':
      return glyphs.arrowLeft;
    case 'right':
      return glyphs.arrow;
    case 'pageup':
      return 'PageUp';
    case 'pagedown':
      return 'PageDown';
    case 'home':
      return 'Home';
    case 'end':
      return 'End';
    case 'space':
      return 'Space';
    default:
      return key.length === 1 ? key.toUpperCase() : key;
  }
}

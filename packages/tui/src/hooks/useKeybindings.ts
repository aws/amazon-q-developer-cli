/**
 * React hook exposing the user-configured keybindings from the Rust settings
 * store. Returns parsed bindings plus helpers for matching and formatting.
 *
 * Bindings are re-resolved whenever `appStore.settings` changes (e.g. after
 * the initial `listSettings()` fetch).
 */
import { useMemo } from 'react';
import { useAppStore } from '../stores/app-store.js';
import {
  formatKeybinding,
  matchesKeybinding,
  resolveKeybinding,
  type Keybinding,
} from '../utils/keybindings.js';
import type { Key } from './useKeypress.js';

export interface ResolvedKeybindings {
  cancelStream: Keybinding;
  closeMenu: Keybinding;
  quit: Keybinding;
  /** Convenience: does `input`/`key` match the named binding? */
  matches: (
    name: 'cancelStream' | 'closeMenu' | 'quit',
    input: string,
    key: Key
  ) => boolean;
  /** Convenience: human-readable label for the named binding. */
  label: (name: 'cancelStream' | 'closeMenu' | 'quit') => string;
}

export function useKeybindings(): ResolvedKeybindings {
  const settings = useAppStore((s) => s.settings);

  return useMemo(() => {
    const cancelStream = resolveKeybinding(settings, 'cancelStream');
    const closeMenu = resolveKeybinding(settings, 'closeMenu');
    const quit = resolveKeybinding(settings, 'quit');

    const bindings = { cancelStream, closeMenu, quit };

    return {
      ...bindings,
      matches: (name, input, key) =>
        matchesKeybinding(bindings[name], input, key),
      label: (name) => formatKeybinding(bindings[name]),
    };
  }, [settings]);
}

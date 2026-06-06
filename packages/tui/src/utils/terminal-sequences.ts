/**
 * Common terminal escape sequences used across the TUI.
 *
 * Centralises raw ANSI/DEC private-mode strings so they aren't
 * duplicated in index.tsx, AppContainer, shell-escape, etc.
 */

// Bracketed paste mode
export const ENABLE_BRACKETED_PASTE = '\x1b[?2004h';
export const DISABLE_BRACKETED_PASTE = '\x1b[?2004l';

// Cursor visibility
export const SHOW_CURSOR = '\x1b[?25h';
export const HIDE_CURSOR = '\x1b[?25l';

// Screen clear (preserves scrollback)
export const CLEAR_SCREEN = '\x1b[2J';

// Kitty keyboard protocol — CSI u disambiguation. Push flags=1
// (disambiguate) so terminals that support the protocol report
// Shift+Enter, Ctrl+I-vs-Tab, etc. as distinct sequences. Pop on exit
// to restore the legacy keyboard. No-op on terminals that don't support
// the protocol (the bytes are silently ignored).
export const ENABLE_KITTY_KEYBOARD = '\x1b[>1u';
export const DISABLE_KITTY_KEYBOARD = '\x1b[<u';

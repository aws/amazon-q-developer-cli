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

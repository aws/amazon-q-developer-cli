/**
 * Key → intent mapping for the spec review surface, kept pure so the footer and
 * the help overlay describe exactly what the handler does.
 *
 * Intents are coarse (`page`, `edge`) because the concrete line delta depends on
 * the viewport height, which only the screen knows.
 */

import type { Key } from '../../hooks/useKeypress.js';

export type ReviewIntent =
  | { type: 'move-line'; delta: 1 | -1 }
  | { type: 'move-page'; direction: 1 | -1 }
  | { type: 'move-edge'; edge: 'start' | 'end' }
  | { type: 'jump-section'; direction: 1 | -1 }
  | { type: 'jump-comment'; direction: 1 | -1 }
  | { type: 'comment' }
  | { type: 'comment-delete' }
  | { type: 'help' }
  | { type: 'close' };

/**
 * Returns null for keys this surface doesn't own, so the caller can ignore
 * twinki's broadcast cleanly.
 */
export function mapReviewKey(input: string, key: Key): ReviewIntent | null {
  if (key.escape) return { type: 'close' };
  if (key.return) return { type: 'comment' };
  if (key.upArrow) return { type: 'move-line', delta: -1 };
  if (key.downArrow) return { type: 'move-line', delta: 1 };
  if (key.pageUp) return { type: 'move-page', direction: -1 };
  if (key.pageDown) return { type: 'move-page', direction: 1 };
  if (key.home) return { type: 'move-edge', edge: 'start' };
  if (key.end) return { type: 'move-edge', edge: 'end' };
  if (key.delete || key.backspace) return { type: 'comment-delete' };

  // Every ctrl combination resolves here, so a bare-letter case below can never
  // also fire for the app's global ctrl shortcuts.
  if (key.ctrl) {
    if (input === 'u') return { type: 'move-page', direction: -1 };
    if (input === 'd') return { type: 'move-page', direction: 1 };
    return null;
  }
  if (key.meta) return null;

  switch (input) {
    case 'j':
      return { type: 'move-line', delta: 1 };
    case 'k':
      return { type: 'move-line', delta: -1 };
    case 'g':
      return { type: 'move-edge', edge: 'start' };
    case 'G':
      return { type: 'move-edge', edge: 'end' };
    case 'n':
      return { type: 'jump-section', direction: 1 };
    case 'N':
      return { type: 'jump-section', direction: -1 };
    case ']':
      return { type: 'jump-comment', direction: 1 };
    case '[':
      return { type: 'jump-comment', direction: -1 };
    case 'e':
      return { type: 'comment' };
    case '?':
      return { type: 'help' };
    case 'q':
      return { type: 'close' };
    default:
      return null;
  }
}

export interface ReviewKeyHint {
  keys: string;
  action: string;
}

/** Every binding, in the order the help lists them. */
export const REVIEW_KEY_HINTS: readonly ReviewKeyHint[] = [
  { keys: 'up down / j k', action: 'move a line' },
  { keys: 'pgup pgdn / ctrl+u ctrl+d', action: 'half a page' },
  { keys: 'n N', action: 'next / previous section' },
  { keys: '] [', action: 'next / previous comment' },
  { keys: 'home end / g G', action: 'top / bottom' },
  { keys: 'enter / e', action: 'comment on this line' },
  { keys: 'del', action: 'delete the comment under the cursor' },
  { keys: '?', action: 'this list' },
  { keys: 'esc / q', action: 'back to the checkpoint' },
];

export interface ReviewFooterHint {
  /** Rendered as-is unless `glyph` names a character that ASCII mode replaces. */
  keys: string;
  glyph?: 'arrows' | 'enter';
  action: string;
}

/**
 * The always-on footer, ordered the way every other menu in the TUI reads:
 * leave, move, jump, act, help.
 *
 * All six entries with the `to` phrasing overflow an 80-column row, so a cursor
 * resting on a comment trades the section jump — a browsing move — for the two
 * things that can be done to the comment. `?` still lists everything.
 */
export function reviewFooterHints(onComment: boolean): ReviewFooterHint[] {
  return [
    { keys: 'esc', action: 'to go back' },
    { keys: '', glyph: 'arrows', action: 'to navigate' },
    ...(onComment
      ? ([] as ReviewFooterHint[])
      : [{ keys: 'n/N', action: 'to jump' }]),
    onComment
      ? { keys: '', glyph: 'enter', action: 'to edit' }
      : { keys: '', glyph: 'enter', action: 'to comment' },
    ...(onComment ? [{ keys: 'del', action: 'to delete' }] : []),
    { keys: '?', action: 'to see keys' },
  ];
}

/** Key column width in the help list, with room before the labels. */
export const REVIEW_HINT_KEY_WIDTH =
  Math.max(...REVIEW_KEY_HINTS.map((hint) => hint.keys.length)) + 2;

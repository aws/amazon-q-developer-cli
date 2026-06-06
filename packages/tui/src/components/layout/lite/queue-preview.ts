/**
 * One-line preview of a queued message, suitable for the lite UI's queue
 * strip above the divider. The full message is preserved in the queue
 * itself — pulling a slot back into the prompt input restores the
 * original text — this helper only formats the *display* row.
 *
 * Why a dedicated helper: the prior implementation rendered each queued
 * message as a bare `<Text>{chalk.dim(`${i + 1}. ${msg}`)}` with no wrap
 * directive, so twinki's default word-wrap (`wrapTextWithAnsi`) walked
 * the entire string and laid out hundreds of physical rows for a
 * multi-KB queued message. Each LiteLayout re-render (tool start/finish,
 * model commit, mode swap, ...) re-paid the chalk allocation + Yoga
 * measurement, which on multi-KB inputs blew past the 150ms spinner
 * cadence in `LiteLiveRegion` — making the in-flight tool's spinner
 * appear to stop ticking.
 *
 * The fix bounds work in three layers, in this order:
 *
 *   1. `slice(0, maxCols * 2)` — caps the raw input that downstream
 *      passes ever see. `maxCols * 2` covers double-width characters
 *      (CJK ideographs, fullwidth forms) without dragging the full
 *      message through whitespace collapse and grapheme segmentation.
 *      For a 50KB queued message this is the load-bearing cut.
 *   2. `replace(/\s+/g, ' ')` — collapses runs of whitespace (tabs,
 *      newlines, multiple spaces) into a single space. Without this a
 *      multi-line paste would still emit one row per `\n` even after
 *      the slice, because `<Text>` splits on newlines before rendering.
 *      Operates on the already-bounded prefix.
 *   3. `truncateToWidth(flat, maxCols)` — grapheme-aware terminal-
 *      column truncation (Intl.Segmenter, no emoji splits). Final width
 *      cap so the rendered row never wraps regardless of chrome (chevron
 *      marker, index prefix, etc.).
 *
 * The chalk styling at the call site then walks at most `maxCols` chars,
 * not `msg.length` chars, on every render.
 */
import { truncateToWidth } from '../../../utils/text-width.js';

export function previewLine(msg: string, maxCols: number): string {
  // Defensive floor — a 0/negative width would make truncateToWidth
  // return just the ellipsis, which is a poor signal that the terminal
  // is unusably narrow rather than that the message is empty.
  const width = Math.max(8, maxCols);
  // Pre-slice raw input. JS string slice is O(maxCols), independent of
  // the full message length — this is the single load-bearing cut.
  const sliced = msg.length > width * 2 ? msg.slice(0, width * 2) : msg;
  const flat = sliced.replace(/\s+/g, ' ').trim();
  return truncateToWidth(flat, width);
}

/**
 * One-line display preview of a queued message for the lite UI's queue
 * strip. The full message stays in the queue (pulling a slot back into the
 * prompt restores the original text); this only formats the row.
 *
 * WHY bound the work: a multi-KB queued message rendered with twinki's
 * default word-wrap was re-laid-out on every LiteLayout re-render, blowing
 * past the 150ms spinner cadence in LiteLiveRegion (in-flight spinner
 * appeared frozen). slice→collapse-whitespace→truncate caps the cost so the
 * call-site chalk styling walks at most `maxCols` chars, not `msg.length`.
 */
import { truncateToWidth } from '../../../utils/text-width.js';

export function previewLine(msg: string, maxCols: number): string {
  // Defensive floor — a 0/negative width would make truncateToWidth return
  // just the ellipsis, a poor signal for an unusably narrow terminal.
  const width = Math.max(8, maxCols);
  // O(maxCols) pre-slice (×2 covers double-width chars) — the load-bearing cut.
  const sliced = msg.length > width * 2 ? msg.slice(0, width * 2) : msg;
  const flat = sliced.replace(/\s+/g, ' ').trim();
  return truncateToWidth(flat, width);
}

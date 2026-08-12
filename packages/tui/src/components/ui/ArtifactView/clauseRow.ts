import { truncateToWidth } from '../../../utils/text-width.js';

/**
 * Columns a clause row cannot use: the panel's horizontal padding plus the
 * row's own indent.
 */
const CLAUSE_ROW_INDENT = 6;

/**
 * The clause text as its row should show it.
 *
 * A clause is one row so an expanded section stays a glance rather than a copy
 * of the document, which means most clauses get cut. The cut is made here, to an
 * ellipsis, because the renderer's own truncation removes the overflow without
 * leaving any sign it happened — a row ending mid-word reads as a fault rather
 * than as a clause with more to it.
 */
export function clauseRowText(
  number: string,
  text: string,
  terminalWidth: number
): string {
  const room = terminalWidth - CLAUSE_ROW_INDENT - number.length - 1;
  return truncateToWidth(text, Math.max(0, room));
}

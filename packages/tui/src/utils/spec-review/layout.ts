/**
 * Turns a spec document and its staged comments into the rows the review
 * surface draws.
 *
 * Each row carries the indent it is drawn with, and wrapping is measured
 * against that indent in terminal columns. A row wider than the terminal gets
 * folded a second time by the terminal, and that fold carries no indent, so its
 * text lands hard against the left edge.
 */
import { visibleWidth, wrapAtWords } from '../text-width.js';
import type { ReviewAction } from './review-actions.js';

/** Columns held for the cursor marker, which every row reserves. */
export const MARKER_COLS = 1;

/**
 * One rendered row. Drawn as the marker, then `indent`, then `bullet`, then
 * `text` — `rowColumns` measures exactly that.
 */
export type Row = {
  kind: 'line' | 'comment';
  lineIndex: number;
  /** Set on comment rows, naming the comment the row belongs to. */
  commentId?: string;
  indent: string;
  bullet: string;
  text: string;
  /** True on rows produced by wrapping, which carry no marker or bullet. */
  continues: boolean;
};

/** Columns the row occupies once drawn, marker included. */
export function rowColumns(row: Row): number {
  return (
    MARKER_COLS +
    row.indent.length +
    visibleWidth(row.bullet) +
    visibleWidth(row.text)
  );
}

const LINE_INDENT = ' ';
const COMMENT_INDENT = '   ';

/** A bullet or numbered list marker, with the space that follows it. */
const LIST_MARKER = /^(\s*)(?:[-*+]|\d{1,3}[.)])\s+/;
const FENCE = /^\s*(?:```|~~~)/;

/**
 * Where a wrapped row's text sits.
 *
 * Under a list item it lines up with the item's text, past the marker, so the
 * continuation can't be mistaken for a sibling item. Everywhere else — prose,
 * table rows, code and diagrams — it lines up with the first row's text, since
 * there is no marker to hang from and shifting would only misrepresent the
 * document's own indentation.
 */
function continuedIndentFor(line: string, inFence: boolean): string {
  const marker = inFence ? null : LIST_MARKER.exec(line);
  const width = marker
    ? marker[0].length
    : line.length - line.trimStart().length;
  return LINE_INDENT + ' '.repeat(width);
}

/** Budget for a row's text: the terminal, less everything drawn before it. */
function textBudget(indent: string, bullet: string, width: number): number {
  return Math.max(
    1,
    width - MARKER_COLS - indent.length - visibleWidth(bullet)
  );
}

/**
 * Flatten the document into rendered rows, so a long line is readable in full
 * rather than cut off at the terminal's edge — the user can't comment on what
 * they can't see.
 *
 * `bullet` marks a comment; it is passed in so its width is measured as drawn,
 * which differs between the unicode and ASCII glyph sets.
 */
export function layoutRows(
  lines: readonly string[],
  actions: readonly ReviewAction[],
  width: number,
  bullet: string
): Row[] {
  const byLine = new Map<number, ReviewAction[]>();
  for (const action of actions) {
    const line = action.anchor.range.start;
    const list = byLine.get(line) ?? [];
    list.push(action);
    byLine.set(line, list);
  }

  const commentContinuedIndent = ' '.repeat(
    COMMENT_INDENT.length + visibleWidth(bullet)
  );
  const rows: Row[] = [];

  let inFence = false;
  lines.forEach((line, lineIndex) => {
    if (FENCE.test(line)) inFence = !inFence;
    const continuedIndent = continuedIndentFor(line, inFence);
    const chunks = wrapAtWords(
      line,
      textBudget(LINE_INDENT, '', width),
      textBudget(continuedIndent, '', width)
    );
    chunks.forEach((text, i) => {
      rows.push({
        kind: 'line',
        lineIndex,
        indent: i === 0 ? LINE_INDENT : continuedIndent,
        bullet: '',
        text,
        continues: i > 0,
      });
    });

    for (const action of byLine.get(lineIndex) ?? []) {
      const budget = textBudget(COMMENT_INDENT, bullet, width);
      const bodyChunks = wrapAtWords(action.body, budget, budget);
      bodyChunks.forEach((text, i) => {
        rows.push({
          kind: 'comment',
          lineIndex,
          commentId: action.id,
          indent: i === 0 ? COMMENT_INDENT : commentContinuedIndent,
          bullet: i === 0 ? bullet : '',
          text,
          continues: i > 0,
        });
      });
    }
  });

  return rows;
}

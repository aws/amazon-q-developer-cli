/**
 * Review comments the user attaches to a spec document, and the text they
 * become when sent to the agent.
 *
 * A comment is anchored by what the document says, not by where it sits: the
 * enclosing heading plus the quoted line. The agent rewrites the document in
 * response, so a positional anchor would be stale by the time it mattered —
 * `range` exists only to place the marker in the viewer.
 */

/** Inclusive 0-based span of lines. */
export interface LineRange {
  start: number;
  end: number;
}

/** What a review action points at. */
export interface ReviewAnchor {
  /**
   * Lines the comment covers, for rendering the marker. Not sent to the agent.
   * A single-line comment has `start === end`.
   */
  range: LineRange;
  /** Nearest heading above the range, e.g. "Requirement 2: Count-Up Timing". */
  heading: string | null;
  /** First line's text, quoted so the agent can locate it after any rewrite. */
  snippet: string;
}

/**
 * Something the user wants changed. Only comments today; an `edit` variant can
 * join without reshaping the staging area or the surface that collects them.
 */
export type ReviewAction = {
  kind: 'comment';
  /** Identifies this comment so it can be edited or dropped on its own. */
  id: string;
  anchor: ReviewAnchor;
  body: string;
};

export function nextReviewActionId(): string {
  return `review-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

/** "1 comment" / "2 comments", for the several places that count them. */
export function commentCount(n: number): string {
  return `${n} comment${n === 1 ? '' : 's'}`;
}

/** Nearest markdown heading at or above `lineIndex`, without its `#` marker. */
export function findEnclosingHeading(
  lines: readonly string[],
  lineIndex: number
): string | null {
  for (let i = Math.min(lineIndex, lines.length - 1); i >= 0; i--) {
    const match = /^#{1,6}\s+(.*\S)\s*$/.exec(lines[i] ?? '');
    if (match) return match[1] ?? null;
  }
  return null;
}

/**
 * The next markdown heading above or below `from`, for moving through a spec the
 * way it is read — requirement by requirement rather than line by line.
 */
export function nextHeadingLine(
  lines: readonly string[],
  from: number,
  direction: 1 | -1
): number | null {
  for (let i = from + direction; i >= 0 && i < lines.length; i += direction) {
    if (/^#{1,6}\s+\S/.test(lines[i] ?? '')) return i;
  }
  return null;
}

/** Build the anchor for the lines a comment covers. */
export function anchorFor(
  lines: readonly string[],
  range: LineRange
): ReviewAnchor {
  return {
    range,
    heading: findEnclosingHeading(lines, range.start),
    snippet: (lines[range.start] ?? '').trim(),
  };
}

function escapeAttribute(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/**
 * The message an agent receives for a set of comments: context first, then the
 * directive. The opening prose also guarantees the payload never starts with
 * `/`, which the first text block of a prompt is scanned for.
 */
export function composeRevisionRequest(
  documentName: string,
  actions: readonly ReviewAction[]
): string {
  const comments = [...actions]
    .sort((a, b) => a.anchor.range.start - b.anchor.range.start)
    .map((action) => {
      const on = action.anchor.heading
        ? ` on="${escapeAttribute(action.anchor.heading)}"`
        : '';
      const quote = action.anchor.snippet
        ? ` quote="${escapeAttribute(action.anchor.snippet)}"`
        : '';
      return `<comment${on}${quote}>\n${action.body}\n</comment>`;
    })
    .join('\n');

  return `The user reviewed ${documentName} and left comments. Each one names the section it belongs to and quotes the line it annotates.

${comments}

Revise ${documentName} so every comment is addressed. Locate each one by its quoted line, and leave the rest of the document — its headings, its wording, and its structure — as it is.`;
}

/** Where the cursor can rest: every document line, and each comment under it. */
export function navigableStops(
  lineCount: number,
  actions: readonly ReviewAction[]
): { lineIndex: number; commentId: string | null }[] {
  const byLine = new Map<number, ReviewAction[]>();
  for (const action of actions) {
    const line = action.anchor.range.start;
    const list = byLine.get(line) ?? [];
    list.push(action);
    byLine.set(line, list);
  }
  const stops: { lineIndex: number; commentId: string | null }[] = [];
  for (let lineIndex = 0; lineIndex < lineCount; lineIndex++) {
    stops.push({ lineIndex, commentId: null });
    for (const action of byLine.get(lineIndex) ?? []) {
      stops.push({ lineIndex, commentId: action.id });
    }
  }
  return stops;
}

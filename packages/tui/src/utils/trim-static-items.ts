/** Max static items kept in the React fiber tree.  Bounds Yoga node count so
 *  terminal resize doesn't re-layout an unbounded number of items. */
export const MAX_STATIC_ITEMS = parseInt(
  process.env.KIRO_MAX_STATIC_ITEMS || '200',
  10
);

/** Item stored in the static items array. Only `id` is needed for trimming. */
interface HasId {
  id: string;
}

/** Trim oldest static items when the array exceeds the cap.  Returns the
 *  number of items removed.  Caller must notify the renderer to adjust its
 *  static write cursor by the returned count (see adjustStaticCursor). */
export function trimStaticItems<T extends HasId>(
  items: T[],
  _emittedIds: Set<string>,
  cap = MAX_STATIC_ITEMS
): number {
  if (items.length <= cap * 1.1) return 0;
  const removed = items.splice(0, items.length - cap);
  // Do NOT delete from emittedIds — those items were already written to the
  // terminal.  Keeping them in the guard set prevents re-emission if any
  // code path (e.g. flushedMap for completed turns) tries to re-append them.
  return removed.length;
}

/** Max static items kept in the React fiber tree.  Bounds Yoga node count so
 *  terminal resize doesn't re-layout an unbounded number of items. */
export const MAX_STATIC_ITEMS = 200;

/** Item stored in the static items array. Only `id` is needed for trimming. */
interface HasId {
  id: string;
}

/** Trim oldest static items when the array exceeds the cap.  Returns the
 *  number of items removed.  Safe because the renderer's monotonic write
 *  cursor will not re-write already-flushed items. */
export function trimStaticItems<T extends HasId>(
  items: T[],
  emittedIds: Set<string>,
  cap = MAX_STATIC_ITEMS
): number {
  if (items.length <= cap * 1.1) return 0;
  const removed = items.splice(0, items.length - cap);
  for (const item of removed) emittedIds.delete(item.id);
  return removed.length;
}

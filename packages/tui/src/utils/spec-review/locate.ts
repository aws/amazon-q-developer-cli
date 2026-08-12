/**
 * Where a parsed item sits in the document it came from.
 *
 * The spec parser hands back each item's `detailBody` as a verbatim slice of the
 * source, so the slice's offset is the item's position — no line numbers need to
 * be threaded through the parser to open a document at the item under a cursor.
 */

/** 0-based line the slice starts on, or 0 when it can't be found. */
export function lineOfSlice(source: string, slice: string | null): number {
  if (!slice) return 0;
  const at = source.indexOf(slice);
  if (at < 0) return 0;
  let line = 0;
  for (let i = 0; i < at; i++) {
    if (source.charCodeAt(i) === 10) line++;
  }
  return line;
}

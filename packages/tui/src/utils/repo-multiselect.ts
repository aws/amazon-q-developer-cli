/** Pure filter + ordered-selection helpers for the `/repo` picker. */
import type { SourceProviderResource } from '@kiro/acp-type-covenant';
import { type Glyphs, UNICODE_GLYPHS } from './glyphs.js';

/**
 * Drop catalog rows whose `name` repeats an earlier row (first sighting wins).
 * Providers can return the same repo more than once (e.g. across pages);
 * selection and attach are keyed by name, so duplicate names are
 * indistinguishable downstream — one checkbox would light up every copy and
 * React row keys would collide.
 */
export function dedupeRepoResources(
  resources: SourceProviderResource[]
): SourceProviderResource[] {
  const seen = new Set<string>();
  return resources.filter((r) =>
    seen.has(r.name) ? false : (seen.add(r.name), true)
  );
}

/** Case-insensitive substring filter on repo name (empty query = all). */
export function filterRepoResources(
  resources: SourceProviderResource[],
  query: string
): SourceProviderResource[] {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return resources;
  return resources.filter((r) => r.name.toLowerCase().includes(q));
}

/**
 * Toggle a repo's membership in the ordered selected list. Selecting appends
 * (so the `Selected(N)` summary lists repos in the order chosen); re-toggling
 * removes it. Returns a new array — never mutates the input.
 */
export function toggleRepoSelection(
  selected: readonly string[],
  name: string
): string[] {
  return selected.includes(name)
    ? selected.filter((n) => n !== name)
    : [...selected, name];
}

/**
 * Compact relative "last used" label (e.g. `3h ago`, `2w ago`, `just now`) for
 * the picker's column. Falls back to the em-dash glyph when the source omitted
 * `updatedAt` or it is unparseable, so the column stays aligned.
 */
export function formatRepoLastUsed(
  updatedAt?: string,
  glyphs: Glyphs = UNICODE_GLYPHS
): string {
  if (!updatedAt) return glyphs.emDash;
  const then = new Date(updatedAt).getTime();
  if (Number.isNaN(then)) return glyphs.emDash;
  const secs = Math.floor((Date.now() - then) / 1000);
  if (secs < 60) return 'just now';
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  const days = Math.floor(secs / 86400);
  if (days < 7) return `${days}d ago`;
  return `${Math.floor(days / 7)}w ago`;
}

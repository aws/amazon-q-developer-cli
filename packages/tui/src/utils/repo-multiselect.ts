/** Pure filter + ordered-selection helpers for the `/repo` picker. */
import type { SourceProviderResource } from '@kiro/acp-type-covenant';
import { type Glyphs, UNICODE_GLYPHS } from './glyphs.js';
import { formatRelativeTime } from './sessions.js';

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
 * Compact last-used label for a repo row from the resource's `updatedAt`;
 * an em-dash placeholder (glyph-routed for ASCII mode) when unknown.
 */
export function formatRepoLastUsed(
  updatedAt?: string,
  glyphs: Glyphs = UNICODE_GLYPHS
): string {
  if (!updatedAt || Number.isNaN(new Date(updatedAt).getTime())) {
    return glyphs.emDash;
  }
  return formatRelativeTime(updatedAt, { compact: true });
}

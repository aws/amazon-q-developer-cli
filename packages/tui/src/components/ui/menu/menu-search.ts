import { fuzzyScore } from '../../../utils/fuzzyScore.js';
import type { MenuItem } from './Menu.js';

/**
 * Match tier for a searchable menu item; lower ranks first. Label matches
 * always beat description-only matches: long descriptions contain almost any
 * short subsequence, which otherwise buries items whose name literally
 * contains the query.
 */
function matchTier(label: string, query: string): number {
  // Labels often carry a display-only leading slash the user doesn't type.
  const bare = label.startsWith('/') ? label.slice(1) : label;
  if (bare.startsWith(query)) return 0;
  if (bare.includes(query)) return 1;
  if (fuzzyScore(query, label) > 0) return 2;
  return 3;
}

/** Filter and rank searchable menu items: tier first, fuzzy score within. */
export function rankMenuItems(
  items: readonly MenuItem[],
  searchText: string
): MenuItem[] {
  const query = searchText.toLowerCase();
  const scored: { item: MenuItem; tier: number; score: number }[] = [];
  for (const item of items) {
    const label = item.label.toLowerCase();
    const labelScore = fuzzyScore(query, label);
    const descScore = fuzzyScore(query, item.description.toLowerCase());
    if (labelScore <= 0 && descScore <= 0) continue;
    scored.push({
      item,
      tier: matchTier(label, query),
      score: Math.max(labelScore, descScore),
    });
  }
  scored.sort((a, b) => a.tier - b.tier || b.score - a.score);
  return scored.map((s) => s.item);
}

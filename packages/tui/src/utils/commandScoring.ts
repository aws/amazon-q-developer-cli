/**
 * Unified scoring function for slash command matching.
 * Uses the same fuzzyScore approach as Menu's searchable mode.
 */

import { fuzzyScore } from './fuzzyScore.js';

export function scoreCommands<T extends { name: string; description: string }>(
  commands: readonly T[],
  partial: string
): Array<{ command: T; score: number }> {
  if (!partial) {
    const all = commands.map((command) => ({ command, score: 1 }));
    all.sort((a, b) => a.command.name.localeCompare(b.command.name));
    return all;
  }

  const query = partial.toLowerCase();
  const results: Array<{ command: T; score: number }> = [];

  for (const command of commands) {
    const name = command.name.replace(/^\//, '').toLowerCase();
    const nameScore = fuzzyScore(query, name);
    const descScore = fuzzyScore(query, command.description.toLowerCase());
    const score = Math.max(nameScore, descScore);
    if (score > 0) {
      results.push({ command, score });
    }
  }

  results.sort(
    (a, b) => b.score - a.score || a.command.name.localeCompare(b.command.name)
  );
  return results;
}

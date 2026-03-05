/**
 * Fuzzy subsequence match scoring. Returns 0 if query is not a subsequence of target.
 * Higher score = better match. Rewards consecutive runs and word-boundary matches.
 */
export function fuzzyScore(query: string, target: string): number {
  let qi = 0;
  let score = 0;
  let consecutive = 0;
  let prevMatchIdx = -2;

  for (let ti = 0; ti < target.length && qi < query.length; ti++) {
    if (target[ti] === query[qi]) {
      qi++;
      score += 1;
      if (ti === prevMatchIdx + 1) {
        consecutive++;
        score += consecutive;
      } else {
        consecutive = 0;
      }
      if (ti === 0 || '-_ .'.includes(target[ti - 1]!)) {
        score += 2;
      }
      prevMatchIdx = ti;
    }
  }
  return qi === query.length ? score : 0;
}

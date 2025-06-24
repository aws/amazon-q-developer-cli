export function alphaByTitle(a: { title: string }, b: { title: string }) {
  if (a.title > b.title) return 1;
  if (a.title < b.title) return -1;

  return 0;
}

export function alphaByTitlePrioritized(
  a: { title: string; priority?: number },
  b: { title: string; priority?: number },
) {
  const aPriority = a.priority ?? 0;
  const bPriority = b.priority ?? 0;
  if (aPriority < bPriority) return 1;
  if (aPriority > bPriority) return -1;

  return alphaByTitle(a, b);
}

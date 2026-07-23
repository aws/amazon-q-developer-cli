const BLOCK_LINE = /^\s*(?:```|~~~|#{1,6}\s|>|[-+*]\s|\d+[.)]\s|\|| {4}|\t)/;
const FENCE_LINE = /^\s*(?:```|~~~)/;

/** Collapse Markdown soft breaks in stage instructions while preserving
 * paragraphs, lists, quotes, fences, and explicit hard breaks. */
export function normalizeSubagentPrompt(prompt: string): string {
  const lines = prompt.replace(/\r\n?/g, '\n').split('\n');
  while (lines[0]?.trim().length === 0) lines.shift();
  while (lines.at(-1)?.trim().length === 0) lines.pop();
  if (lines.length <= 1) return lines[0] ?? '';

  let inFence = FENCE_LINE.test(lines[0] ?? '');
  let output = (lines[0] ?? '').trimEnd();
  for (let i = 1; i < lines.length; i++) {
    const previous = lines[i - 1] ?? '';
    const current = lines[i] ?? '';
    const preserveBreak =
      inFence ||
      previous.trim().length === 0 ||
      current.trim().length === 0 ||
      BLOCK_LINE.test(previous) ||
      BLOCK_LINE.test(current) ||
      / {2,}$/.test(previous);

    output += preserveBreak ? '\n' : ' ';
    output += preserveBreak ? current.trimEnd() : current.trim();
    if (FENCE_LINE.test(current)) inFence = !inFence;
  }
  return output;
}

export function parseSubagentStageNames(content: string | undefined): string[] {
  if (!content) return [];
  try {
    const parsed = JSON.parse(content) as { stages?: unknown };
    if (!Array.isArray(parsed.stages)) return [];
    return parsed.stages.flatMap((stage, index) => {
      if (!stage || typeof stage !== 'object') return [];
      const name = (stage as { name?: unknown }).name;
      return [
        typeof name === 'string' && name.length > 0
          ? name
          : `stage-${index + 1}`,
      ];
    });
  } catch {
    return [];
  }
}

/** Pipeline declaration order wins over child-session completion order. */
export function orderSubagentStageItems<T extends { stageName: string }>(
  items: readonly T[],
  stageNames: readonly string[]
): T[] {
  if (items.length < 2 || stageNames.length === 0) return [...items];
  const rank = new Map<string, number>();
  for (const [index, name] of stageNames.entries()) {
    if (!rank.has(name)) rank.set(name, index);
  }
  return items
    .map((item, index) => ({ item, index }))
    .sort((a, b) => {
      const aRank = rank.get(a.item.stageName) ?? Number.MAX_SAFE_INTEGER;
      const bRank = rank.get(b.item.stageName) ?? Number.MAX_SAFE_INTEGER;
      return aRank - bRank || a.index - b.index;
    })
    .map(({ item }) => item);
}

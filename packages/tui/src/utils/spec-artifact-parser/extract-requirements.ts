import type { RequirementItem } from './types.js';

/**
 * Extract `### Requirement N:` blocks from a requirements.md source.
 *
 * Line-based scan rather than full markdown AST: this is more permissive
 * of partial/streaming input (the parser runs while the agent is mid-write)
 * and gives us byte-identical detail slices.
 *
 * Each requirement runs from its `### Requirement N` heading to the next
 * `### Requirement` heading (or EOF). The user story is the first non-empty
 * line beginning with `**User Story:**` within that range, captured verbatim.
 */
export function extractRequirements(source: string): RequirementItem[] {
  if (!source) return [];
  const lines = source.split('\n');

  // Match `### Requirement <digits>` (followed optionally by `:` and a title).
  // We deliberately require at least one digit so plain `### Requirement` text
  // doesn't accidentally start a new block.
  const headingRe = /^### Requirement\s+(\d+)\s*:?\s*(.*)$/;

  // Pass 1: locate heading lines and capture number + title.
  type Heading = { lineIdx: number; number: number; title: string };
  const headings: Heading[] = [];
  for (let i = 0; i < lines.length; i++) {
    const m = headingRe.exec(lines[i]!);
    if (m) {
      headings.push({
        lineIdx: i,
        number: parseInt(m[1]!, 10),
        title: (m[2] ?? '').trim(),
      });
    }
  }

  if (headings.length === 0) return [];

  // Pass 2: slice each requirement's body and find its user story.
  const items: RequirementItem[] = [];
  for (let h = 0; h < headings.length; h++) {
    const heading = headings[h]!;
    const nextHeading = headings[h + 1];
    const startLine = heading.lineIdx;
    const endLine = nextHeading ? nextHeading.lineIdx : lines.length;
    const blockLines = lines.slice(startLine, endLine);

    // Find the first non-empty `**User Story:**` line, captured verbatim
    // (preserving leading/trailing whitespace as it appears on the line —
    // useful when the block is mid-stream and the line is incomplete).
    let userStory: string | null = null;
    for (const raw of blockLines) {
      const trimmedLeft = raw.replace(/^\s+/, '');
      if (trimmedLeft.length === 0) continue;
      if (trimmedLeft.startsWith('**User Story:**')) {
        userStory = raw;
        break;
      }
    }

    // detailBody: full source slice including heading and any blank trailing
    // lines up to the next heading. Joining with '\n' is symmetric with the
    // earlier `split('\n')`; we don't add a trailing newline.
    const detailBody = blockLines.join('\n');

    items.push({
      number: heading.number,
      title: heading.title,
      userStory,
      detailBody,
    });
  }

  return items;
}

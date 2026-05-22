import type { HighLevelTask, SubTask } from './types.js';

interface CheckboxLine {
  /** Original line index in the source. */
  lineIdx: number;
  /** Indentation in spaces (tabs expanded to 4). */
  depth: number;
  /** True if the bracket contained `x` or `X`. */
  checked: boolean;
  /** Text after the checkbox (verbatim, trimmed of leading whitespace). */
  body: string;
  /** Numbering string captured from the body (e.g. "1", "1.0"), or null. */
  number: string | null;
  /** Title text after stripping the leading number+dot, if present. */
  title: string;
}

const CHECKBOX_RE = /^([ \t]*)- \[([xX ])\]\s*(.*)$/;
// Matches a leading "1.", "1.2.", "1.2.3" (any number of dot-separated digits)
// followed by optional whitespace. Captures the full numbering string.
const NUMBER_PREFIX_RE = /^(\d+(?:\.\d+)*)\.?\s+(.*)$/;

/**
 * Convert a line of leading whitespace into a depth count (in spaces),
 * expanding tabs to 4 spaces.
 */
function indentDepth(leading: string): number {
  let depth = 0;
  for (const ch of leading) {
    if (ch === '\t') depth += 4;
    else if (ch === ' ') depth += 1;
    else break;
  }
  return depth;
}

/**
 * Parse all checkbox lines from the source. Non-checkbox lines are ignored.
 */
function parseCheckboxes(source: string): CheckboxLine[] {
  const lines = source.split('\n');
  const out: CheckboxLine[] = [];
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    const m = CHECKBOX_RE.exec(line);
    if (!m) continue;

    const leading = m[1] ?? '';
    const mark = m[2] ?? ' ';
    const body = (m[3] ?? '').trim();
    const depth = indentDepth(leading);
    const checked = mark === 'x' || mark === 'X';

    let number: string | null = null;
    let title = body;
    const numMatch = NUMBER_PREFIX_RE.exec(body);
    if (numMatch) {
      number = numMatch[1] ?? null;
      title = (numMatch[2] ?? '').trim();
    }

    out.push({ lineIdx: i, depth, checked, body, number, title });
  }
  return out;
}

/**
 * Extract high-level tasks and their direct sub-tasks from a tasks.md source.
 *
 * Algorithm:
 *   1. Find every checkbox line (`- [ ]` / `- [x]` / `- [X]`).
 *   2. The smallest indentation depth among them is the high-level depth.
 *   3. High-level tasks are at that depth; sub-tasks are any checkbox at a
 *      strictly greater depth, associated with the most recent preceding
 *      high-level task in file order.
 *   4. detailBody is the verbatim source slice from the high-level task line
 *      up to (but excluding) the next high-level task line, or EOF.
 *
 * Numbering ("1", "1.0", "2") is captured verbatim from the body of each
 * high-level task. If a task body has no leading number, `number` is the
 * empty string (we still surface the task — losing it would silently hide
 * content that exists on disk).
 */
export function extractTasks(source: string): HighLevelTask[] {
  if (!source) return [];

  const checkboxes = parseCheckboxes(source);
  if (checkboxes.length === 0) return [];

  const minDepth = checkboxes.reduce(
    (acc, c) => (c.depth < acc ? c.depth : acc),
    Number.POSITIVE_INFINITY
  );

  // Indices in `checkboxes` of high-level tasks.
  const highLevelIdxs: number[] = [];
  for (let i = 0; i < checkboxes.length; i++) {
    if (checkboxes[i]!.depth === minDepth) highLevelIdxs.push(i);
  }
  if (highLevelIdxs.length === 0) return [];

  const lines = source.split('\n');
  const items: HighLevelTask[] = [];

  for (let h = 0; h < highLevelIdxs.length; h++) {
    const idx = highLevelIdxs[h]!;
    const head = checkboxes[idx]!;
    const nextIdx = highLevelIdxs[h + 1];
    const nextHead = nextIdx !== undefined ? checkboxes[nextIdx] : undefined;

    // Sub-tasks: any subsequent checkbox before the next high-level task,
    // at strictly greater depth.
    const subTasks: SubTask[] = [];
    const lastChildIdx = nextIdx !== undefined ? nextIdx : checkboxes.length;
    for (let i = idx + 1; i < lastChildIdx; i++) {
      const c = checkboxes[i]!;
      if (c.depth > minDepth) {
        subTasks.push({
          title: c.body,
          checked: c.checked,
          depth: c.depth,
        });
      }
    }

    // Source slice for detail body.
    const startLine = head.lineIdx;
    const endLine = nextHead ? nextHead.lineIdx : lines.length;
    const detailBody = lines.slice(startLine, endLine).join('\n');

    items.push({
      number: head.number ?? '',
      title: head.title,
      checked: head.checked,
      subTasks,
      detailBody,
    });
  }

  return items;
}

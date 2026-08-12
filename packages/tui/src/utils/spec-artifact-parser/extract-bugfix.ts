import type { BugfixSection, BugfixClause } from './types.js';

/**
 * `X.Y` numbered clause, the form the bugfix workflow requires of every clause
 * in every section — the section number, then the clause within it.
 */
const CLAUSE = /^(\d+\.\d+)\s+(.*)$/;

/** H3 heading, the level the three behaviour sections sit at. */
const H3 = /^###\s+(.*\S)\s*$/;

/** Any ATX heading, for bounding a section's slice. */
const HEADING = /^#{1,6}\s/;

function firstParagraphAfter(lines: string[], from: number): string {
  let i = from;
  while (i < lines.length && lines[i]!.trim().length === 0) i++;
  const buf: string[] = [];
  for (; i < lines.length; i++) {
    const line = lines[i]!;
    if (line.trim().length === 0 || HEADING.test(line)) break;
    buf.push(line.trim());
  }
  return buf.join(' ');
}

/**
 * Extract the summary fields from a bugfix.md source.
 *
 * `## Introduction` is matched by name, since the workflow that writes these
 * documents always emits it. The behaviour sections are matched structurally
 * instead — any H3 carrying `X.Y` clauses is one — so the three the workflow
 * requires are picked up without their exact titles being hardcoded, and a
 * document with more or fewer yields more or fewer sections rather than an
 * error. The panel shows what is there.
 */
export function extractBugfix(source: string): {
  overview: string;
  sections: BugfixSection[];
} {
  if (!source) return { overview: '', sections: [] };
  const lines = source.split('\n');

  let overview = '';
  for (let i = 0; i < lines.length; i++) {
    if (/^##\s+Introduction\s*$/.test(lines[i]!)) {
      overview = firstParagraphAfter(lines, i + 1);
      break;
    }
  }

  const headings: { line: number; title: string }[] = [];
  for (let i = 0; i < lines.length; i++) {
    const match = H3.exec(lines[i]!);
    if (match) headings.push({ line: i, title: match[1]! });
  }

  const sections: BugfixSection[] = [];
  for (let h = 0; h < headings.length; h++) {
    const heading = headings[h]!;
    // A section runs to the next heading of any level, so an H2 after the last
    // H3 ends it rather than swallowing the rest of the document.
    let end = lines.length;
    for (let i = heading.line + 1; i < lines.length; i++) {
      if (HEADING.test(lines[i]!)) {
        end = i;
        break;
      }
    }
    const body = lines.slice(heading.line, end);
    const clauses: BugfixClause[] = [];
    for (const line of body) {
      const match = CLAUSE.exec(line.trim());
      if (match) clauses.push({ number: match[1]!, text: match[2]!.trim() });
    }
    if (clauses.length === 0) continue;
    sections.push({
      title: heading.title,
      clauses,
      detailBody: body.join('\n').replace(/\s+$/, ''),
    });
  }

  return { overview, sections };
}

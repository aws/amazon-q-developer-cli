import { DESIGN_OVERVIEW_MAX_CHARS, type DesignSection } from './types.js';

interface H2Match {
  /** Line index of the `## …` line. */
  lineIdx: number;
  /** Heading text without the `## ` prefix. */
  title: string;
}

/**
 * Find every H2 heading line (lines beginning with exactly `## ` — two hash
 * chars + space). Excludes H1 (`# …`) and H3+ (`### …`).
 */
function findH2Headings(lines: string[]): H2Match[] {
  const headings: H2Match[] = [];
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    // Strict check: starts with "## " but NOT "###"
    if (line.startsWith('## ') && !line.startsWith('### ')) {
      const title = line.slice(3).trim();
      headings.push({ lineIdx: i, title });
    }
  }
  return headings;
}

/**
 * Extract the body lines below an H2 heading, stopping at the next H2 or EOF.
 * The returned slice includes the heading line itself.
 */
function sliceSection(
  lines: string[],
  heading: H2Match,
  next: H2Match | undefined
): string[] {
  const endLine = next ? next.lineIdx : lines.length;
  return lines.slice(heading.lineIdx, endLine);
}

/**
 * Returns the lines after the heading (excluding the heading itself), trimmed
 * of leading/trailing blank lines. Used to determine whether a section has
 * any non-empty body.
 */
function sectionBodyLines(sectionLines: string[]): string[] {
  return sectionLines.slice(1);
}

/**
 * Returns true if the lines contain at least one non-whitespace character.
 */
function hasNonEmptyBody(bodyLines: string[]): boolean {
  return bodyLines.some((l) => l.trim().length > 0);
}

/**
 * Read the first contiguous non-empty paragraph from a sequence of lines.
 * A paragraph runs from the first non-blank line to the next blank line
 * (or end of input). Lines are joined with a single space.
 */
function firstParagraph(bodyLines: string[]): string {
  // Skip leading blank lines.
  let i = 0;
  while (i < bodyLines.length && bodyLines[i]!.trim().length === 0) i++;
  if (i >= bodyLines.length) return '';

  const buf: string[] = [];
  for (; i < bodyLines.length; i++) {
    const line = bodyLines[i]!;
    if (line.trim().length === 0) break;
    buf.push(line.trim());
  }
  return buf.join(' ').trim();
}

/**
 * Extract the design summary fields from a design.md source.
 *
 * Overview fallback chain:
 *   1. First paragraph of `## Introduction` (when its body is non-empty)
 *   2. First paragraph of `## Overview` (when its body is non-empty)
 *   3. First paragraph of `## Architecture Overview` (when non-empty)
 *   4. First non-empty PROSE paragraph of the body (all headings excluded)
 *   5. Empty string
 *
 * Sections are H2 headings with non-empty bodies, in source order.
 */
export function extractDesign(source: string): {
  overview: string;
  overviewTruncated: boolean;
  sections: DesignSection[];
} {
  if (!source) {
    return { overview: '', overviewTruncated: false, sections: [] };
  }

  const lines = source.split('\n');
  const h2s = findH2Headings(lines);

  // ── Overview ────────────────────────────────────────────────
  // Source priority: Introduction → Overview → Architecture Overview, then
  // the first prose paragraph of the document body.
  let overview = '';
  const overviewHeadingTitles = [
    'introduction',
    'overview',
    'architecture overview',
  ];
  for (const title of overviewHeadingTitles) {
    if (overview) break;
    const heading = h2s.find((h) => h.title.toLowerCase() === title);
    if (!heading) continue;
    const next = h2s[h2s.indexOf(heading) + 1];
    const body = sectionBodyLines(sliceSection(lines, heading, next));
    if (hasNonEmptyBody(body)) {
      overview = firstParagraph(body);
    }
  }

  if (!overview) {
    // First non-empty PROSE paragraph of the body. Drop every ATX heading
    // (H1–H6), not just the H1 title, so the overview is never a bare heading
    // line like "## Overview" (which would otherwise be picked up here when a
    // doc's overview heading isn't one of the named ones above).
    const body = lines.filter((line) => !/^#{1,6}\s/.test(line));
    overview = firstParagraph(body);
  }

  // ── Truncation ──────────────────────────────────────────────
  let overviewTruncated = false;
  if (overview.length > DESIGN_OVERVIEW_MAX_CHARS) {
    overview = overview.slice(0, DESIGN_OVERVIEW_MAX_CHARS) + '…';
    overviewTruncated = true;
  }

  // ── Sections ────────────────────────────────────────────────
  const sections: DesignSection[] = [];
  for (let i = 0; i < h2s.length; i++) {
    const heading = h2s[i]!;
    const next = h2s[i + 1];
    const sectionLines = sliceSection(lines, heading, next);
    const body = sectionBodyLines(sectionLines);
    if (!hasNonEmptyBody(body)) continue;
    sections.push({
      title: heading.title,
      detailBody: sectionLines.join('\n'),
    });
  }

  return { overview, overviewTruncated, sections };
}

/**
 * Unified diff renderer for lite mode write/edit tool calls. Row layout is
 * `<line#> <gutter> <highlighted code>`; added/removed rows carry a bg tint
 * to the terminal edge. Long lines wrap with a hanging indent (via
 * wrapAnsiLine, which carries SGR state across rows) so continuations align
 * under row 0's body instead of falling back to col 0.
 */
import chalk from 'chalk';
import { diffLines } from 'diff';
import { visibleWidth } from '../utils/text-width.js';
import {
  wrapAnsiLine,
  resolveLanguageFromPathLite,
  highlightLineSafe,
} from './render/text.js';
import type { RenderTheme } from './render/theme.js';

// Legacy hardcoded fallbacks used when no theme is supplied (tests, snapshot
// callers, mid-mount pure-context renders). Kept in lockstep with
// DEFAULT_RENDER_THEME.diffAddedBg / diffRemovedBg so theme-less callers see
// identical output to the pre-theming code. (= chalk.bgHex('#1F2D22'/'#2D1F22').)
const ADDED_BG_OPEN = '\x1b[48;2;31;45;34m';
const REMOVED_BG_OPEN = '\x1b[48;2;45;31;34m';
// Full SGR reset at row end: closes the bg tint AND any unclosed fg/style left
// mid-token when a wrap boundary lands inside a styled span (else cyan etc.
// bleeds into the next row's hanging indent).
const ROW_RESET = '\x1b[0m';

const ADDED_BAR = '#80ffb5';
const REMOVED_BAR = '#ff8080';

const LINE_NUM_WIDTH = 4;

interface DiffLine {
  type: 'context' | 'added' | 'removed';
  oldNum: number | null;
  newNum: number | null;
  text: string;
}

export interface RenderUnifiedDiffOpts {
  path?: string;
  /**
   * Suppress the leading `  {path}` header row inside the diff body.
   * The path is still consumed for syntax-highlight language detection.
   * Used by the scrollback caller, where the inline arg chip on the
   * tool-call header already shows the path — printing it again as the
   * first row of the diff body would duplicate it. The approval prompt
   * leaves this off because its surrounding chrome doesn't show the
   * path anywhere else.
   */
  suppressPathHeader?: boolean;
  startLine?: number;
  contextLines?: number;
  termCols?: number;
  /** Supplies bg tint + bar colors; falls back to the legacy constants. */
  theme?: RenderTheme;
}

interface DiffStyling {
  addedBgOpen: string;
  removedBgOpen: string;
  addedBarFn: (s: string) => string;
  removedBarFn: (s: string) => string;
}

/**
 * Pull the SGR open sequence from a chalk-style bg wrapper so {@link applyBg}
 * can re-assert it after every full reset cli-highlight emits between tokens.
 * Probes on a single space (not `''`): chalk's empty-input optimization
 * returns `''` even at full truecolor support, which would silently drop
 * every theme bg. Returns `fallback` for non-SGR output or reset-shaped SGR
 * (re-asserting a reset would stack resets and drop the tint).
 */
function extractBgOpen(bgFn: (s: string) => string, fallback: string): string {
  try {
    const styled = bgFn(' ');
    // eslint-disable-next-line no-control-regex
    const m = /^\x1b\[[0-9;]*m/.exec(styled);
    if (!m) return fallback;
    const params = m[0].slice(2, -1); // strip leading "\x1b[" and trailing "m"
    if (params === '' || /^[0;]+$/.test(params)) return fallback;
    return m[0];
  } catch {
    return fallback;
  }
}

function resolveDiffStyling(theme?: RenderTheme): DiffStyling {
  if (!theme) {
    return {
      addedBgOpen: ADDED_BG_OPEN,
      removedBgOpen: REMOVED_BG_OPEN,
      addedBarFn: chalk.hex(ADDED_BAR),
      removedBarFn: chalk.hex(REMOVED_BAR),
    };
  }
  return {
    addedBgOpen: extractBgOpen(theme.diffAddedBg, ADDED_BG_OPEN),
    removedBgOpen: extractBgOpen(theme.diffRemovedBg, REMOVED_BG_OPEN),
    addedBarFn: theme.diffAddedBar,
    removedBarFn: theme.diffRemovedBar,
  };
}

export function renderUnifiedDiff(
  oldText: string,
  newText: string,
  opts: RenderUnifiedDiffOpts = {}
): string[] {
  const startLine = opts.startLine ?? 1;
  const contextLines = opts.contextLines ?? 3;
  const termCols = opts.termCols ?? 80;
  // Resolve bg + bar styling once per call so the per-line renderer
  // doesn't have to re-derive it for every row. Theme-driven when
  // opts.theme is supplied; legacy hardcoded constants otherwise.
  const styling = resolveDiffStyling(opts.theme);

  const normOld = oldText.replace(/\n$/, '');
  const normNew = newText.replace(/\n$/, '');

  if (normOld === normNew) return [];

  const changes = diffLines(normOld, normNew);

  const flat: DiffLine[] = [];
  let oldNum = startLine;
  let newNum = startLine;
  for (const change of changes) {
    const lines = change.value.split('\n');
    if (lines.length > 0 && lines[lines.length - 1] === '') lines.pop();
    for (const line of lines) {
      if (change.added) {
        flat.push({
          type: 'added',
          oldNum: null,
          newNum: newNum++,
          text: line,
        });
      } else if (change.removed) {
        flat.push({
          type: 'removed',
          oldNum: oldNum++,
          newNum: null,
          text: line,
        });
      } else {
        flat.push({ type: 'context', oldNum, newNum, text: line });
        oldNum++;
        newNum++;
      }
    }
  }

  type Hunk = { start: number; end: number };
  const hunks: Hunk[] = [];
  let i = 0;
  while (i < flat.length) {
    if (flat[i]!.type === 'context') {
      i++;
      continue;
    }
    let start = i;
    let backCount = 0;
    while (
      start > 0 &&
      flat[start - 1]!.type === 'context' &&
      backCount < contextLines
    ) {
      start--;
      backCount++;
    }
    let end = i;
    while (end < flat.length) {
      if (flat[end]!.type !== 'context') {
        end++;
        continue;
      }
      let lookAhead = end;
      let ctxRun = 0;
      while (
        lookAhead < flat.length &&
        flat[lookAhead]!.type === 'context' &&
        ctxRun < 2 * contextLines
      ) {
        lookAhead++;
        ctxRun++;
      }
      if (lookAhead < flat.length && flat[lookAhead]!.type !== 'context') {
        end = lookAhead;
        continue;
      }
      end = Math.min(end + contextLines, flat.length);
      break;
    }
    hunks.push({ start, end });
    i = end;
  }

  if (hunks.length === 0) return [];

  const out: string[] = [];

  if (opts.path && !opts.suppressPathHeader) {
    out.push(chalk.dim(`  ${opts.path}`));
  }

  const language = resolveLanguageFromPathLite(opts.path);

  for (let h = 0; h < hunks.length; h++) {
    const hunk = hunks[h]!;
    if (h === 0 && hunk.start > 0) {
      out.push(chalk.dim('  ...'));
    } else if (h > 0) {
      const prev = hunks[h - 1]!;
      if (hunk.start > prev.end) out.push(chalk.dim('  ...'));
    }
    for (let j = hunk.start; j < hunk.end; j++) {
      out.push(renderDiffLine(flat[j]!, termCols, styling, language));
    }
    if (h === hunks.length - 1 && hunk.end < flat.length) {
      out.push(chalk.dim('  ...'));
    }
  }

  let added = 0;
  let removed = 0;
  for (const dl of flat) {
    if (dl.type === 'added') added++;
    else if (dl.type === 'removed') removed++;
  }
  const parts: string[] = [];
  if (added > 0) parts.push(`added ${added} ${added === 1 ? 'line' : 'lines'}`);
  if (removed > 0)
    parts.push(`removed ${removed} ${removed === 1 ? 'line' : 'lines'}`);
  if (parts.length > 0) out.push(chalk.dim(`  ${parts.join(', ')}`));

  return out;
}

/**
 * Wrap `inner` in a bg-tinted block so the bg extends to `termCols`.
 *
 * cli-highlight emits `\x1b[0m` (full reset) between tokens. A naive
 * bg wrapper would have the bg cleared at every reset and re-applied
 * only at the next chalk-styled segment, leaving "stripes" inside the
 * row. We re-assert the bg open code right after every reset so the
 * tint stays continuous.
 */
function applyBg(inner: string, bgOpen: string): string {
  // Re-assert bgOpen after every full reset cli-highlight emits between
  // tokens, else the bg clears mid-row and leaves "stripes".
  // eslint-disable-next-line no-control-regex
  const reasserted = inner.replace(/\x1b\[0m/g, `\x1b[0m${bgOpen}`);
  return `${bgOpen}${reasserted}${ROW_RESET}`;
}

function renderDiffLine(
  dl: DiffLine,
  termCols: number,
  styling: DiffStyling,
  language?: string
): string {
  // Anchor the gutter number on the resulting (new) file for context + added
  // rows so it stays monotonic with adjacent additions; removed rows fall back
  // to oldNum since they have no presence in the new file.
  const numStr =
    dl.type === 'removed'
      ? String(dl.oldNum ?? '').padStart(LINE_NUM_WIDTH)
      : String(dl.newNum ?? '').padStart(LINE_NUM_WIDTH);

  const gutterGlyph =
    dl.type === 'added' ? '+' : dl.type === 'removed' ? '-' : ' ';
  // chalk.bold over the bar color so the +/- glyph stays readable on light
  // pastel bar themes (e.g. kiroLight's #5de89d on near-white bg).
  const gutter =
    dl.type === 'added'
      ? chalk.bold(styling.addedBarFn(gutterGlyph))
      : dl.type === 'removed'
        ? chalk.bold(styling.removedBarFn(gutterGlyph))
        : ' ';

  // linePrefix = 2-space indent + padded line# + space. headWidth adds the
  // gutter cell; continuation rows hang-indent to headWidth so the bg block
  // forms a clean rectangle aligned under row 0's body.
  const linePrefix = `  ${numStr} `;
  const headWidth = visibleWidth(linePrefix) + 1;
  // BODY_INSET sits inside the bg block so the tint reaches the gutter on
  // every row; mirrored on (un-tinted) context rows for consistent alignment.
  const BODY_INSET = '  ';
  const innerCols = Math.max(8, termCols - headWidth - BODY_INSET.length);

  // Highlight the FULL source line in one shot so cli-highlight's line-at-a-
  // time tokenizer colors tokens correctly even when they later wrap.
  const styled = highlightLineSafe(dl.text, language);

  // wrapAnsiLine splits into visual rows while carrying SGR state across
  // boundaries; each row then gets its own bg block + hanging indent so the
  // tint reaches the terminal edge on every row, including the last.
  const rows = wrapAnsiLine(styled, innerCols, innerCols);
  if (rows.length === 0) rows.push('');

  if (dl.type === 'context') {
    const hangingIndent = ' '.repeat(headWidth + BODY_INSET.length);
    return rows
      .map((row, i) =>
        i === 0
          ? chalk.dim(linePrefix) + gutter + BODY_INSET + chalk.dim(row)
          : hangingIndent + chalk.dim(row)
      )
      .join('\n');
  }

  // added / removed: each row gets its own bg-tinted block padded to innerCols
  // (bg reaches the right edge). Continuation rows skip the +/- glyph —
  // repeating it would read as a new diff line.
  const bgOpen =
    dl.type === 'added' ? styling.addedBgOpen : styling.removedBgOpen;
  const hangingIndent = ' '.repeat(headWidth);
  return rows
    .map((row, i) => {
      const rowVis = visibleWidth(row);
      const padCount = Math.max(0, innerCols - rowVis);
      const innerPlain = BODY_INSET + row + ' '.repeat(padCount);
      if (i === 0) {
        return chalk.dim(linePrefix) + gutter + applyBg(innerPlain, bgOpen);
      }
      return hangingIndent + applyBg(innerPlain, bgOpen);
    })
    .join('\n');
}

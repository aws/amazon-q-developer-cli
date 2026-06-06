/**
 * Unified diff renderer for lite mode write/edit tool calls.
 *
 * Each row is laid out as:
 *
 *     <line#>  <gutter>  <syntax-highlighted code>
 *
 * - The gutter is its own colored cell (`-` red bar, `+` green bar, ` ` for
 *   context). It's NOT prepended onto the line text — repeating `+/-` glyphs
 *   inside the body row read as visual noise.
 * - Added/removed rows get a soft bg tint that extends to the terminal edge.
 * - Code is syntax-highlighted with cli-highlight while the bg tint stays
 *   intact (we re-assert the bg after every full ANSI reset the highlighter
 *   emits between tokens).
 * - Long source lines are manually wrapped at the terminal width with a
 *   hanging indent so wrapped continuation rows align flush under the start
 *   of the body on row 0 — instead of falling back to col 0 like terminal
 *   soft-wrap would do, which made wrapped lines read as visually
 *   disconnected from their gutter. Highlighting is applied to the FULL
 *   logical source line first; the wrap operates on the styled output via
 *   `wrapAnsiLine`, which preserves zero-width SGR escapes across wrap
 *   boundaries so each continuation row inherits whatever style was
 *   active mid-token. The bg block is re-emitted per visual row and
 *   padded to fill the row width, so the diff's signature edge-to-edge
 *   tint holds for every row of a wrapped line, not just one-row entries.
 */
import chalk from 'chalk';
import { diffLines } from 'diff';
import { highlight } from 'cli-highlight';
import { visibleWidth } from '../utils/text-width.js';
import { resolveHighlightLanguage } from '../utils/highlight-languages.js';
import { wrapAnsiLine } from './render/text.js';
import type { RenderTheme } from './render/theme.js';

// Legacy hardcoded SGR escapes used when no theme is supplied (tests,
// snapshot callers, mid-mount pure-context renders). When LiteLayout
// (or the approval prompt) passes a theme via {@link RenderUnifiedDiffOpts},
// the renderer derives equivalent SGR from `theme.diffAddedBg` /
// `theme.diffRemovedBg` and routes the gutter glyph through
// `theme.diffAddedBar` / `theme.diffRemovedBar`. /settings theme then
// flips kiroDark ↔ kiroLight bg + bar values for new diffs without
// touching already-flushed scrollback (twinki Static immutability).
//
// The bg open codes below correspond to `chalk.bgHex('#1F2D22')` and
// `chalk.bgHex('#2D1F22')` respectively — kept in lockstep with
// `DEFAULT_RENDER_THEME.diffAddedBg` / `diffRemovedBg` in render.ts so
// theme-less callers see identical output to the pre-theming code.
const ADDED_BG_OPEN = '\x1b[48;2;31;45;34m';
const REMOVED_BG_OPEN = '\x1b[48;2;45;31;34m';
// Full SGR reset at the end of each rendered visual row. Closes the bg
// tint AND any unclosed fg/style left mid-token by the highlighter when
// a wrap boundary lands inside a styled span — without it, a row that
// ends in the middle of a `\x1b[36m...` token would carry cyan into the
// next row's hanging-indent spaces. (cli-highlight does close every
// token at end of input, so 1-row lines never hit this case; the safer
// reset is a no-op there.)
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
  /** 1-based start line in the source file (defaults to 1). */
  startLine?: number;
  /** Lines of unchanged context to include around each hunk. */
  contextLines?: number;
  /** Terminal columns for full-row background fill. */
  termCols?: number;
  /**
   * Per-render theme. Supplies the bg tint and bar (gutter glyph) colors
   * for added/removed rows so /theme bundled:dark|light flips the diff
   * palette in lockstep with the rest of lite scrollback. When omitted
   * (tests, snapshot callers, mid-mount pure-context renders), the
   * renderer falls back to the legacy hardcoded SGR constants above so
   * existing fixtures stay green.
   */
  theme?: RenderTheme;
}

/**
 * Resolved bg open codes + bar wrappers for a single diff render. Built
 * once per {@link renderUnifiedDiff} call from {@link RenderUnifiedDiffOpts.theme}
 * (or the legacy fallbacks when no theme is passed) and threaded into
 * each {@link renderDiffLine} call. Keeps the per-line render free of
 * theme branching — every line just reads `addedBgOpen` / `addedBarFn`
 * from the same struct.
 */
interface DiffStyling {
  addedBgOpen: string;
  removedBgOpen: string;
  addedBarFn: (s: string) => string;
  removedBarFn: (s: string) => string;
}

/**
 * Pull the SGR open sequence from a chalk-style bg wrapper. Calling the
 * wrapper on a single-space placeholder (`' '`) produces
 * `<openSGR><space><closeSGR>` (e.g. `\x1b[48;2;45;58;48m \x1b[49m`); we
 * want just the leading `\x1b[48;...m` part so {@link applyBg} can
 * re-assert it after every full reset cli-highlight emits between
 * syntax tokens. Tolerates truecolor (`48;2;R;G;B`), 256-color
 * (`48;5;N`), and named-color (`40-47`) bg formats by matching any
 * leading SGR.
 *
 * The non-empty sentinel matters: chalk has an empty-input → empty-
 * output optimization (`chalk.bgHex(hex)('')` returns `''` even at full
 * truecolor support), so probing on `''` would always trip the regex
 * fallback even for correctly-configured themes — losing every theme
 * bg silently.
 *
 * Returns `fallback` when the wrapper produces unexpected output (no
 * leading SGR — e.g. a plain `(s) => s` noop or a custom theme that
 * doesn't emit chalk-style escapes). Without a proper SGR open code
 * to re-assert, the highlighter's full resets would drop the tint
 * mid-row, so falling back to the legacy bg keeps the diff visually
 * intact even with broken theme contributions.
 */
function extractBgOpen(bgFn: (s: string) => string, fallback: string): string {
  try {
    const styled = bgFn(' ');
    // eslint-disable-next-line no-control-regex
    const m = /^\x1b\[[0-9;]*m/.exec(styled);
    if (!m) return fallback;
    // Reject reset-shaped SGR (`\x1b[m`, `\x1b[0m`, `\x1b[0;0m`, etc.).
    // A wrapper that emits a reset as its "open" would, when re-asserted
    // after every cli-highlight reset via {@link applyBg}, just stack
    // additional resets on top — dropping the diff's bg tint silently.
    // The legacy fallback keeps the row visually intact instead.
    const params = m[0].slice(2, -1); // strip leading "\x1b[" and trailing "m"
    if (params === '' || /^[0;]+$/.test(params)) return fallback;
    return m[0];
  } catch {
    return fallback;
  }
}

/**
 * Build the per-render styling struct from an optional theme. The
 * legacy branch returns the prior hardcoded SGR + chalk wrappers so
 * theme-less callers (tests, snapshot fixtures) see identical output
 * to the pre-theming renderer.
 */
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

  const language = resolveLanguageFromPath(opts.path);

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

function resolveLanguageFromPath(path?: string): string | undefined {
  if (!path) return undefined;
  const base = path.split('/').pop() ?? path;
  const ext = base.includes('.') ? base.split('.').pop() : undefined;
  return resolveHighlightLanguage(ext);
}

function highlightSafe(code: string, language?: string): string {
  if (!code || !language || language === 'plaintext') return code;
  try {
    const orig = console.error;
    console.error = () => {};
    try {
      return highlight(code, { language });
    } finally {
      console.error = orig;
    }
  } catch {
    return code;
  }
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
  // `\x1b` is the literal ANSI ESC byte we're matching — eslint's
  // no-control-regex flags any ASCII control char in a regex, but here it's
  // the exact byte cli-highlight emits in its full-reset sequence.
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
  // Use the post-edit (new) line number for context + added rows so the
  // gutter stays monotonic with adjacent additions; removed rows fall back
  // to oldNum since they have no presence in the new file. Mixing oldNum on
  // context with newNum on additions made the column read 46+, 47+, 20,
  // 21-, 22-, 48+ — the eye loses track of which file the number belongs
  // to, so we anchor on the resulting file.
  const numStr =
    dl.type === 'removed'
      ? String(dl.oldNum ?? '').padStart(LINE_NUM_WIDTH)
      : String(dl.newNum ?? '').padStart(LINE_NUM_WIDTH);

  const gutterGlyph =
    dl.type === 'added' ? '+' : dl.type === 'removed' ? '-' : ' ';
  // Compose the bar wrapper with chalk.bold so the +/- glyph stays
  // readable on themes whose bar color is a light pastel (e.g.
  // kiroLight's `#5de89d` on a near-white bg). The bold applies on top
  // of the theme color, so the SGR is `<bold-open><color-open>+<color-close><bold-close>`
  // — visually identical to the legacy `chalk.hex(BAR).bold(glyph)` chain.
  const gutter =
    dl.type === 'added'
      ? chalk.bold(styling.addedBarFn(gutterGlyph))
      : dl.type === 'removed'
        ? chalk.bold(styling.removedBarFn(gutterGlyph))
        : ' ';

  // 2-space indent + line# + space = LINE_NUM_WIDTH + 3 cells.
  const linePrefix = `  ${numStr} `;
  // headWidth covers everything left of the body inset: linePrefix +
  // gutter cell. A continuation row's hanging indent equals headWidth
  // (so the bg block on continuation rows starts at the same column as
  // row 0's bg block, with the body inset rendered INSIDE the bg block
  // on every row — wrapped content lines up vertically and the tint
  // forms a clean rectangle for the wrapped span).
  const headWidth = visibleWidth(linePrefix) + 1;
  // 2-space inset between gutter and body, kept inside the bg block so
  // the tint extends to the gutter on every row. Same value used for
  // context rows below — context has no bg but mirroring the inset
  // keeps the visual alignment consistent across context/added/removed.
  const BODY_INSET = '  ';
  const innerCols = Math.max(8, termCols - headWidth - BODY_INSET.length);

  // Highlight the FULL source line in one shot. cli-highlight tokenizes
  // line-at-a-time, so passing the whole logical line gives correct
  // token coloring even for tokens that later land across a wrap
  // boundary in the rendered output. Per-chunk highlighting (the older
  // pre-soft-wrap behavior) re-tokenized partial fragments and produced
  // neutral or wrong coloring on continuation rows.
  const styled = highlightSafe(dl.text, language);

  // Manual wrap: the styled string flows through `wrapAnsiLine` so each
  // visual row becomes its own entry, with the wrapper preserving zero-
  // width SGR escapes across cell boundaries. We then render each row
  // with its own bg block + hanging indent so wrapped continuation rows
  // align under the body of row 0 instead of falling back to col 0
  // (which is what terminal soft-wrap would have done — see commit
  // history for the soft-wrap → manual-wrap migration). One row per
  // visual line means every row's bg can extend to the terminal edge,
  // including the last row of a multi-row wrap.
  const rows = wrapAnsiLine(styled, innerCols, innerCols);
  if (rows.length === 0) rows.push('');

  if (dl.type === 'context') {
    // Context rows have no bg tint, just dim styling. Hanging indent on
    // continuation rows pads to BODY_INSET past headWidth so wrapped
    // source aligns flush with the start of row 0's body content.
    const hangingIndent = ' '.repeat(headWidth + BODY_INSET.length);
    return rows
      .map((row, i) =>
        i === 0
          ? chalk.dim(linePrefix) + gutter + BODY_INSET + chalk.dim(row)
          : hangingIndent + chalk.dim(row)
      )
      .join('\n');
  }

  // added / removed: each row gets its own bg-tinted block.
  //
  // Padding: every visual row pads its body to innerCols so the bg
  // block extends to the terminal's right edge — the diff's signature
  // visual. The previous (soft-wrap) implementation only padded one-row
  // lines; multi-row lines left a partial last visual row un-tinted.
  // Manual wrap means we know each row's exact width here, so we can
  // pad consistently across the whole diff.
  //
  // Continuation rows DON'T re-render the +/- gutter glyph — repeating
  // the glyph would read as a new diff line of the same kind. The bg
  // tint and the row's vertical alignment under the body of row 0 are
  // sufficient signals that the row is a wrap continuation.
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

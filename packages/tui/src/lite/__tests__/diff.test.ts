import './setup-chalk-level.js';

import { describe, test, expect } from 'vitest';
import { renderUnifiedDiff } from '../diff.js';
import type { RenderTheme } from '../render.js';
import { visibleWidth } from '../../utils/text-width.js';
import stripAnsi from 'strip-ansi';

const entryContaining = (out: string[], needle: string) => {
  const e = out.find((x) => stripAnsi(x).includes(needle));
  expect(e).toBeDefined();
  return e!;
};

/**
 * The diff renderer manually wraps long source lines so wrapped continuation
 * rows align under the body of row 0 (a hanging indent that lines up with
 * the column where the body starts after the line-number gutter and `+`/`-`
 * marker).
 *
 * The earlier "emit as ONE logical line and let the terminal soft-wrap"
 * approach kept triple-click selection and URL copy-paste perfect, but
 * terminal soft-wrap has no concept of hanging indent — wrapped rows
 * fell back to col 0 and the diff read as visually disconnected from
 * its gutter. Manual wrap trades per-row triple-click selection for the
 * indented-and-tinted visual that matches what users expect from a code
 * diff. The bg tint + syntax highlighting still travel correctly across
 * wrap boundaries because we wrap the already-styled output via
 * `wrapAnsiLine`, which preserves zero-width SGR escapes across cell
 * boundaries.
 */
describe('renderUnifiedDiff — wrapping', () => {
  // Column geometry shared across tests. linePrefix = `'  ' + numStr (4) +
  // ' '` = 7 cols, plus a 1-col gutter cell = 8 cols of head. The body
  // (after gutter) gets a 2-col inset before content begins, so wrapped
  // continuation rows must indent to col 10 to align under the start of
  // row 0's source text.
  const HEAD_WIDTH = 8;
  const BODY_START_COL = 10;

  const diffFor = (
    oldText: string,
    newText: string,
    opts: Parameters<typeof renderUnifiedDiff>[2] = {
      path: 'src/foo.ts',
      termCols: 40,
    }
  ) => renderUnifiedDiff(oldText, newText, opts);

  test('a long added line wraps onto multiple rows with hanging indent', () => {
    const longCode =
      'const reallyLongVariableNameHere = someFunctionCall(argument1, argument2, argument3);';
    const addedEntry = entryContaining(diffFor('', longCode), 'reallyLong');
    const visualRows = stripAnsi(addedEntry).split('\n');
    expect(visualRows.length).toBeGreaterThanOrEqual(2);
    // Row 0 carries the line# and the `+` gutter glyph; continuation rows are
    // pure hanging indent to BODY_START_COL then the wrapped body.
    expect(visualRows[0]!).toMatch(/^\s+1\s+\+/);
    for (let i = 1; i < visualRows.length; i++) {
      const row = visualRows[i]!;
      expect(row.startsWith(' '.repeat(BODY_START_COL))).toBe(true);
      expect(row.trim().length).toBeGreaterThan(0);
    }
    // wrapAnsiLine eats inter-word spaces at boundaries but never splits a
    // word, so every source word must still appear in order.
    const concat = visualRows
      .map((row) => row.trimEnd())
      .join(' ')
      .replace(/\s+/g, ' ');
    for (const word of longCode.split(/\s+/)) {
      expect(concat).toContain(word);
    }
  });

  test('a long context line wraps with the same hanging indent', () => {
    const longComment =
      '// pretend this is a really long pre-existing comment line in the source file';
    const ctxEntry = entryContaining(
      diffFor(`${longComment}\nold body`, `${longComment}\nnew body`),
      'pretend this'
    );
    const visualRows = stripAnsi(ctxEntry).split('\n');
    expect(visualRows.length).toBeGreaterThanOrEqual(2);
    // Row 0: line# + space gutter (no `+`/`-`).
    expect(visualRows[0]!).toMatch(/^\s+1\s/);
    expect(visualRows[0]!).not.toMatch(/^\s+1\s+[+-]/);
    for (let i = 1; i < visualRows.length; i++) {
      expect(visualRows[i]!.startsWith(' '.repeat(BODY_START_COL))).toBe(true);
    }
  });

  test('continuation rows do not repeat the gutter glyph', () => {
    const addedEntry = entryContaining(diffFor('', 'a'.repeat(200)), 'aaaa');
    const visualRows = stripAnsi(addedEntry).split('\n');
    const gutterCol = HEAD_WIDTH - 1;
    for (let i = 1; i < visualRows.length; i++) {
      expect(visualRows[i]!.charAt(gutterCol)).toBe(' ');
    }
  });

  test('every wrapped row pads its bg block to termCols', () => {
    // bg tint must extend to the right edge on EVERY wrap row, not just
    // single-row entries (the pre-fix soft-wrap left the last row ragged).
    const addedEntry = entryContaining(diffFor('', 'b'.repeat(200)), 'bbbb');
    const visualRows = addedEntry.split('\n');
    expect(visualRows.length).toBeGreaterThan(1);
    for (const vr of visualRows) {
      expect(visibleWidth(stripAnsi(vr))).toBe(40);
    }
  });

  test('bg + highlight ANSI is re-emitted per wrapped row', () => {
    // Each row must re-open the bg and end with a reset so a mid-token-wrap
    // unclosed fg can't bleed into the next row's hanging indent.
    const longCode =
      'const reallyLongVariableNameHere = someFunctionCall(argument1, argument2, argument3);';
    const addedEntry = entryContaining(diffFor('', longCode), 'reallyLong');
    const visualRows = addedEntry.split('\n');
    expect(visualRows.length).toBeGreaterThan(1);
    for (const vr of visualRows) {
      // eslint-disable-next-line no-control-regex
      expect(vr).toMatch(/\x1b\[48;2;31;45;34m/);
      expect(vr.endsWith('\x1b[0m')).toBe(true);
    }
  });

  test('short added line still pads to termCols for the bg block effect', () => {
    const addedEntry = entryContaining(diffFor('', 'short'), 'short');
    expect(addedEntry.split('\n').length).toBe(1);
    expect(visibleWidth(stripAnsi(addedEntry))).toBe(40);
  });

  test('syntax highlighting carries through wrap boundaries', () => {
    // The wrapAnsiLine SGR-carryover pass must re-emit active open codes at
    // each continuation row so a highlight span that opens on row 0 and runs
    // past the wrap doesn't drop to default style mid-line. A long contiguous
    // comment gets one cli-highlight SGR span, so the wrap lands mid-span.
    const longComment =
      '// This is an intentionally very long single-line comment that is designed to exceed the terminal width during testing of the diff renderer';
    const addedEntry = entryContaining(
      diffFor('', longComment),
      'intentionally'
    );
    const visualRows = addedEntry.split('\n');
    expect(visualRows.length).toBeGreaterThan(1);
    // Pull the comment-color SGR sequence out of row 0 (whichever
    // sequence cli-highlight chose for `comment` tokens). The first
    // non-bg SGR sequence emitted AFTER the bg-open on row 0 is the
    // body's syntax-highlight color — that's what must propagate to
    // every continuation row. (Earlier SGR seqs on row 0 belong to
    // the line# / gutter chrome and are scoped to row 0 only.)
    // eslint-disable-next-line no-control-regex
    const sgrRe = /\x1b\[[0-9;]*m/g;
    const row0 = visualRows[0]!;
    const bgOpenIdx = row0.indexOf('\x1b[48;');
    expect(bgOpenIdx).toBeGreaterThanOrEqual(0);
    const afterBg = row0.slice(bgOpenIdx);
    sgrRe.lastIndex = 0;
    let commentOpen: string | undefined;
    let m: RegExpExecArray | null;
    while ((m = sgrRe.exec(afterBg)) !== null) {
      if (m[0].startsWith('\x1b[48;')) continue; // bg open
      if (m[0] === '\x1b[0m') break; // body ended without a body-color seq
      commentOpen = m[0];
      break;
    }
    if (!commentOpen) {
      // Bun can run cli-highlight without token color SGR. In that mode
      // there is no body color to propagate; the bg carryover is covered by
      // the adjacent "bg + highlight ANSI is re-emitted" test.
      return;
    }
    expect(commentOpen).toBeDefined();
    for (let i = 1; i < visualRows.length; i++) {
      expect(visualRows[i]!).toContain(commentOpen!);
    }
  });
});

/**
 * Theme support: write/edit diffs route bg + bar colors through the active
 * /settings theme; no theme falls back to the legacy hardcoded SGR constants.
 *
 * Tests use HAND-CRAFTED SGR strings (not `chalk.bgHex(...)`) so they're
 * deterministic regardless of the CI runtime's `chalk.level` — chalk would
 * otherwise downsample/strip truecolor and false-fail the `48;2;R;G;B` checks.
 */
function makeThemeWith(diffSlots: {
  diffAddedBg: (s: string) => string;
  diffRemovedBg: (s: string) => string;
  diffAddedBar: (s: string) => string;
  diffRemovedBar: (s: string) => string;
}): RenderTheme {
  const noop = (s: string) => s;
  return {
    brand: noop,
    responseChip: noop,
    userTag: noop,
    userBody: noop,
    inlineCode: noop,
    link: noop,
    secondary: noop,
    ...diffSlots,
  };
}

describe('renderUnifiedDiff — theme support', () => {
  const LEGACY_ADDED = /\x1b\[48;2;31;45;34m/; // #1F2D22
  const themeFor = (overrides: Partial<Parameters<typeof makeThemeWith>[0]>) =>
    makeThemeWith({
      diffAddedBg: (s) => '\x1b[48;2;10;200;50m' + s + '\x1b[49m',
      diffRemovedBg: (s) => '\x1b[48;2;204;0;0m' + s + '\x1b[49m',
      diffAddedBar: (s) => '\x1b[38;2;255;0;255m' + s + '\x1b[39m',
      diffRemovedBar: (s) => '\x1b[38;2;153;0;0m' + s + '\x1b[39m',
      ...overrides,
    });
  const addedShort = (theme?: RenderTheme) =>
    entryContaining(
      renderUnifiedDiff('', 'short', {
        path: 'src/foo.ts',
        termCols: 40,
        theme,
      }),
      'short'
    );

  test('legacy SGR is used when no theme is supplied', () => {
    // Locks in the no-theme fallback contract so a swapped default constant
    // surfaces here, not just via downstream snapshot breakage.
    expect(addedShort()).toMatch(LEGACY_ADDED);
  });

  test('theme-supplied bg SGR replaces the legacy constants for added rows', () => {
    // Distinctive RGB (10,200,50 bg / 255,0,255 bar) so the assertion can't
    // accidentally pass against any shipped kiroDark/kiroLight value.
    const addedEntry = addedShort(themeFor({}));
    // eslint-disable-next-line no-control-regex
    expect(addedEntry).toMatch(/\x1b\[48;2;10;200;50m/);
    // Renderer must replace, not stack, the legacy SGR.
    expect(addedEntry).not.toMatch(LEGACY_ADDED);
    // eslint-disable-next-line no-control-regex
    expect(addedEntry).toMatch(/\x1b\[38;2;255;0;255m/);
  });

  test('theme-supplied removed slots replace legacy SGR for removed rows', () => {
    const theme = themeFor({
      diffRemovedBg: (s) => '\x1b[48;2;204;85;170m' + s + '\x1b[49m',
      diffRemovedBar: (s) => '\x1b[38;2;0;221;238m' + s + '\x1b[39m',
    });
    const removedEntry = entryContaining(
      renderUnifiedDiff('vanish', '', {
        path: 'src/foo.ts',
        termCols: 40,
        theme,
      }),
      'vanish'
    );
    // eslint-disable-next-line no-control-regex
    expect(removedEntry).toMatch(/\x1b\[48;2;204;85;170m/);
    // eslint-disable-next-line no-control-regex
    expect(removedEntry).not.toMatch(/\x1b\[48;2;45;31;34m/);
    // eslint-disable-next-line no-control-regex
    expect(removedEntry).toMatch(/\x1b\[38;2;0;221;238m/);
  });

  test('theme bg SGR is re-asserted across wrap boundaries', () => {
    // applyBg re-applies the bg after each cli-highlight reset; when the bg
    // is theme-supplied that re-assertion must use the theme SGR. Mid-wrap
    // rows are the strictest check (per-row re-emit).
    const themeBgSgr = '\x1b[48;2;77;88;99m';
    const theme = themeFor({ diffAddedBg: (s) => themeBgSgr + s + '\x1b[49m' });
    const longCode =
      'const longThing = someFunc(arg1, arg2, arg3, arg4, arg5, arg6);';
    const addedEntry = entryContaining(
      renderUnifiedDiff('', longCode, {
        path: 'src/foo.ts',
        termCols: 40,
        theme,
      }),
      'longThing'
    );
    const visualRows = addedEntry.split('\n');
    expect(visualRows.length).toBeGreaterThan(1);
    for (const vr of visualRows) {
      expect(vr).toContain(themeBgSgr);
      expect(vr.endsWith('\x1b[0m')).toBe(true);
      expect(vr).not.toMatch(LEGACY_ADDED);
    }
  });

  // A broken diffAddedBg wrapper must not crash or drop the bg tint — the
  // renderer falls back to the legacy SGR. extractBgOpen rejects reset-shaped
  // "opens" so a wrapper returning a bare reset also falls back rather than
  // stacking resets that silently drop the tint.
  test.each([
    [
      'throws',
      () => {
        throw new Error('synthetic theme failure');
      },
    ],
    ['returns plain text', (s: string) => s],
    ['emits a reset', (s: string) => '\x1b[m' + s + '\x1b[m'],
  ] as const)(
    'falls back to legacy SGR when a theme bg wrapper %s',
    (_name, diffAddedBg) => {
      expect(addedShort(themeFor({ diffAddedBg }))).toMatch(LEGACY_ADDED);
    }
  );
});

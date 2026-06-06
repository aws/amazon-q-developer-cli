import { describe, test, expect } from 'vitest';
import { renderUnifiedDiff } from '../diff.js';
import type { RenderTheme } from '../render.js';
import { visibleWidth } from '../../utils/text-width.js';
import stripAnsi from 'strip-ansi';

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

  test('a long added line wraps onto multiple rows with hanging indent', () => {
    // Pick source text that's clearly wider than termCols so the wrap
    // fires deterministically.
    const longCode =
      'const reallyLongVariableNameHere = someFunctionCall(argument1, argument2, argument3);';
    const out = renderUnifiedDiff('', longCode, {
      path: 'src/foo.ts',
      termCols: 40,
    });
    // The added line is one entry in `out`. Each entry's text contains
    // embedded `\n`s when wrap continuation rows are rendered.
    const addedEntry = out.find((e) => stripAnsi(e).includes('reallyLong'));
    expect(addedEntry).toBeDefined();
    const visualRows = stripAnsi(addedEntry!).split('\n');
    expect(visualRows.length).toBeGreaterThanOrEqual(2);
    // Row 0 carries the line# and the `+` gutter glyph at col HEAD_WIDTH-1.
    expect(visualRows[0]!).toMatch(/^\s+1\s+\+/);
    // Continuation rows have no line# and no `+` glyph — they're pure
    // hanging indent up through BODY_START_COL, then the wrapped body.
    for (let i = 1; i < visualRows.length; i++) {
      const row = visualRows[i]!;
      expect(row.startsWith(' '.repeat(BODY_START_COL))).toBe(true);
      expect(row.trim().length).toBeGreaterThan(0);
    }
    // Every word in the source must appear, in order, somewhere in the
    // concatenated visual-row content. wrapAnsiLine eats inter-word
    // spaces at wrap boundaries (so the source can't be reconstructed
    // verbatim) but it never splits a word across rows when a space is
    // available — so the order of words is preserved.
    const concat = visualRows
      .map((row) => row.trimEnd())
      .join(' ')
      .replace(/\s+/g, ' ');
    for (const word of longCode.split(/\s+/)) {
      expect(concat).toContain(word);
    }
  });

  test('a long context line wraps with the same hanging indent', () => {
    // Context lines pick up the same hanging-indent treatment so a wrap
    // inside an unchanged comment aligns visually with its surrounding
    // body — otherwise context wraps would dip back to col 0 while
    // bg-tinted neighbors stayed at the body column, producing a
    // ragged left edge through the diff.
    const longComment =
      '// pretend this is a really long pre-existing comment line in the source file';
    const oldText = `${longComment}\nold body`;
    const newText = `${longComment}\nnew body`;
    const out = renderUnifiedDiff(oldText, newText, {
      path: 'src/foo.ts',
      termCols: 40,
    });
    const ctxEntry = out.find((e) => stripAnsi(e).includes('pretend this'));
    expect(ctxEntry).toBeDefined();
    const visualRows = stripAnsi(ctxEntry!).split('\n');
    expect(visualRows.length).toBeGreaterThanOrEqual(2);
    // Row 0: line# + space gutter (no `+`/`-`).
    expect(visualRows[0]!).toMatch(/^\s+1\s/);
    expect(visualRows[0]!).not.toMatch(/^\s+1\s+[+-]/);
    // Continuation rows: pure hanging indent then content.
    for (let i = 1; i < visualRows.length; i++) {
      expect(visualRows[i]!.startsWith(' '.repeat(BODY_START_COL))).toBe(true);
    }
  });

  test('continuation rows do not repeat the gutter glyph', () => {
    // A wrapped `+` line should not produce a second `+` glyph in any
    // continuation row's gutter column — the bg tint and alignment
    // under row 0's body are sufficient signals that the row is part
    // of the same additive line. Repeating the glyph would read as a
    // brand-new diff line of the same kind.
    const longCode = 'a'.repeat(200);
    const out = renderUnifiedDiff('', longCode, {
      path: 'src/foo.ts',
      termCols: 40,
    });
    const addedEntry = out.find((e) => stripAnsi(e).includes('aaaa'));
    expect(addedEntry).toBeDefined();
    const visualRows = stripAnsi(addedEntry!).split('\n');
    // Every continuation row has a space at the gutter column, not `+`/`-`.
    const gutterCol = HEAD_WIDTH - 1;
    for (let i = 1; i < visualRows.length; i++) {
      expect(visualRows[i]!.charAt(gutterCol)).toBe(' ');
    }
  });

  test('every wrapped row pads its bg block to termCols', () => {
    // The diff's signature visual — bg tint extending to the right edge
    // of the terminal — must hold for every row of a wrap, not just
    // single-row entries. The pre-fix soft-wrap implementation only
    // padded one-row lines; multi-row wraps left the partial last row
    // with a ragged un-tinted right edge. Manual wrap fixes that by
    // knowing each row's exact width and padding accordingly.
    const longCode = 'b'.repeat(200);
    const out = renderUnifiedDiff('', longCode, {
      path: 'src/foo.ts',
      termCols: 40,
    });
    const addedEntry = out.find((e) => stripAnsi(e).includes('bbbb'));
    expect(addedEntry).toBeDefined();
    const visualRows = addedEntry!.split('\n');
    expect(visualRows.length).toBeGreaterThan(1);
    for (const vr of visualRows) {
      expect(visibleWidth(stripAnsi(vr))).toBe(40);
    }
  });

  test('bg + highlight ANSI is re-emitted per wrapped row', () => {
    // Hard guarantee that styling re-opens at the start of each wrap
    // row and resets at the end. Each visual row must carry an
    // ADDED_BG_OPEN sequence and end with `\x1b[0m` so any unclosed fg
    // from a mid-token wrap can't bleed into the next row's hanging
    // indent. The reasserted-bg pattern (bg re-applied after every
    // internal cli-highlight reset) is unchanged within each row.
    const longCode =
      'const reallyLongVariableNameHere = someFunctionCall(argument1, argument2, argument3);';
    const out = renderUnifiedDiff('', longCode, {
      path: 'src/foo.ts',
      termCols: 40,
    });
    const addedEntry = out.find((e) => stripAnsi(e).includes('reallyLong'));
    expect(addedEntry).toBeDefined();
    const visualRows = addedEntry!.split('\n');
    expect(visualRows.length).toBeGreaterThan(1);
    for (const vr of visualRows) {
      // eslint-disable-next-line no-control-regex
      expect(vr).toMatch(/\x1b\[48;2;31;45;34m/);
      expect(vr.endsWith('\x1b[0m')).toBe(true);
    }
  });

  test('short added line still pads to termCols for the bg block effect', () => {
    // Short lines fit on one visual row. They pad to termCols so the
    // bg tint extends to the right edge — same behavior as before the
    // wrap-with-indent change.
    const out = renderUnifiedDiff('', 'short', {
      path: 'src/foo.ts',
      termCols: 40,
    });
    const addedEntry = out.find((e) => stripAnsi(e).includes('short'));
    expect(addedEntry).toBeDefined();
    expect(addedEntry!.split('\n').length).toBe(1);
    expect(visibleWidth(stripAnsi(addedEntry!))).toBe(40);
  });

  test('syntax highlighting carries through wrap boundaries', () => {
    // wrapAnsiLine only attaches captured ANSI escapes to the next
    // visible cell — so a styled span that opens at row 0's first cell
    // and doesn't close until row N's last cell would have its open
    // sequence ONLY on row 0 by default. Continuation rows would
    // render in default style, killing the highlighting in the middle
    // of a wrapped line.
    //
    // The wrapAnsiLine SGR-carryover pass re-emits the active open
    // codes at the start of each continuation row so highlighting
    // persists across wrap boundaries — matching what the terminal
    // soft-wrap does for free on a single logical line, but preserved
    // here for the manual-wrap case.
    //
    // Use a long contiguous comment — cli-highlight applies a single
    // grey/italic SGR span over the whole line, so the wrap WILL land
    // mid-span.
    const longComment =
      '// This is an intentionally very long single-line comment that is designed to exceed the terminal width during testing of the diff renderer';
    const out = renderUnifiedDiff('', longComment, {
      path: 'src/foo.ts',
      termCols: 40,
    });
    const addedEntry = out.find((e) => stripAnsi(e).includes('intentionally'));
    expect(addedEntry).toBeDefined();
    const visualRows = addedEntry!.split('\n');
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
    expect(commentOpen).toBeDefined();
    for (let i = 1; i < visualRows.length; i++) {
      expect(visualRows[i]!).toContain(commentOpen!);
    }
  });
});


/**
 * Theme support: write/edit diffs route their bg + bar colors through the
 * active /settings theme so kiroDark ↔ kiroLight (and any custom theme)
 * actually re-skin the diff. When no theme is supplied — the path tests
 * and snapshot fixtures take — the renderer falls back to the legacy
 * hardcoded SGR constants so existing assertions stay green.
 *
 * The full RenderTheme shape has many fields; the diff renderer only
 * reads four of them. The helper below builds a stub that fulfills the
 * type with no-op stubs for the unused slots and explicit values for
 * the diff slots — keeping each test focused on the colors that
 * actually reach `renderUnifiedDiff`.
 *
 * Tests use HAND-CRAFTED SGR strings (not `chalk.bgHex(...)`) for the
 * theme stubs. Chalk's runtime SGR emission depends on the host
 * terminal's color support — running tests under a CI runner that
 * reports 256-color or no TTY would silently downsample chalk's
 * truecolor output to a different SGR (or strip it entirely),
 * false-failing assertions that expect a specific
 * `48;2;R;G;B` pattern. Hand-crafted strings make these tests
 * deterministic regardless of `chalk.level` at test time.
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
  test('legacy SGR is used when no theme is supplied', () => {
    // Locks in the no-theme fallback path. A regression that swaps the
    // legacy hardcoded constants for some other default would surface as
    // every snapshot test that looked for `48;2;31;45;34m` (#1F2D22)
    // breaking — this test makes the contract explicit instead of
    // relying on the wrap-rows test to catch it.
    const out = renderUnifiedDiff('', 'short', {
      path: 'src/foo.ts',
      termCols: 40,
    });
    const addedEntry = out.find((e) => stripAnsi(e).includes('short'));
    expect(addedEntry).toBeDefined();
    // eslint-disable-next-line no-control-regex
    expect(addedEntry!).toMatch(/\x1b\[48;2;31;45;34m/);
  });

  test('theme-supplied bg SGR replaces the legacy constants for added rows', () => {
    // Distinctive RGB so this assertion can't accidentally pass against
    // any of the kiroDark/kiroLight values shipped today: bg = (10, 200, 50).
    // Hand-crafted SGR (not `chalk.bgHex(...)`) so the test is deterministic
    // regardless of the CI runtime's chalk.level.
    const theme = makeThemeWith({
      diffAddedBg: (s) => '\x1b[48;2;10;200;50m' + s + '\x1b[49m',
      diffRemovedBg: (s) => '\x1b[48;2;204;0;0m' + s + '\x1b[49m',
      diffAddedBar: (s) => '\x1b[38;2;255;0;255m' + s + '\x1b[39m',
      diffRemovedBar: (s) => '\x1b[38;2;153;0;0m' + s + '\x1b[39m',
    });
    const out = renderUnifiedDiff('', 'short', {
      path: 'src/foo.ts',
      termCols: 40,
      theme,
    });
    const addedEntry = out.find((e) => stripAnsi(e).includes('short'));
    expect(addedEntry).toBeDefined();
    // Theme bg SGR (#0AC832 → 48;2;10;200;50) must appear in the row.
    // eslint-disable-next-line no-control-regex
    expect(addedEntry!).toMatch(/\x1b\[48;2;10;200;50m/);
    // Legacy SGR must be absent — the renderer must replace, not stack.
    // eslint-disable-next-line no-control-regex
    expect(addedEntry!).not.toMatch(/\x1b\[48;2;31;45;34m/);
    // Theme bar SGR (#FF00FF → 38;2;255;0;255) lands on the gutter glyph.
    // eslint-disable-next-line no-control-regex
    expect(addedEntry!).toMatch(/\x1b\[38;2;255;0;255m/);
  });

  test('theme-supplied removed slots replace legacy SGR for removed rows', () => {
    // Parallel to the added test, against the removed slot. Picking
    // distinctive removed values (bg #CC55AA, bar #00DDEE) so neither
    // can be confused with the legacy `48;2;45;31;34m` / `#ff8080`.
    const theme = makeThemeWith({
      diffAddedBg: (s) => '\x1b[48;2;10;200;50m' + s + '\x1b[49m',
      diffRemovedBg: (s) => '\x1b[48;2;204;85;170m' + s + '\x1b[49m',
      diffAddedBar: (s) => '\x1b[38;2;255;0;255m' + s + '\x1b[39m',
      diffRemovedBar: (s) => '\x1b[38;2;0;221;238m' + s + '\x1b[39m',
    });
    // Pure removal: oldText has a line, newText is empty.
    const out = renderUnifiedDiff('vanish', '', {
      path: 'src/foo.ts',
      termCols: 40,
      theme,
    });
    const removedEntry = out.find((e) => stripAnsi(e).includes('vanish'));
    expect(removedEntry).toBeDefined();
    // Theme removed bg SGR (#CC55AA → 48;2;204;85;170).
    // eslint-disable-next-line no-control-regex
    expect(removedEntry!).toMatch(/\x1b\[48;2;204;85;170m/);
    // eslint-disable-next-line no-control-regex
    expect(removedEntry!).not.toMatch(/\x1b\[48;2;45;31;34m/);
    // Theme removed bar SGR (#00DDEE → 38;2;0;221;238).
    // eslint-disable-next-line no-control-regex
    expect(removedEntry!).toMatch(/\x1b\[38;2;0;221;238m/);
  });

  test('theme bg SGR is re-asserted across wrap boundaries', () => {
    // The whole point of `applyBg` is that the bg gets re-applied after
    // every full reset cli-highlight emits between syntax tokens. When
    // the bg comes from the theme, that re-assertion must use the
    // theme's SGR — not the legacy constant. Mid-wrap rows are the
    // strictest check because they hit the wrapper's per-row re-emit.
    const themeBgSgr = '\x1b[48;2;77;88;99m';
    const theme = makeThemeWith({
      diffAddedBg: (s) => themeBgSgr + s + '\x1b[49m',
      diffRemovedBg: (s) => '\x1b[48;2;99;77;88m' + s + '\x1b[49m',
      diffAddedBar: (s) => '\x1b[38;2;128;255;181m' + s + '\x1b[39m',
      diffRemovedBar: (s) => '\x1b[38;2;255;128;128m' + s + '\x1b[39m',
    });
    const longCode =
      'const longThing = someFunc(arg1, arg2, arg3, arg4, arg5, arg6);';
    const out = renderUnifiedDiff('', longCode, {
      path: 'src/foo.ts',
      termCols: 40,
      theme,
    });
    const addedEntry = out.find((e) => stripAnsi(e).includes('longThing'));
    expect(addedEntry).toBeDefined();
    const visualRows = addedEntry!.split('\n');
    expect(visualRows.length).toBeGreaterThan(1);
    for (const vr of visualRows) {
      expect(vr).toContain(themeBgSgr);
      expect(vr.endsWith('\x1b[0m')).toBe(true);
      // eslint-disable-next-line no-control-regex
      expect(vr).not.toMatch(/\x1b\[48;2;31;45;34m/);
    }
  });

  test('falls back to legacy SGR when a theme bg wrapper throws', () => {
    // Defensive — a custom theme's wrapper that throws shouldn't crash
    // the renderer. The diff still has to come out, so we expect the
    // legacy SGR to take over for the broken slot.
    const theme = makeThemeWith({
      diffAddedBg: () => {
        throw new Error('synthetic theme failure');
      },
      diffRemovedBg: (s) => '\x1b[48;2;204;0;0m' + s + '\x1b[49m',
      diffAddedBar: (s) => '\x1b[38;2;128;255;181m' + s + '\x1b[39m',
      diffRemovedBar: (s) => '\x1b[38;2;255;128;128m' + s + '\x1b[39m',
    });
    const out = renderUnifiedDiff('', 'short', {
      path: 'src/foo.ts',
      termCols: 40,
      theme,
    });
    const addedEntry = out.find((e) => stripAnsi(e).includes('short'));
    expect(addedEntry).toBeDefined();
    // Legacy SGR re-emerges as the fallback. Not a crash, not stripped.
    // eslint-disable-next-line no-control-regex
    expect(addedEntry!).toMatch(/\x1b\[48;2;31;45;34m/);
  });

  test('falls back to legacy SGR when a theme bg wrapper returns plain text', () => {
    // A wrapper that emits no SGR opens (`(s) => s`) shouldn't make the
    // bg-reapply pattern in `applyBg` lose track of the bg — without a
    // proper SGR open code to re-assert, the highlighter's full resets
    // would drop the tint mid-row. Falling back to the legacy SGR keeps
    // the renderer functional even with broken theme contributions.
    const theme = makeThemeWith({
      diffAddedBg: (s) => s,
      diffRemovedBg: (s) => s,
      diffAddedBar: (s) => '\x1b[38;2;128;255;181m' + s + '\x1b[39m',
      diffRemovedBar: (s) => '\x1b[38;2;255;128;128m' + s + '\x1b[39m',
    });
    const out = renderUnifiedDiff('', 'short', {
      path: 'src/foo.ts',
      termCols: 40,
      theme,
    });
    const addedEntry = out.find((e) => stripAnsi(e).includes('short'));
    expect(addedEntry).toBeDefined();
    // eslint-disable-next-line no-control-regex
    expect(addedEntry!).toMatch(/\x1b\[48;2;31;45;34m/);
  });

  test('falls back to legacy SGR when a theme bg wrapper emits a reset', () => {
    // `\x1b[m` and `\x1b[0m` are SGR resets, not bg opens. A wrapper that
    // returns one of these as its "open" would, when re-asserted after
    // every cli-highlight reset via `applyBg`, just stack additional
    // resets on top — silently dropping the diff's bg tint with no
    // visible error. `extractBgOpen` rejects reset-shaped sequences so
    // the legacy SGR takes over instead, keeping the row visually intact.
    const theme = makeThemeWith({
      diffAddedBg: (s) => '\x1b[m' + s + '\x1b[m',
      diffRemovedBg: (s) => '\x1b[0m' + s + '\x1b[0m',
      diffAddedBar: (s) => '\x1b[38;2;128;255;181m' + s + '\x1b[39m',
      diffRemovedBar: (s) => '\x1b[38;2;255;128;128m' + s + '\x1b[39m',
    });
    const out = renderUnifiedDiff('', 'short', {
      path: 'src/foo.ts',
      termCols: 40,
      theme,
    });
    const addedEntry = out.find((e) => stripAnsi(e).includes('short'));
    expect(addedEntry).toBeDefined();
    // eslint-disable-next-line no-control-regex
    expect(addedEntry!).toMatch(/\x1b\[48;2;31;45;34m/);
  });
});
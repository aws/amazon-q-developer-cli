import './setup-chalk-level.js';

import { describe, test, expect } from 'vitest';
import {
  renderUnifiedDiff,
  buildRenderTheme,
  type RenderTheme,
} from '../render.js';
import { visibleWidth } from '../../utils/text-width.js';
import stripAnsi from 'strip-ansi';

const entryContaining = (out: string[], needle: string) => {
  const e = out.find((x) => stripAnsi(x).includes(needle));
  expect(e).toBeDefined();
  return e!;
};

/**
 * The diff renderer manually wraps long source lines (instead of letting the
 * terminal soft-wrap) so continuation rows get a hanging indent aligned under
 * row 0's body. bg tint + syntax highlight survive wrap boundaries because we
 * wrap the already-styled output via wrapAnsiLine (preserves zero-width SGR).
 */
describe('renderUnifiedDiff — wrapping', () => {
  // Column geometry: head = '  ' + numStr(4) + ' ' (7) + 1-col gutter = 8; body
  // gets a 2-col inset, so continuation rows indent to col 10.
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
    // A highlight span opening on row 0 must re-emit on each continuation row.
    // A long contiguous comment is one cli-highlight span, so the wrap lands
    // mid-span — the strictest case.
    const longComment =
      '// This is an intentionally very long single-line comment that is designed to exceed the terminal width during testing of the diff renderer';
    const addedEntry = entryContaining(
      diffFor('', longComment),
      'intentionally'
    );
    const visualRows = addedEntry.split('\n');
    expect(visualRows.length).toBeGreaterThan(1);
    // The first non-bg SGR after the bg-open on row 0 is the body's highlight
    // color (earlier seqs are line#/gutter chrome, scoped to row 0).
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
 * Theme support: diffs route bg + bar colors through the active theme; no theme
 * falls back to the legacy hardcoded SGR. Tests use HAND-CRAFTED SGR strings
 * (not chalk.bgHex) so they're deterministic regardless of chalk.level.
 */
function makeThemeWith(diffSlots: {
  diffAddedBg: ((s: string) => string) | null;
  diffRemovedBg: ((s: string) => string) | null;
  diffAddedBar: (s: string) => string;
  diffRemovedBar: (s: string) => string;
  diffAddedFg?: (s: string) => string;
  diffRemovedFg?: (s: string) => string;
}): RenderTheme {
  const noop = (s: string) => s;
  return {
    brand: noop,
    primary: noop,
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
  // eslint-disable-next-line no-control-regex
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

  // No-theme → legacy fallback; theme-supplied slots must REPLACE (not stack)
  // the legacy SGR per direction. Distinctive RGBs so an assertion can't
  // accidentally match a shipped kiroDark/kiroLight value. `absentRaw` guards
  // the replaced legacy bg from also surfacing. themeOverrides=null → no theme;
  // {} → the default themeFor slots (added 10,200,50 bg / 255,0,255 bar).
  test.each<{
    name: string;
    old: string;
    next: string;
    needle: string;
    themeOverrides: Partial<Parameters<typeof makeThemeWith>[0]> | null;
    presentRaw: RegExp[];
    absentRaw: RegExp[];
  }>([
    {
      name: 'no theme → legacy SGR',
      old: '',
      next: 'short',
      needle: 'short',
      themeOverrides: null,
      presentRaw: [LEGACY_ADDED],
      absentRaw: [],
    },
    {
      name: 'theme bg+bar replace legacy on added rows',
      old: '',
      next: 'short',
      needle: 'short',
      themeOverrides: {},
      // eslint-disable-next-line no-control-regex
      presentRaw: [/\x1b\[48;2;10;200;50m/, /\x1b\[38;2;255;0;255m/],
      absentRaw: [LEGACY_ADDED],
    },
    {
      name: 'theme bg+bar replace legacy on removed rows',
      old: 'vanish',
      next: '',
      needle: 'vanish',
      themeOverrides: {
        diffRemovedBg: (s: string) => '\x1b[48;2;204;85;170m' + s + '\x1b[49m',
        diffRemovedBar: (s: string) => '\x1b[38;2;0;221;238m' + s + '\x1b[39m',
      },
      // eslint-disable-next-line no-control-regex
      presentRaw: [/\x1b\[48;2;204;85;170m/, /\x1b\[38;2;0;221;238m/],
      // eslint-disable-next-line no-control-regex
      absentRaw: [/\x1b\[48;2;45;31;34m/],
    },
  ])(
    '$name',
    ({ old, next, needle, themeOverrides, presentRaw, absentRaw }) => {
      const theme =
        themeOverrides === null ? undefined : themeFor(themeOverrides);
      const entry = entryContaining(
        renderUnifiedDiff(old, next, {
          path: 'src/foo.ts',
          termCols: 40,
          theme,
        }),
        needle
      );
      for (const re of presentRaw) expect(entry).toMatch(re);
      for (const re of absentRaw) expect(entry).not.toMatch(re);
    }
  );

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

/**
 * kiroSafe contract (P468472407): a theme with null bg slots explicitly opts
 * out of bg tints — the terminal background is unknown (SSH / failed
 * detection), so diff rows must render git-style: no bg SGR at all, whole
 * line painted with diffAddedFg/diffRemovedFg. Regression: this used to fall
 * back to hardcoded DARK tints, unreadable on light terminals.
 */
describe('renderUnifiedDiff — fg-only mode (theme opts out of bg tints)', () => {
  // eslint-disable-next-line no-control-regex
  const ANY_BG = /\x1b\[48[;m]/;
  const ADDED_FG_SGR = '\x1b[38;2;1;170;1m';
  const REMOVED_FG_SGR = '\x1b[38;2;170;1;1m';
  const safeTheme = makeThemeWith({
    diffAddedBg: null,
    diffRemovedBg: null,
    diffAddedBar: (s) => '\x1b[38;2;2;255;2m' + s + '\x1b[39m',
    diffRemovedBar: (s) => '\x1b[38;2;255;2;2m' + s + '\x1b[39m',
    diffAddedFg: (s) => ADDED_FG_SGR + s + '\x1b[39m',
    diffRemovedFg: (s) => REMOVED_FG_SGR + s + '\x1b[39m',
  });
  const diffOpts = { path: 'src/foo.ts', termCols: 40, theme: safeTheme };

  test('added rows: no bg SGR, body painted with diffAddedFg', () => {
    const entry = entryContaining(
      renderUnifiedDiff('', 'short', diffOpts),
      'short'
    );
    expect(entry).not.toMatch(ANY_BG);
    expect(entry).toContain(ADDED_FG_SGR);
  });

  test('removed rows: no bg SGR, body painted with diffRemovedFg', () => {
    const entry = entryContaining(
      renderUnifiedDiff('vanish', '', diffOpts),
      'vanish'
    );
    expect(entry).not.toMatch(ANY_BG);
    expect(entry).toContain(REMOVED_FG_SGR);
  });

  test('long lines wrap with fg re-applied per row and no right padding', () => {
    const longCode =
      'const reallyLongVariableNameHere = someFunctionCall(argument1, argument2);';
    const entry = entryContaining(
      renderUnifiedDiff('', longCode, diffOpts),
      'reallyLong'
    );
    const visualRows = entry.split('\n');
    expect(visualRows.length).toBeGreaterThan(1);
    for (const vr of visualRows) {
      expect(vr).toContain(ADDED_FG_SGR);
      expect(vr).not.toMatch(ANY_BG);
      // fg-only rows are not padded to the terminal edge (no bg to extend).
      expect(stripAnsi(vr)).not.toMatch(/ $/);
    }
  });

  test('missing fg slots fall back without reintroducing a bg tint', () => {
    const theme = makeThemeWith({
      diffAddedBg: null,
      diffRemovedBg: null,
      diffAddedBar: (s) => s,
      diffRemovedBar: (s) => s,
    });
    const entry = entryContaining(
      renderUnifiedDiff('', 'short', { ...diffOpts, theme }),
      'short'
    );
    expect(entry).not.toMatch(ANY_BG);
  });
});

/**
 * buildRenderTheme end-to-end: a getColor whose diff backgrounds resolve to
 * 'inherit' (kiroSafe's `background: 'default'`) must produce null bg slots
 * and pick up the theme's diff foregrounds — this is the exact wiring the
 * light-terminal bug broke (inherit used to hit the dark hardcoded fallback).
 */
describe('buildRenderTheme — inherit diff backgrounds (kiroSafe)', () => {
  const FG_ADDED = '\x1b[38;2;3;160;3m';
  const FG_REMOVED = '\x1b[38;2;160;3;3m';
  const mockGetColor = (path: string) => {
    if (
      path === 'diff.added.background' ||
      path === 'diff.removed.background'
    ) {
      const fn = (s: string) => s;
      (fn as any).hex = 'inherit';
      return fn;
    }
    if (path === 'diff.added.foreground')
      return (s: string) => FG_ADDED + s + '\x1b[39m';
    if (path === 'diff.removed.foreground')
      return (s: string) => FG_REMOVED + s + '\x1b[39m';
    return (s: string) => s;
  };

  test('maps inherit bg to null and renders fg-only diff rows', () => {
    const theme = buildRenderTheme(mockGetColor as any);
    expect(theme.diffAddedBg).toBeNull();
    expect(theme.diffRemovedBg).toBeNull();

    const entry = entryContaining(
      renderUnifiedDiff('', 'short', {
        path: 'src/foo.ts',
        termCols: 40,
        theme,
      }),
      'short'
    );
    // eslint-disable-next-line no-control-regex
    expect(entry).not.toMatch(/\x1b\[48[;m]/);
    expect(entry).toContain(FG_ADDED);
  });
});

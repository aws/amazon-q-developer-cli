import chalk from 'chalk';
import { UNICODE_GLYPHS, type Glyphs } from '../../utils/glyphs.js';

export function resolveGlyphs(g?: Glyphs): Glyphs {
  return g ?? UNICODE_GLYPHS;
}

// Must be frame-shaped (exception name leading a line, followed by `:`), not a
// bare prose mention — else an agent message explaining `AccessDeniedException`
// would be re-styled wholesale as a system error.
const ERROR_FRAME_RE =
  /(^|\n)\s*(?:ValidationException|ThrottlingException|ServiceException|AccessDeniedException|ResourceNotFoundException|InternalServerException)\s*:/;

export function isErrorContent(text: string): boolean {
  return ERROR_FRAME_RE.test(text);
}

// Default chalk wrappers for callers that don't thread a theme (tests, pure
// contexts); mirror the kiroDark base theme. When a theme IS available, callers
// pass RenderContext.theme and the renderer reads from there instead.
export const brand = chalk.hex('#C19AFF');
export const responseChip = chalk.hex('#FF8FB1');
export const DEFAULT_USER_TAG = chalk.bold.cyan;
// Tool-output body tint — soft sage-green reads as "successful result" without
// competing with neutral prose; errors stay loud red.
export const softSuccessOutput = chalk.hex('#a3c0a3');

// Per-render theme accessors, passed via RenderContext.theme so the renderer
// stays pure. Each field is a chalk-like `(s) => string`; every one falls back
// to a legacy hardcoded color when the theme is unavailable.
export interface RenderTheme {
  brand: (s: string) => string;
  responseChip: (s: string) => string;
  userTag: (s: string) => string;
  userBody: (s: string) => string;
  inlineCode: (s: string) => string;
  link: (s: string) => string;
  secondary: (s: string) => string;
  diffAddedBg: (s: string) => string;
  diffRemovedBg: (s: string) => string;
  diffAddedBar: (s: string) => string;
  diffRemovedBar: (s: string) => string;
}

// The diff bgHex values must match diff.ts's ADDED_BG_OPEN / REMOVED_BG_OPEN
// constants exactly (applyBg re-asserts those SGRs across cli-highlight resets).
const DEFAULT_RENDER_THEME: RenderTheme = {
  brand,
  responseChip,
  userTag: DEFAULT_USER_TAG,
  userBody: chalk.cyan,
  inlineCode: chalk.cyan,
  link: chalk.cyan,
  secondary: chalk.dim,
  diffAddedBg: chalk.bgHex('#1F2D22'),
  diffRemovedBg: chalk.bgHex('#2D1F22'),
  diffAddedBar: chalk.hex('#80ffb5'),
  diffRemovedBar: chalk.hex('#ff8080'),
};

export function resolveTheme(t?: RenderTheme): RenderTheme {
  return t ?? DEFAULT_RENDER_THEME;
}

// Each token falls back to a hardcoded color when the resolver throws — keeps
// lite render functional even if a custom theme misses a slot.
export function buildRenderTheme(
  getColor: (path: string) => any,
  getUserPromptColor?: () => any,
  getUserPromptBgHex?: () => string | undefined
): RenderTheme {
  const safeChalk = (path: string, fallback: (s: string) => string) => {
    try {
      const fn = getColor(path);
      // chalk chains are callable with (s) => string, but theme accessors
      // can return a chain that needs further calling. Test by invoking on
      // the empty string — if it doesn't return a string, fall back.
      const probe = fn('');
      if (typeof probe !== 'string') return fallback;
      return (s: string) => fn(s);
    } catch {
      return fallback;
    }
  };
  // getColor builds an FG-mode wrapper, so for diff bg tints we read its
  // resolved hex and rebuild as bg-mode chalk. The ansi256(N) sentinel (256-
  // color terminals) routes through bgAnsi256 to preserve the color-table
  // index — bgHex would double-convert through hex and lose precision.
  const safeBgChalk = (
    path: string,
    fallback: (s: string) => string
  ): ((s: string) => string) => {
    try {
      const fn = getColor(path);
      const hex = fn?.hex;
      if (typeof hex !== 'string' || hex === 'inherit') return fallback;
      const ansi256Match = /^ansi256\((\d+)\)$/.exec(hex);
      if (ansi256Match) {
        const idx = parseInt(ansi256Match[1]!, 10);
        const bg = chalk.bgAnsi256(idx);
        return (s: string) => bg(s);
      }
      const bg = chalk.bgHex(hex);
      return (s: string) => bg(s);
    } catch {
      return fallback;
    }
  };
  let userTagColorFn: (s: string) => string = chalk.cyan;
  if (getUserPromptColor) {
    try {
      const fn = getUserPromptColor();
      const probe = fn('');
      if (typeof probe === 'string') userTagColorFn = (s: string) => fn(s);
    } catch {
      // keep chalk.cyan fallback
    }
  }
  // userBody composes the prompt text color with the prompt bg hex (if any)
  // so a Purple preset paints white-on-violet across the whole message body
  // in scrollback — matching what standard mode does via `<Box backgroundColor>`.
  let userBodyFn: (s: string) => string = userTagColorFn;
  if (getUserPromptBgHex) {
    try {
      const bgHex = getUserPromptBgHex();
      if (bgHex && bgHex !== 'inherit') {
        const bg = chalk.bgHex(bgHex);
        userBodyFn = (s: string) => bg(userTagColorFn(s));
      }
    } catch {
      // keep fg-only fallback
    }
  }
  return {
    brand: safeChalk('brand', brand),
    // No dedicated "response chip" slot — accent is the closest, still shifts per theme.
    responseChip: safeChalk('accent', responseChip),
    userTag: (s: string) => chalk.bold(userTagColorFn(s)),
    userBody: userBodyFn,
    inlineCode: safeChalk('highlight', chalk.cyan),
    link: safeChalk('link', chalk.cyan),
    secondary: safeChalk('secondary', chalk.dim),
    // Bg slots use safeBgChalk (cli-highlight resets need real bg SGRs to
    // re-assert); bar slots are fg glyph colors.
    diffAddedBg: safeBgChalk('diff.added.background', chalk.bgHex('#1F2D22')),
    diffRemovedBg: safeBgChalk(
      'diff.removed.background',
      chalk.bgHex('#2D1F22')
    ),
    diffAddedBar: safeChalk('diff.added.bar', chalk.hex('#80ffb5')),
    diffRemovedBar: safeChalk('diff.removed.bar', chalk.hex('#ff8080')),
  };
}

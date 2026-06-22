import chalk from 'chalk';
import { UNICODE_GLYPHS, type Glyphs } from '../../utils/glyphs.js';

/**
 * Resolve the glyph set for this render. ASCII mode (env `KIRO_ASCII_MODE=1`
 * or `chat.allowAsciiArt=false`) flips every box-drawing / decorative char
 * in lite scrollback to its ASCII fallback. Callers thread the active set
 * through {@link RenderContext.glyphs} (set in LiteLayout from
 * `useGlyphs()`); pure-context callers (tests, sub-renderers without a
 * ctx) get UNICODE_GLYPHS so existing snapshot/assertion shape is preserved
 * without per-test wiring.
 */
export function resolveGlyphs(g?: Glyphs): Glyphs {
  return g ?? UNICODE_GLYPHS;
}

/** AWS-style exception name patterns. Matched only when followed by a
 *  colon-shaped error frame, never as bare prose mentions. The previous
 *  loose match re-styled an entire agent message as a system error any
 *  time it discussed `AccessDeniedException` / `ThrottlingException` /
 *  etc., which is a normal thing for the agent to do when explaining what
 *  could go wrong. We now require the exception name to lead a line and
 *  be followed by `:` plus authoritative-error context — i.e., the form
 *  AWS clients actually surface. */
const ERROR_FRAME_RE =
  /(^|\n)\s*(?:ValidationException|ThrottlingException|ServiceException|AccessDeniedException|ResourceNotFoundException|InternalServerException)\s*:/;

/** Detect AWS-style exceptions surfaced as model speech so we can re-render
 *  them as system errors. Only fires on actual error frames (`Name: ...`
 *  at the start of a line), not on prose that merely mentions an exception
 *  name in passing. */
export function isErrorContent(text: string): boolean {
  return ERROR_FRAME_RE.test(text);
}

/**
 * Default chalk wrappers used when no theme is provided to the renderer
 * (e.g. unit tests, storybook). Mirror the kiroDark base theme so behavior
 * doesn't visibly change for callers that haven't been wired up yet.
 *
 * When a theme IS available, callers pass {@link RenderContext.theme} and the
 * renderer reads `theme.brand` / `theme.responseChip` / `theme.userTag` from
 * there instead. This is how /theme bundled:dark|light actually changes the
 * colors of agent text, "You:" tag, and subagent response chips in lite mode.
 */
export const brand = chalk.hex('#C19AFF');
export const responseChip = chalk.hex('#FF8FB1');
export const DEFAULT_USER_TAG = chalk.bold.cyan;
/**
 * Tool-output body tint — soft sage-green that reads as "successful
 * result" without being a loud accent. Errors render in red (loud,
 * demand attention); success output in this subtle green keeps the
 * "result" semantic without annoying the eye. Sits at ~75% luminance
 * with low saturation so it differentiates from neutral white agent
 * prose / args values, but doesn't compete with them for attention.
 *
 * Bar glyph stays dim (neutral chrome) so the green is the load-bearing
 * color of the block; the `│` margin is still in the same family as
 * other dim structural glyphs (table borders, blockquote bars, etc.).
 */
export const softSuccessOutput = chalk.hex('#a3c0a3');

/**
 * Per-render theme accessors. Passed via {@link RenderContext.theme} so the
 * renderer stays pure (no React/store imports) while visuals follow the active
 * theme. Each field is a chalk-like `(s) => string` (fg unless noted as a bg
 * tint), and every one falls back to a legacy hardcoded color when the theme
 * is unavailable so pure-context callers / snapshots stay green.
 */
export interface RenderTheme {
  brand: (s: string) => string;
  responseChip: (s: string) => string;
  userTag: (s: string) => string;
  /** User message body: prompt text + (optional) bg, mirroring standard
   *  mode's `<Box backgroundColor>`. Skipped on the bare role tag. */
  userBody: (s: string) => string;
  /** Inline code span. The `seg.quote` flag that selects this is set on
   *  `codespan` tokens by the marked parser — historical naming, not a
   *  blockquote tie-in (modern TUI's MarkdownRenderer matches). */
  inlineCode: (s: string) => string;
  link: (s: string) => string;
  /** Dim `(url)` trailer after a link whose text differs from the URL. */
  secondary: (s: string) => string;
  diffAddedBg: (s: string) => string;
  diffRemovedBg: (s: string) => string;
  diffAddedBar: (s: string) => string;
  diffRemovedBar: (s: string) => string;
}

/**
 * Fallback theme for tests / pure-context callers that don't thread a theme.
 * Mirrors the prior hardcoded chalk colors so snapshots stay green. The diff
 * bgHex values must match diff.ts's ADDED_BG_OPEN / REMOVED_BG_OPEN constants
 * exactly (applyBg re-asserts those SGRs across cli-highlight resets).
 */
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

/**
 * Build a {@link RenderTheme} from a theme's `getColor` accessor and the
 * user's prompt-tag color. Cheap; LiteLayout calls this per render, but the
 * resulting object is stable as long as the inputs are. Each token falls
 * back to the previous hardcoded color when the resolver throws — keeps the
 * lite render functional even if a custom theme misses a slot.
 */
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
  // Bg-mode chalk lookup. The theme's `getColor` accessor builds an
  // FG-mode chalk wrapper by default (mode='fg' inside
  // `getTerminalChalkColor`), so `safeChalk('diff.added.background', …)`
  // would paint the BG color as the FOREGROUND — we need a real bg
  // wrapper for the diff body tint. Read the resolved hex off the
  // wrapper's `.hex` property and rebuild as a bg-mode chalk. The
  // 256-color sentinel `ansi256(N)` (returned for `has256 && !has16m`
  // terminals) gets routed through `chalk.bgAnsi256` to preserve the
  // original color-table index — going through `bgHex` would
  // double-convert through hex approximation and lose precision on
  // older terminals.
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

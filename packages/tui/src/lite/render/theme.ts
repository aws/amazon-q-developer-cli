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
 * Per-render theme accessors. Pass via {@link RenderContext.theme} so the
 * renderer stays pure (no React/store imports) but visuals follow the active
 * theme. Each field is a chalk-like `(s) => string` so the wrapper can apply
 * `.bold` / `.dim` chains itself.
 */
export interface RenderTheme {
  /** Agent name color (was hardcoded brand purple). Used for "Kiro:" /
   *  custom-agent-name role tags and for in-line tool reasoning text. */
  brand: (s: string) => string;
  /** Subagent response chip color (was hardcoded pink). Distinct from the
   *  per-agent input color so the eye separates input from output. */
  responseChip: (s: string) => string;
  /** "You:" tag color (was hardcoded bold cyan). Picks up the user's chosen
   *  prompt text color so light-mode swaps to a darker accent automatically. */
  userTag: (s: string) => string;
  /** Body wrapper for the user's submitted message in scrollback. Applies the
   *  prompt preset's text color and (when set) bg color, mirroring the
   *  `<Box backgroundColor>` highlight that standard mode paints in
   *  `Message.tsx`. Skipped on the bare role tag so the `You:` accent stays
   *  clean. */
  userBody: (s: string) => string;
  /** Inline code (`` `backtick` ``) span color. Maps to theme `highlight` slot
   *  in lite mode (kiroDark: `#0087FF`, kiroLight: `#005fff`). The misleading
   *  `seg.quote` flag in {@link MarkdownSegment} is set on `codespan` tokens
   *  by the marked-based parser — historical naming, not a blockquote tie-in.
   *  Modern TUI's `MarkdownRenderer` uses the same slot for the same flag.
   *  Falls back to `chalk.cyan` when the theme is unavailable, matching the
   *  prior hardcoded color. */
  inlineCode: (s: string) => string;
  /** Link label color. Maps to theme `link` slot (typically blue). Underline
   *  is applied separately by the renderer so links stay visually distinct
   *  on themes whose `link` color matches prose. Falls back to `chalk.cyan`. */
  link: (s: string) => string;
  /** Secondary text color, used for the dim `(url)` trailer that follows
   *  link labels when the visible text differs from the URL. Maps to theme
   *  `secondary` slot (typically grey). Falls back to `chalk.dim`. */
  secondary: (s: string) => string;
  /** Diff "added" line background tint. Maps to theme
   *  `diff.added.background` (kiroDark: `#2d3a30`, kiroLight: `#d4f0d4`).
   *  Used by {@link renderUnifiedDiff} to paint the soft tint behind
   *  added rows; the renderer extracts the leading SGR open from
   *  `wrapper('')` so it can re-assert the bg after every full reset
   *  cli-highlight emits between syntax tokens (see `applyBg` in
   *  `diff.ts`). Falls back to `chalk.bgHex('#1F2D22')` (legacy SGR
   *  `\x1b[48;2;31;45;34m`) when the theme is unavailable, matching
   *  the prior hardcoded diff palette. */
  diffAddedBg: (s: string) => string;
  /** Diff "removed" line background tint. Maps to theme
   *  `diff.removed.background` (kiroDark: `#3a2d2f`, kiroLight:
   *  `#f0d4d4`). Falls back to `chalk.bgHex('#2D1F22')` (legacy SGR
   *  `\x1b[48;2;45;31;34m`). */
  diffRemovedBg: (s: string) => string;
  /** Diff "added" gutter glyph color (the `+` left-of-line accent).
   *  Maps to theme `diff.added.bar` (kiroDark: `#80ffb5`, kiroLight:
   *  `#5de89d`). The renderer composes this wrapper with `chalk.bold`
   *  so the glyph stays readable on themes whose bar color is a light
   *  pastel. Falls back to `chalk.hex('#80ffb5')`. */
  diffAddedBar: (s: string) => string;
  /** Diff "removed" gutter glyph color (the `-` left-of-line accent).
   *  Maps to theme `diff.removed.bar` (kiroDark: `#ff8080`, kiroLight:
   *  `#eb5c5c`). Falls back to `chalk.hex('#ff8080')`. */
  diffRemovedBar: (s: string) => string;
}

/**
 * Fallback {@link RenderTheme} used by tests and pure-context callers that
 * don't thread a theme through. Mirrors the prior hardcoded chalk colors so
 * existing snapshots / ANSI-code assertions stay green when no theme is
 * supplied. Lite mode itself always passes a real theme via
 * {@link buildRenderTheme}; this is purely the "no theme available"
 * fallback shape.
 */
const DEFAULT_RENDER_THEME: RenderTheme = {
  brand,
  responseChip,
  userTag: DEFAULT_USER_TAG,
  userBody: chalk.cyan,
  inlineCode: chalk.cyan,
  link: chalk.cyan,
  secondary: chalk.dim,
  // Hardcoded diff fallbacks. The bgHex values produce the legacy SGR
  // open codes `\x1b[48;2;31;45;34m` / `\x1b[48;2;45;31;34m` that the
  // diff renderer's `applyBg` re-asserts after every cli-highlight reset
  // — match the prior `ADDED_BG_OPEN` / `REMOVED_BG_OPEN` constants in
  // `diff.ts` exactly so existing snapshot tests stay green when no
  // theme is supplied. The bar fg colors mirror the prior `ADDED_BAR` /
  // `REMOVED_BAR` hex values for the same reason.
  diffAddedBg: chalk.bgHex('#1F2D22'),
  diffRemovedBg: chalk.bgHex('#2D1F22'),
  diffAddedBar: chalk.hex('#80ffb5'),
  diffRemovedBar: chalk.hex('#ff8080'),
};

/**
 * Resolve a theme from an optional argument. Centralizes the
 * "no theme → DEFAULT_RENDER_THEME" decision so callers don't each have to
 * do `theme ?? DEFAULT_RENDER_THEME`.
 */
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
    // Theme doesn't carry a dedicated "response chip" slot — accent reads
    // the closest, and we still get a per-theme shift (kiroLight vs Dark).
    responseChip: safeChalk('accent', responseChip),
    userTag: (s: string) => chalk.bold(userTagColorFn(s)),
    userBody: userBodyFn,
    // Markdown body slots — see {@link RenderTheme} for what each maps to.
    // `highlight` is the theme's "callout / pop" color, used for inline
    // code; `link` is the link slot (blue in kiroDark/Light); `secondary`
    // is the muted text slot used for the `(url)` trailer after links.
    // Each falls back to its prior hardcoded color when the theme accessor
    // throws, keeping pure-context callers unaffected.
    inlineCode: safeChalk('highlight', chalk.cyan),
    link: safeChalk('link', chalk.cyan),
    secondary: safeChalk('secondary', chalk.dim),
    // Diff body slots — see {@link RenderTheme} for what each maps to.
    // Bg slots use `safeBgChalk` so the wrapper paints the background
    // (cli-highlight's resets need real bg SGRs to re-assert across
    // syntax tokens). Bar slots are FG colors used for the +/- glyphs;
    // `safeChalk` is fine. Each falls back to its prior hardcoded color
    // when the theme accessor throws or returns a non-string, keeping
    // pure-context callers and snapshot tests unaffected.
    diffAddedBg: safeBgChalk('diff.added.background', chalk.bgHex('#1F2D22')),
    diffRemovedBg: safeBgChalk(
      'diff.removed.background',
      chalk.bgHex('#2D1F22')
    ),
    diffAddedBar: safeChalk('diff.added.bar', chalk.hex('#80ffb5')),
    diffRemovedBar: safeChalk('diff.removed.bar', chalk.hex('#ff8080')),
  };
}

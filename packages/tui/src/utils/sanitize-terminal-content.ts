/**
 * Neutralize the terminal control sequences in UNTRUSTED display text (tool
 * output, shell stdout/stderr, tool error strings) that can outlive the text
 * they appear in — an unclosed OSC 8 hyperlink turns every later line into a
 * link, an unclosed SGR colors the rest of the screen — while leaving the text
 * itself, and legitimate color, alone.
 *
 * SGR (`ESC [ ... m`) is the only escape family kept: it can set text
 * attributes but cannot move the cursor, clear the screen, switch charsets, or
 * touch the clipboard. Every other escape — cursor/erase/scroll CSI, DEC
 * private modes, OSC (incl. OSC 52 clipboard writes and OSC 8 links),
 * DCS/SOS/PM/APC strings, charset selection (`ESC ( 0`), full reset (`ESC c`),
 * and stray ESC — is a corruption or injection vector in inline content and is
 * removed. Only the 7-bit form of SGR is kept; the 8-bit C1 equivalent is
 * removed with the rest.
 *
 * To stop an unterminated attribute from bleeding into later lines/frames, any
 * line that opens an SGR is terminated with a reset. State is therefore bounded
 * to the single line it appears on.
 *
 * Bytes that only affect the text's own layout — tab, newline, and carriage
 * return — are left as-is, so progress redraws and line endings render exactly
 * as they did before this text was sanitized.
 */
import {
  buildEscapeRegExp,
  buildSgrRegExp,
  buildSgrScanRegExp,
  buildStrippableControlsRegExp,
} from './terminal-escape-grammar.js';

// A callback decides per match whether to keep it (SGR) or drop it. C0 controls
// are handled in a second pass that excludes ESC so it cannot corrupt a kept
// SGR sequence.
const ANY_ESCAPE_RE = buildEscapeRegExp();

// Anything with intermediate bytes or a non-numeric parameter is not SGR, so a
// whole-match test is what decides whether a sequence survives.
const SGR_RE = buildSgrRegExp();

const STRIPPABLE_CONTROLS_RE = buildStrippableControlsRegExp();

const LINE_SGR_RE = buildSgrScanRegExp();

// Whether the line ends with an SGR attribute still active — i.e. its last
// effective SGR is not a full reset (`ESC[0m` or `ESC[m`). Compound sequences
// ending in a reset are treated conservatively as still-open, which only costs
// a harmless redundant trailing reset.
const lineLeavesSgrOpen = (line: string): boolean => {
  let open = false;
  for (const match of line.matchAll(LINE_SGR_RE)) {
    const params = match[1];
    open = !(params === '' || params === '0');
  }
  return open;
};

/**
 * Sanitize a single string of untrusted content. Keeps printable text, tabs,
 * newlines, carriage returns, and SGR; removes every other escape and control
 * byte; and bounds SGR state to each line so it cannot bleed.
 */
export function sanitizeUntrustedText(text: string): string {
  if (text === '') return '';

  const withoutHostileEscapes = text.replace(ANY_ESCAPE_RE, (seq) =>
    SGR_RE.test(seq) ? seq : ''
  );
  const withoutControls = withoutHostileEscapes.replace(
    STRIPPABLE_CONTROLS_RE,
    ''
  );

  if (!withoutControls.includes('\x1b')) return withoutControls;

  return withoutControls
    .split('\n')
    .map((line) => (lineLeavesSgrOpen(line) ? `${line}\x1b[0m` : line))
    .join('\n');
}

/**
 * Sanitize the error string of a tool result in place-safe fashion: an error
 * result gets its (untrusted) error text cleaned; success/cancelled results are
 * returned unchanged. Success `output` is left to the extractor/render-layer
 * sanitizers, which see it as displayable text rather than a structured value.
 */
export function sanitizeToolResultError<
  T extends { status: string; error?: string },
>(result: T | undefined): T | undefined {
  if (result && result.status === 'error' && typeof result.error === 'string') {
    return { ...result, error: sanitizeUntrustedText(result.error) };
  }
  return result;
}

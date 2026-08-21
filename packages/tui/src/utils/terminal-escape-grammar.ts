/**
 * Grammar for terminal escape sequences, shared by every sanitizer that
 * enforces it. Kept in one place because a gap in the grammar is a gap in all
 * of them, and two copies drift.
 *
 * Callers decide what to do with a match: strip it, or inspect it and keep the
 * subset they consider safe.
 */

// Bytes a string sequence (OSC/DCS/SOS/PM/APC) may carry: printable only.
// Excluding C0 keeps an unterminated sequence from running past the line it
// started on and swallowing the rest of the input; excluding C1 keeps it from
// running past an 8-bit terminator.
const STRING_PAYLOAD = '[^\\x00-\\x1f\\x7f\\x80-\\x9f]*';

// BEL, 7-bit ST (ESC \), or 8-bit ST (0x9c). A well-formed sequence may use
// any of them, so all three must end a match.
const STRING_TERMINATOR = '(?:\\x07|\\x1b\\\\|\\x9c)';

const CSI_BODY = '[0-?]*[ -/]*';

// The ESC introducer, as a pattern source rather than a literal control byte.
const ESC = '\\x1b';

/**
 * Alternatives matching one escape sequence each, in priority order — a
 * truncated form must be tried before the stray-introducer catch-alls, or the
 * catch-all wins and leaves the remaining bytes as literal text.
 */
export const ESCAPE_ALTERNATIVES: readonly string[] = [
  // CSI: ESC [ params intermediates final (includes SGR, final `m`)
  `\\x1b\\[${CSI_BODY}[@-~]`,
  // OSC: ESC ] ... BEL/ST
  `\\x1b\\]${STRING_PAYLOAD}${STRING_TERMINATOR}?`,
  // DCS / SOS / PM / APC: ESC P|X|^|_ ... ST
  `\\x1b[PX^_]${STRING_PAYLOAD}${STRING_TERMINATOR}?`,
  // 8-bit C1 forms of the same families. A terminal acts on these identically,
  // so a complete sequence is matched the same way as its 7-bit spelling.
  `\\x9b${CSI_BODY}[@-~]`,
  `\\x9d${STRING_PAYLOAD}${STRING_TERMINATOR}?`,
  `[\\x90\\x98\\x9e\\x9f]${STRING_PAYLOAD}${STRING_TERMINATOR}?`,
  // A CSI with no final byte, in either spelling — end of input, or a byte that
  // cannot continue the sequence. Dropping it matters most for the 8-bit form:
  // a surviving `\x9b` is a live introducer, so the bytes that complete it —
  // in the next streamed chunk, or after a newline, neither of which aborts
  // collection at the terminal — would reassemble into a real command.
  `${ESC}\\[${CSI_BODY}(?![@-~])`,
  `\\x9b${CSI_BODY}(?![@-~])`,
  // Two-byte sequences (ESC c, ESC 7, ESC ( B, ...) and any stray ESC
  '\\x1b[ -/]?[0-~]?',
];

/** A fresh global RegExp over the grammar (callers own the lastIndex state). */
export function buildEscapeRegExp(): RegExp {
  return new RegExp(ESCAPE_ALTERNATIVES.join('|'), 'g');
}

/**
 * `ESC [ params m` restricted to SGR parameters (digits, `;`, and the `:`
 * sub-parameter separator), anchored so callers can test a whole match.
 */
export function buildSgrRegExp(): RegExp {
  return new RegExp(['^', ESC, '\\[[0-9;:]*m$'].join(''));
}

/** Global form of the SGR pattern, capturing the parameters. */
export function buildSgrScanRegExp(): RegExp {
  return new RegExp([ESC, '\\[([0-9;:]*)m'].join(''), 'g');
}

/**
 * C0 controls and DEL, excluding tab, newline, carriage return, and ESC. The
 * first three lay out the text rather than reconfigure the terminal; ESC is
 * left to the escape grammar so this cannot break apart a kept sequence.
 */
export function buildStrippableControlsRegExp(): RegExp {
  const c0 = '\\x00-\\x08\\x0b\\x0c\\x0e-\\x1a\\x1c-\\x1f\\x7f';
  // The C1 bytes that are control functions on their own — IND, NEL, HTS, RI,
  // SS2, SS3, ST — whose 7-bit spellings (ESC D/E/H/M/N/O, ESC \\) are already
  // stripped. Some outlive their text: HTS sets a tab stop, RI moves the cursor
  // up a line. The rest of the block is left alone so text that is already
  // mis-decoded is not mangled further.
  const c1ControlFunctions = '\\x84\\x85\\x88\\x8d\\x8e\\x8f\\x9c';
  return new RegExp(['[', c0, c1ControlFunctions, ']'].join(''), 'g');
}

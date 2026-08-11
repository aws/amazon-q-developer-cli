/**
 * Strip terminal escape sequences and C0 controls from untrusted display
 * text. The renderer's own sanitizer removes dangerous CSI only; transcript
 * bytes can also carry OSC (incl. OSC 52 clipboard writes), DCS/SOS/PM/APC
 * strings, and bare ESC finals like `ESC c` (full reset), all of which must
 * never reach the terminal from session titles or search snippets.
 */
const TERMINAL_ESCAPES_RE = new RegExp(
  [
    // CSI: ESC [ params intermediates final
    '\\x1b\\[[0-?]*[ -/]*[@-~]',
    // OSC: ESC ] ... terminated by BEL or ST (or end of string)
    '\\x1b\\][^\\x07\\x1b]*(?:\\x07|\\x1b\\\\)?',
    // DCS / SOS / PM / APC strings, terminated by ST (or end of string)
    '\\x1b[PX^_][^\\x1b]*(?:\\x1b\\\\)?',
    // Two-byte sequences (ESC c, ESC 7, ESC (B, ...) and any stray ESC
    '\\x1b[ -/]?[0-~]?',
  ].join('|'),
  'g'
);

// eslint-disable-next-line no-control-regex
const C0_CONTROLS_RE = /[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g;

export function stripTerminalEscapes(text: string): string {
  return text.replace(TERMINAL_ESCAPES_RE, '').replace(C0_CONTROLS_RE, '');
}

export function sanitizeSessionTitleForDisplay(title: unknown): string {
  if (typeof title !== 'string' || title === '') return '';
  return stripTerminalEscapes(title).replace(/\r\n?|\n/g, '\\n');
}

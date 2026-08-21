/**
 * Strip terminal escape sequences and C0 controls from untrusted display
 * text. Transcript bytes can carry OSC (incl. OSC 52 clipboard writes),
 * DCS/SOS/PM/APC strings, and bare ESC finals like `ESC c` (full reset), none
 * of which may reach the terminal from session titles or search snippets.
 *
 * Unlike the styled-output path, nothing is preserved here: this text is
 * non-styled metadata, so SGR is dropped along with everything else.
 */
import { buildEscapeRegExp } from './terminal-escape-grammar.js';

const TERMINAL_ESCAPES_RE = buildEscapeRegExp();

// LINT-DEBT(no-control-regex): pre-existing suppression accepted at gate adoption; matching C0 control characters is the purpose of this sanitizer regex
// eslint-disable-next-line no-control-regex
const C0_CONTROLS_RE = /[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g;

export function stripTerminalEscapes(text: string): string {
  return text.replace(TERMINAL_ESCAPES_RE, '').replace(C0_CONTROLS_RE, '');
}

export function sanitizeSessionTitleForDisplay(title: unknown): string {
  if (typeof title !== 'string' || title === '') return '';
  return stripTerminalEscapes(title).replace(/\r\n?|\n/g, '\\n');
}

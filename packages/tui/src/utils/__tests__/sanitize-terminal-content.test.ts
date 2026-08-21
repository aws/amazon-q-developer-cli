import { describe, it, expect } from 'bun:test';
import {
  sanitizeUntrustedText,
  sanitizeToolResultError,
} from '../sanitize-terminal-content';

const ESC = '\x1b';

describe('sanitizeUntrustedText', () => {
  it('passes through plain text unchanged', () => {
    expect(sanitizeUntrustedText('hello world')).toBe('hello world');
  });

  it('returns empty string unchanged', () => {
    expect(sanitizeUntrustedText('')).toBe('');
  });

  it('keeps SGR color sequences', () => {
    const input = `${ESC}[31mred${ESC}[0m`;
    expect(sanitizeUntrustedText(input)).toBe(`${ESC}[31mred${ESC}[0m`);
  });

  it('keeps truecolor and sub-parameter SGR (colons)', () => {
    const input = `${ESC}[38;2;255;0;0mrgb${ESC}[0m ${ESC}[4:3mcurly${ESC}[0m`;
    // Both lines/segments retain their SGR; the whole thing is one line so a
    // single trailing reset is appended (idempotent alongside the explicit one).
    expect(sanitizeUntrustedText(input)).toContain(`${ESC}[38;2;255;0;0m`);
    expect(sanitizeUntrustedText(input)).toContain(`${ESC}[4:3m`);
  });

  it('contains an underline bleed by appending a reset to the line', () => {
    // Unterminated underline — the defect that leaked into the parent shell.
    const input = `${ESC}[4munderlined`;
    expect(sanitizeUntrustedText(input)).toBe(`${ESC}[4munderlined${ESC}[0m`);
  });

  it('bounds SGR per line so it cannot bleed to the next line', () => {
    const input = `${ESC}[4mfirst\nsecond`;
    expect(sanitizeUntrustedText(input)).toBe(
      `${ESC}[4mfirst${ESC}[0m\nsecond`
    );
  });

  it('does not append a reset to lines without SGR', () => {
    expect(sanitizeUntrustedText('a\nb\nc')).toBe('a\nb\nc');
  });

  it('strips non-SGR CSI (cursor move, erase)', () => {
    expect(sanitizeUntrustedText(`${ESC}[2Jcleared`)).toBe('cleared');
    expect(sanitizeUntrustedText(`${ESC}[10;5Hmoved`)).toBe('moved');
    expect(sanitizeUntrustedText(`up${ESC}[3Adown`)).toBe('updown');
  });

  it('strips DEC private mode toggles', () => {
    expect(sanitizeUntrustedText(`${ESC}[?25ltext`)).toBe('text');
    expect(sanitizeUntrustedText(`${ESC}[?1049htext`)).toBe('text');
  });

  it('strips OSC sequences including OSC 52 clipboard writes', () => {
    expect(sanitizeUntrustedText(`${ESC}]0;title${ESC}\\text`)).toBe('text');
    expect(sanitizeUntrustedText(`${ESC}]52;c;ZXZpbA==\x07text`)).toBe('text');
  });

  it('strips OSC 8 hyperlinks', () => {
    const input = `${ESC}]8;;https://evil.example${ESC}\\click${ESC}]8;;${ESC}\\`;
    expect(sanitizeUntrustedText(input)).toBe('click');
  });

  it('strips DCS / PM / APC strings', () => {
    expect(sanitizeUntrustedText(`${ESC}Ppayload${ESC}\\ok`)).toBe('ok');
    expect(sanitizeUntrustedText(`${ESC}_apc${ESC}\\ok`)).toBe('ok');
  });

  it('strips ESC c full reset (RIS)', () => {
    expect(sanitizeUntrustedText(`${ESC}creset`)).toBe('reset');
  });

  it('strips charset selection (ESC ( 0 line-drawing switch)', () => {
    // The classic "everything renders as box-drawing/dotted glyphs" vector.
    expect(sanitizeUntrustedText(`${ESC}(0abc${ESC}(B`)).toBe('abc');
  });

  it('strips a bare/stray ESC and the byte it would consume', () => {
    // A terminal treats the byte after ESC as part of the sequence, so it
    // never displays; the sanitizer matches that by consuming both.
    expect(sanitizeUntrustedText(`a${ESC}b`)).toBe('a');
  });

  it('strips C0 control chars but keeps tab and newline', () => {
    expect(sanitizeUntrustedText('a\x07b')).toBe('ab'); // BEL
    expect(sanitizeUntrustedText('a\x08b')).toBe('ab'); // BS
    expect(sanitizeUntrustedText('a\tb\nc')).toBe('a\tb\nc'); // tab + newline kept
  });

  it('leaves carriage returns for downstream line handling', () => {
    // CR lays out the text rather than reconfiguring the terminal, so the
    // renderers keep the behavior they had before sanitization existed.
    expect(sanitizeUntrustedText('abc\rXYZ')).toBe('abc\rXYZ');
    expect(sanitizeUntrustedText('a\r\nb')).toBe('a\r\nb');
  });

  it('does not corrupt the ESC of a kept SGR via the C0 pass', () => {
    // Regression guard: ESC (0x1b) is a C0 byte; the control pass must not
    // strip it out of a surviving SGR sequence.
    const out = sanitizeUntrustedText(`${ESC}[32mok${ESC}[0m`);
    expect(out.startsWith(`${ESC}[32m`)).toBe(true);
  });

  it('handles mixed hostile + benign content', () => {
    const input = `${ESC}[2J${ESC}[31mred${ESC}[0m\x07\n${ESC}]52;c;x\x07plain`;
    expect(sanitizeUntrustedText(input)).toBe(`${ESC}[31mred${ESC}[0m\nplain`);
  });
});

describe('sanitizeToolResultError', () => {
  it('sanitizes the error string of an error result', () => {
    const out = sanitizeToolResultError({
      status: 'error',
      error: `${ESC}[4mboom${ESC}[2J`,
    });
    expect(out).toEqual({ status: 'error', error: `${ESC}[4mboom${ESC}[0m` });
  });

  it('leaves success results untouched (identity)', () => {
    const r = { status: 'success', output: 'ok' };
    expect(sanitizeToolResultError(r)).toBe(r);
  });

  it('leaves cancelled results untouched (identity)', () => {
    const r = { status: 'cancelled' };
    expect(sanitizeToolResultError(r)).toBe(r);
  });

  it('passes through undefined', () => {
    expect(sanitizeToolResultError(undefined)).toBeUndefined();
  });
});

describe('unterminated string sequences stay bounded', () => {
  it('drops only the introducer line when OSC has no terminator', () => {
    const input = `${ESC}]0;titleline1\nline2\nline3`;
    expect(sanitizeUntrustedText(input)).toBe('\nline2\nline3');
  });

  it('drops only the introducer line when DCS has no terminator', () => {
    const input = `${ESC}Pqline1\nline2`;
    expect(sanitizeUntrustedText(input)).toBe('\nline2');
  });

  it('accepts 8-bit ST as a terminator instead of swallowing the rest', () => {
    const input = `${ESC}]0;t\x9cline1\nline2`;
    expect(sanitizeUntrustedText(input)).toBe('line1\nline2');
  });

  it('drops a CSI clipped by end of input rather than leaking its params', () => {
    expect(sanitizeUntrustedText(`ok${ESC}[31`)).toBe('ok');
  });
});

describe('8-bit C1 control forms', () => {
  it('removes an 8-bit CSI erase', () => {
    expect(sanitizeUntrustedText('a\x9b2Jb')).toBe('ab');
  });

  it('drops a clipped 8-bit CSI, introducer and params', () => {
    // A surviving \x9b is a live introducer: the bytes completing it would
    // reassemble into a real command at the terminal.
    expect(sanitizeUntrustedText('ok\x9b31')).toBe('ok');
    expect(sanitizeUntrustedText('ok\x9b')).toBe('ok');
  });

  it('drops an 8-bit introducer that a newline does not abort', () => {
    // A C0 byte does not end sequence collection at the terminal, so the
    // introducer must go even when the next byte cannot continue it.
    expect(sanitizeUntrustedText('\x9b2\nJunk')).toBe('\nJunk');
    expect(sanitizeUntrustedText('\x1b[2\nX')).toBe('\nX');
  });

  it('cannot reassemble an erase-screen across two sanitize calls', () => {
    // The streaming chokepoint sanitizes each flush in isolation, so a split
    // sequence must not survive in a form the terminal can complete.
    expect(sanitizeUntrustedText('\x9b2') + sanitizeUntrustedText('J')).toBe(
      'J'
    );
  });

  it('drops lone C1 control functions whose 7-bit spellings are stripped', () => {
    expect(sanitizeUntrustedText('a\x85b')).toBe('ab'); // NEL
    expect(sanitizeUntrustedText('a\x8db')).toBe('ab'); // RI, moves cursor up
    expect(sanitizeUntrustedText('a\x88b')).toBe('ab'); // HTS, sets a tab stop
  });

  it('leaves the rest of the C1 block alone', () => {
    // Stripping all of \x80-\x9f would mangle text that is already
    // mis-decoded, for no gain against the sequences this module targets.
    expect(sanitizeUntrustedText('a\x91b')).toBe('a\x91b');
  });

  it('removes an 8-bit OSC clipboard write', () => {
    expect(sanitizeUntrustedText('a\x9d52;c;x\x9cb')).toBe('ab');
  });

  it('removes an 8-bit DCS string', () => {
    expect(sanitizeUntrustedText('a\x90pq\x9cb')).toBe('ab');
  });
});

describe('carriage return is left to the renderers', () => {
  it('preserves a CR progress sequence verbatim', () => {
    const progress =
      'Resolving deltas:  33% (1/3)\rResolving deltas: 100% (3/3), done.\n';
    expect(sanitizeUntrustedText(progress)).toBe(progress);
  });

  it('preserves CRLF', () => {
    expect(sanitizeUntrustedText('one\r\ntwo')).toBe('one\r\ntwo');
  });
});

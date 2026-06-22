import './setup-chalk-level.js';

import { describe, test, expect } from 'vitest';
import chalk from 'chalk';
import {
  renderUserMessage,
  renderAgentMessage,
  renderSystemError,
  renderSystemInfo,
  renderShellOutputBlock,
  renderMessageToText,
  buildRenderTheme,
  type RenderTheme,
} from '../render.js';
import stripAnsi from 'strip-ansi';
import { useTempKiroHome } from './temp-kiro-home.js';

useTempKiroHome();

describe('renderUserMessage', () => {
  test('single line with You: tag', () => {
    const stripped = stripAnsi(renderUserMessage('hello'));
    expect(stripped).toBe('You: hello');
  });

  test('multi-line continuation rows have no leading indent', () => {
    // Continuation rows are unprefixed so copy/paste yields the original
    // text. The role tag only marks the first line.
    const stripped = stripAnsi(renderUserMessage('line1\nline2'));
    expect(stripped).toBe('You: line1\nline2');
  });
});

describe('renderAgentMessage', () => {
  test('renders plain text', () => {
    const result = renderAgentMessage('Hello world');
    expect(result).toContain('Hello world');
  });

  test('renders bold', () => {
    const result = renderAgentMessage('**bold text**');
    expect(result).toContain('bold text');
  });

  test('renders code blocks with language', () => {
    const result = renderAgentMessage('```typescript\nconst x = 1;\n```');
    // cli-highlight wraps each syntax token in its own ANSI sequence
    // (`\x1B[34mconst\x1B[39m x = \x1B[32m1\x1B[39m;`), so a literal
    // toContain('const x = 1;') misses the highlighted version even when
    // the code is rendered correctly. Strip ANSI before checking the
    // logical content — the rest of the file uses the same pattern.
    const stripped = stripAnsi(result);
    expect(stripped).toContain('typescript');
    expect(stripped).toContain('const x = 1;');
  });

  // Regression: cli-highlight emits `\x1b[31m"…"\x1b[39m` for shell string
  // literals (red on, content, color reset). When the trailing reset lands
  // at the very end of a wrapped line, the ANSI-aware wrapper used to drop
  // it because it only attached escapes to the NEXT cell — and there was
  // no next cell. The closing fence's `\x1b[22m` only resets dim, not color,
  // so the unclosed red leaked into the next paragraph (visible in lite as
  // the prose after a `bash` block rendering entirely red). The fix flushes
  // any trailing ANSI onto the last wrapped row; this test pins the rule by
  // requiring a foreground reset (`[39m` or full `[0m`) somewhere after the
  // red opener, before the prose that follows the block.
  test('code block with trailing ANSI reset does not bleed into next paragraph', () => {
    const md =
      '```bash\nrm -rf "/Applications/Kiro CLI.app.bak"\n```\n\nWant me to run it?';
    const out = renderAgentMessage(md, 'Kiro', undefined, 120);
    const redOpenIdx = out.indexOf('\x1b[31m');
    if (redOpenIdx === -1) {
      // Bun currently runs cli-highlight in a plain-output mode. The
      // reset-preservation assertion below still runs in environments where
      // the highlighter emits ANSI.
      expect(out).toContain('Want me to run it?');
      return;
    }
    // Reset (either bare-fg `[39m` or full `[0m`) must appear AFTER the red
    // opener and BEFORE the trailing prose. Without the fix, no reset
    // existed between them and the prose inherited red on real terminals.
    const proseIdx = out.indexOf('Want me to run it?');
    expect(proseIdx).toBeGreaterThan(redOpenIdx);
    const between = out.slice(redOpenIdx, proseIdx);
    const esc = String.fromCharCode(27);
    expect(between).toMatch(new RegExp(`${esc}\\[(?:39|0)m`));
  });

  // Regression: when wrap lands at a whitespace cell, `wrapAnsiLine` used
  // to advance past the cell on the next row but DROP its `ansi` field on
  // the floor. Closers attached to that space — `\x1b[22m` after a
  // `**bold**` span, `\x1b[24m\x1b[39m\x1b[2m` between a chalk.underline.cyan
  // link label and its dim URL trailer, `\x1b[23m` after `*italic*`, etc.
  // — were silently lost. The row finished without closing the style and
  // terminal state stayed bold/italic/underline/color forever, leaking
  // into the rest of the message AND every subsequent message until
  // something else reset terminal state.
  //
  // Each test below picks a width that forces wrap exactly at the
  // closer-bearing space, then asserts the residual ANSI state at the
  // end of the rendered message is clean. `ansiStateAtEnd` walks every
  // escape through a tiny state machine — same model the real terminal
  // uses — and surfaces which attribute is leaking, which is the
  // information you need to diagnose a regression here.
  describe('wrap-boundary closer preservation', () => {
    function ansiStateAtEnd(s: string): {
      bold: boolean;
      dim: boolean;
      italic: boolean;
      underline: boolean;
      strike: boolean;
      color: boolean;
      bg: boolean;
    } {
      const state = {
        bold: false,
        dim: false,
        italic: false,
        underline: false,
        strike: false,
        color: false,
        bg: false,
      };
      // eslint-disable-next-line no-control-regex
      const re = /\x1b\[([0-9;]*)m/g;
      let m: RegExpExecArray | null;
      while ((m = re.exec(s)) !== null) {
        const codes = m[1] === '' ? ['0'] : m[1]!.split(';');
        for (let i = 0; i < codes.length; i++) {
          const n = parseInt(codes[i]!, 10);
          if (n === 0) {
            state.bold = false;
            state.dim = false;
            state.italic = false;
            state.underline = false;
            state.strike = false;
            state.color = false;
            state.bg = false;
          } else if (n === 1) state.bold = true;
          else if (n === 2) state.dim = true;
          else if (n === 3) state.italic = true;
          else if (n === 4) state.underline = true;
          else if (n === 9) state.strike = true;
          else if (n === 22) {
            // [22] resets BOTH bold and dim — it means "neither bold nor
            // dim", not "close bold". This is why a code block fence's
            // closing `chalk.dim('```')` doesn't fix a leaked color (and
            // why the prior trailing-ansi fix was needed for that case).
            state.bold = false;
            state.dim = false;
          } else if (n === 23) state.italic = false;
          else if (n === 24) state.underline = false;
          else if (n === 29) state.strike = false;
          else if ((n >= 30 && n <= 37) || (n >= 90 && n <= 97))
            state.color = true;
          else if (n === 38) {
            state.color = true;
            // 38;5;N (256-color) or 38;2;R;G;B (truecolor) — skip the args
            // so they don't get reinterpreted as standalone codes.
            const next = parseInt(codes[i + 1]!, 10);
            if (next === 5) i += 2;
            else if (next === 2) i += 4;
          } else if (n === 39) state.color = false;
          else if ((n >= 40 && n <= 47) || (n >= 100 && n <= 107))
            state.bg = true;
          else if (n === 48) {
            state.bg = true;
            const next = parseInt(codes[i + 1]!, 10);
            if (next === 5) i += 2;
            else if (next === 2) i += 4;
          } else if (n === 49) state.bg = false;
        }
      }
      return state;
    }

    // Each row picks a 30-col width that forces wrap exactly at the
    // closer-bearing space, then asserts the named attributes are OFF at the
    // tail. The list-item link case covers the renderListItem→inline path so
    // a future fix of the paragraph path can't silently regress it.
    test.each([
      [
        'long markdown link',
        'see [click here](https://example.com/path)',
        ['underline', 'color'],
      ],
      ['bold span', 'xxxxxxxxxxxxxx **bold** more text', ['bold']],
      ['italic span', 'xxxxxxxxxxxx *italic* more text', ['italic']],
      ['inline code span', 'xxxxxxxxxxxxxx `code` more text', ['color']],
      [
        'list item with link',
        '- see [click here](https://example.com/path)',
        ['underline', 'color'],
      ],
    ] as const)('%s wrap does not leak %j', (_name, input, cleared) => {
      const state = ansiStateAtEnd(
        renderAgentMessage(input, 'Kiro', undefined, 30)
      );
      for (const attr of cleared) expect(state[attr]).toBe(false);
    });
  });

  test('renders list items with bullets', () => {
    const result = renderAgentMessage('- item one\n- item two');
    expect(result).toContain('- item one');
    expect(result).toContain('- item two');
  });

  test('empty content returns empty string', () => {
    expect(renderAgentMessage('')).toBe('');
    expect(renderAgentMessage('   ')).toBe('');
  });

  test('renders headings with bold and a blank line of separation', () => {
    const out = stripAnsi(
      renderAgentMessage('# Title\n\nBody paragraph.', 'Kiro')
    );
    // Heading sits on the role-tag line, then a blank, then the body.
    const lines = out.split('\n');
    expect(lines[0]).toBe('Kiro: Title');
    expect(lines[1]).toBe('');
    expect(lines[2]).toBe('Body paragraph.');
  });

  test('renders ordered lists with their numbers preserved', () => {
    const out = stripAnsi(renderAgentMessage('1. first\n2. second'));
    expect(out).toContain('1. first');
    expect(out).toContain('2. second');
  });

  test('renders tables with box-drawing borders', () => {
    const md = '| h1 | h2 |\n|----|----|\n| a  | b  |';
    const out = stripAnsi(renderAgentMessage(md, 'Kiro', undefined, 80));
    // Header content + border chrome at the corners.
    expect(out).toContain('h1');
    expect(out).toContain('h2');
    expect(out).toContain('┌');
    expect(out).toContain('┘');
  });

  test('renders inline bold inside a sentence', () => {
    const result = renderAgentMessage('This is **important** text.');
    // ANSI bold sequence around the inner span.
    expect(result).toContain('\x1b[1m');
    expect(result).toContain('important');
    expect(stripAnsi(result)).toContain('This is important text.');
  });

  test('paragraphs render as one logical line so the terminal can soft-wrap', () => {
    // Lite stopped manually wrapping paragraphs at termCols so URLs / long
    // prose copy out of the terminal as ONE logical line instead of N
    // hard-newline-separated chunks. The Static <Text> in LiteLayout uses
    // wrap="overflow" so twinki passes the long line through verbatim and
    // the terminal handles the visual wrap. The previous behavior wrapped
    // here at every termCols boundary and broke copy-paste of any link
    // that spanned the terminal edge — see thoughts/lite-copy-paste-newlines-research.md.
    const long =
      'one two three four five six seven eight nine ten eleven twelve';
    const out = stripAnsi(renderAgentMessage(long, 'Kiro', undefined, 30));
    const lines = out.split('\n');
    // The whole paragraph plus the role tag fit on a single logical line —
    // termCols=30 has no effect on the output structure.
    expect(lines.length).toBe(1);
    expect(lines[0]).toBe(`Kiro: ${long}`);
  });

  test('blockquote prepends the bar glyph', () => {
    const out = stripAnsi(renderAgentMessage('> a quote'));
    expect(out).toContain('│');
    expect(out).toContain('a quote');
  });

  // Mid-stream input: when the live region routes streaming text through
  // `renderAgentMessage` (lite's incremental markdown rendering for
  // streaming), unclosed inline markers must NOT bleed style into
  // anything that follows. The marked-based inline lexer already does
  // the right thing — unclosed `**`, `*`, `_`, `~~`, `[`, `` ` `` are
  // emitted as literal text — but we lock that contract in here so a
  // future swap of the inline parser can't quietly regress it.
  describe('streaming-shape input (unclosed/partial markdown)', () => {
    function ansiHasBold(s: string): boolean {
      // eslint-disable-next-line no-control-regex
      return /\x1b\[1m/.test(s);
    }
    function ansiHasItalic(s: string): boolean {
      // eslint-disable-next-line no-control-regex
      return /\x1b\[3m/.test(s);
    }

    // Mid-stream: the closing marker hasn't arrived. Each unclosed marker must
    // render verbatim (no style flipped on for the rest of the buffer) until
    // the close streams in. `noStyle` is checked on the body slice only —
    // bold/italic SGR may still come from the role tag's chalk.bold.
    test.each([
      ['bold', '**partial bold', '**partial bold', ansiHasBold],
      [
        'underscore italic',
        '_partial italic',
        '_partial italic',
        ansiHasItalic,
      ],
      ['inline code', 'a `partial code', '`partial code', undefined],
      [
        'link (bracket only)',
        'see [link without close',
        '[link without close',
        undefined,
      ],
      [
        'link (bracket+paren)',
        'see [partial](http',
        '[partial](http',
        undefined,
      ],
      ['strikethrough', '~~partial strike', '~~partial strike', undefined],
    ] as const)(
      'unclosed %s renders as literal text',
      (_name, input, literal, hasStyle) => {
        const out = renderAgentMessage(input);
        expect(stripAnsi(out)).toContain(literal);
        if (hasStyle) {
          const body = out.slice(out.indexOf(literal));
          expect(hasStyle(body)).toBe(false);
        }
      }
    );

    test('mixed closed + unclosed: only the closed span styles', () => {
      // Realistic mid-stream payload — one fully-formed bold span
      // followed by another that's still arriving. The closed `**bold**`
      // must light up; the trailing `**partial` must stay plain.
      const out = renderAgentMessage('normal **bold** then **partial');
      // Bold SGR appears at least once (for `bold`).
      expect(ansiHasBold(out)).toBe(true);
      const stripped = stripAnsi(out);
      expect(stripped).toContain('normal bold then **partial');
      // After the last `[22m` (bold-off / dim-off) closer that ends the
      // styled span, the rest of the line must NOT contain another `[1m`.
      const lastBoldOff = out.lastIndexOf('\x1b[22m');
      expect(lastBoldOff).toBeGreaterThan(-1);
      const tail = out.slice(lastBoldOff);
      expect(ansiHasBold(tail)).toBe(false);
    });

    test('mid-stream bold open does not leak bold past end of buffer', () => {
      // Most important regression: the user's concern. Render content
      // ending with an unclosed bold marker, then check that the very
      // last ANSI state in the buffer leaves bold OFF — so anything
      // appended after this row in twinki's rendering pipeline is not
      // painted bold by inheritance.
      const out = renderAgentMessage('Hello **mid-stream');
      // Walk the whole string through a tiny SGR state machine and
      // verify bold is OFF at the tail. We only need bold here — the
      // wrap-boundary suite above already pins italic/underline/color.
      let bold = false;
      // eslint-disable-next-line no-control-regex
      const re = /\x1b\[([0-9;]*)m/g;
      let m: RegExpExecArray | null;
      while ((m = re.exec(out)) !== null) {
        const codes = m[1] === '' ? ['0'] : m[1]!.split(';');
        for (const c of codes) {
          const n = parseInt(c, 10);
          if (n === 0 || n === 22) bold = false;
          else if (n === 1) bold = true;
        }
      }
      expect(bold).toBe(false);
    });

    test('partial code block (no closing fence) renders as code', () => {
      // Streaming snapshot mid-fence. parseMarkdown marks the block
      // `isComplete: false` and renderCodeBlock paints it with the
      // language tint anyway — the user sees their code styling
      // immediately, not a wall of plain text until the close arrives.
      const out = stripAnsi(
        renderAgentMessage('```ts\nconst x = 1;\nconst y =', 'Kiro')
      );
      expect(out).toContain('const x = 1;');
      expect(out).toContain('const y =');
    });

    test('partial header (no trailing newline) styles immediately', () => {
      // `# H` commits as a header the moment `# ` is seen. As `Header`
      // streams in, each chunk extends the header text in place — this
      // is the same `tryAppendMarkdownDelta` path the modern TUI uses,
      // and our renderer reflects it correctly.
      const out = renderAgentMessage('# Hea');
      // Bold SGR present somewhere — header text is bolded.
      expect(ansiHasBold(out)).toBe(true);
      expect(stripAnsi(out)).toContain('Kiro: Hea');
    });
  });
});

// Regression: block elements (lists, headers, bold headings, blockquotes)
// used to leave inline markdown markers literal. The parser intentionally
// keeps `seg.text` raw on these elements (see the parser test "should keep
// list item text raw for inline parsing"), so the renderer is responsible
// for re-lexing through the inline path. Previously it called
// `renderInlineSegment` (which only honors flags already on the segment)
// and as a result `**bold**`, `` `code` ``, `*italic*`, links, etc. inside
// bullets/headers/quotes bled through with their markers visible. Tables
// and bare paragraphs were unaffected because they go through different
// renderer paths that already invoke `parseInlineMarkdown`.
describe('inline markdown inside block elements', () => {
  // contains: stripped substrings that must appear; absent: stripped
  // substrings that must NOT (markers stripped); ansi: raw SGR codes the
  // styled span must emit. `\x1b[1m`=bold, `[3m`=italic, `[4m`=underline,
  // `[36m`=cyan code span.
  test.each([
    [
      'list item: bold',
      '- **bold** text',
      ['- bold text'],
      ['**'],
      ['\x1b[1m'],
    ],
    [
      'list item: code',
      '- the `frobnicate` function',
      ['- the frobnicate function'],
      ['`'],
      ['\x1b[36m'],
    ],
    [
      'list item: bold + code combined',
      '- **Setting**: use the `--flag` argument',
      ['- Setting: use the --flag argument'],
      ['**', '`'],
      ['\x1b[1m', '\x1b[36m'],
    ],
    [
      'list item: italic',
      '- this is *important* stuff',
      ['- this is important stuff'],
      [],
      ['\x1b[3m'],
    ],
    [
      'list item: link',
      '- see [docs](https://example.com)',
      ['docs', 'https://example.com'],
      [],
      ['\x1b[4m'],
    ],
    [
      'ordered list: bold + code',
      '1. first **important** step\n2. second `command` step',
      ['1. first important step', '2. second command step'],
      ['**', '`'],
      [],
    ],
    [
      'nested list: bold + code at every indent',
      '- top **bold**\n  - nested `code`',
      ['- top bold', '- nested code'],
      ['**', '`'],
      [],
    ],
    ['header: code', '# About `foo`', ['About foo'], ['`'], []],
    [
      'header: bold',
      '## The **important** part',
      ['The important part'],
      ['**'],
      [],
    ],
    [
      'bold heading: code',
      '**Title with `code`**',
      ['Title with code'],
      ['`'],
      [],
    ],
    [
      'blockquote: code',
      '> see the `--help` flag',
      ['│', 'see the --help flag'],
      ['`'],
      [],
    ],
    [
      'blockquote: bold',
      '> this is **important**',
      ['│', 'this is important'],
      ['**'],
      [],
    ],
  ] as const)(
    '%s renders styled without markers',
    (_n, input, contains, absent, ansi) => {
      const out = renderAgentMessage(input);
      const stripped = stripAnsi(out);
      for (const c of contains) expect(stripped).toContain(c);
      for (const a of absent) expect(stripped).not.toContain(a);
      for (const code of ansi) expect(out).toContain(code);
    }
  );

  test('list item: italic strips lone * markers', () => {
    const out = renderAgentMessage('- this is *important* stuff');
    expect(stripAnsi(out)).not.toMatch(/(?<!\*)\*(?!\*)/);
  });

  test('list item: link strips the [label](url) marker shape', () => {
    const out = renderAgentMessage('- see [docs](https://example.com)');
    expect(stripAnsi(out)).not.toMatch(/\[docs\]\(/);
  });

  // The user-reported bug shape: a bulleted list mixing **bold** keys with
  // `code` values — Kiro's typical "settings explanation" output. This is
  // the exact input that used to break.
  test('list of bold-key + code-value pairs renders fully styled', () => {
    const md = [
      '- **port**: the `--port` flag',
      '- **host**: the `--host` flag',
      '- **debug**: the `--debug` flag',
    ].join('\n');
    const out = renderAgentMessage(md);
    const stripped = stripAnsi(out);
    expect(stripped).toContain('- port: the --port flag');
    expect(stripped).toContain('- host: the --host flag');
    expect(stripped).toContain('- debug: the --debug flag');
    expect(stripped).not.toContain('**');
    expect(stripped).not.toContain('`');
    // Both bold and cyan must show up in the styled output.
    expect(out).toContain('\x1b[1m');
    expect(out).toContain('\x1b[36m');
  });
});

describe('markdown scoping', () => {
  // Markdown rendering must be limited to the model role. Tool outputs,
  // user messages, and system messages should pass through unchanged.
  test('user messages do not render markdown', () => {
    const out = stripAnsi(renderUserMessage('# not a heading\n**not bold**'));
    expect(out).toContain('# not a heading');
    expect(out).toContain('**not bold**');
  });

  test('system info does not render markdown', () => {
    const out = stripAnsi(renderSystemInfo('# stays as #'));
    expect(out).toContain('# stays as #');
  });

  test('system error does not render markdown', () => {
    const out = stripAnsi(renderSystemError('**oops**'));
    expect(out).toContain('**oops**');
  });

  test('renderMessageToText routes model role through markdown but not tool', () => {
    const modelOut = stripAnsi(
      renderMessageToText(
        {
          id: 'm1',
          role: 'model',
          content: '# heading',
        },
        'Kiro'
      )
    );
    expect(modelOut).toContain('Kiro: heading');
    expect(modelOut).not.toContain('# heading');

    const toolOut = stripAnsi(
      renderMessageToText(
        {
          id: 't1',
          role: 'tool_use',
          name: 'shell',
          content: JSON.stringify({ command: '# not a heading' }),
          isFinished: true,
        },
        'Kiro'
      )
    );
    // Tool args block should still show the literal `#` from the command.
    expect(toolOut).toContain('# not a heading');
  });
});

describe('renderShellOutputBlock', () => {
  // Empty/blank input → '' so the live region can gate on truthiness; otherwise
  // each source line gets the `! ` gutter, mid-output blanks are preserved
  // (programs pad), trailing blanks trimmed (live buffers end with \n), and
  // embedded PTY escapes pass through untouched.
  test.each<{ name: string; input: string; expected: string[] }>([
    { name: 'empty string → ""', input: '', expected: [''] },
    { name: 'blank spaces → ""', input: '   ', expected: [''] },
    { name: 'only newlines → ""', input: '\n\n\n', expected: [''] },
    {
      name: 'single line gets the gutter',
      input: 'Enter PIN:',
      expected: ['! Enter PIN:'],
    },
    {
      name: 'gutter on every source line',
      input: 'a\nb\nc',
      expected: ['! a', '! b', '! c'],
    },
    {
      name: 'preserves blank lines mid-output',
      input: 'banner\n\nEnter PIN:',
      expected: ['! banner', '! ', '! Enter PIN:'],
    },
    {
      name: 'trims trailing blank lines',
      input: 'done\n\n\n',
      expected: ['! done'],
    },
  ])('$name', ({ input, expected }) => {
    expect(stripAnsi(renderShellOutputBlock(input)).split('\n')).toEqual(
      expected
    );
  });

  test('gutter uses the brand color so /theme swaps reflow it', () => {
    const out = renderShellOutputBlock('hello');
    const esc = String.fromCharCode(27);
    expect(out).toMatch(new RegExp(`${esc}\\[38;2;193;154;255m`)); // chalk.hex('#C19AFF')
  });

  test('respects an explicit theme.brand override', () => {
    const themed = renderShellOutputBlock('hello', {
      brand: (s: string) => `<<${s}>>`,
    } as any);
    expect(themed).toContain('<<! >>hello');
  });

  test('preserves embedded ANSI escapes from the PTY untouched', () => {
    // CLIs emit colored prompts and cursor-positioning escapes; the gutter
    // composes color + content with no normalization in between.
    const colored = '\x1b[32mOK\x1b[0m';
    expect(renderShellOutputBlock(colored)).toContain(colored);
  });
});

// P438908277: inline colors (code, links, URL trailers) used to be hardcoded
// cyan/dim regardless of /theme; they now read from RenderTheme via
// buildRenderTheme(getColor). No-theme callers keep the prior cyan/dim shape.
describe('theme-driven markdown colors', () => {
  // Recognizable RGB triplets for the three slots. Picked far apart from
  // each other and from cyan so we can assert "this slot's color appears
  // exactly here, not elsewhere" without false positives.
  const HIGHLIGHT_RGB = '\x1b[38;2;0;135;255m'; // #0087FF — kiroDark highlight
  const LINK_RGB = '\x1b[38;2;100;200;100m'; //   #64C864 — distinct green
  const SECONDARY_RGB = '\x1b[38;2;128;128;128m'; // #808080 — kiroDark secondary

  // Exercise the full getColor → buildRenderTheme → renderer path. Unknown
  // slots return null so the renderer falls back to its hardcoded default.
  function buildTestTheme(): RenderTheme {
    const mockGetColor = (path: string): any => {
      if (path === 'highlight') return chalk.hex('#0087FF');
      if (path === 'link') return chalk.hex('#64C864');
      if (path === 'secondary') return chalk.hex('#808080');
      return null;
    };
    return buildRenderTheme(mockGetColor as any);
  }

  // Inline code (highlight slot) and links (link + secondary slots, plus an
  // independent underline) must pick up the theme through every block→inline
  // path: bare prose, list items, headers, and table cells. One table per slot
  // family; `notContains` guards the legacy hardcoded cyan never reappears.
  test.each<{
    name: string;
    input: string;
    termCols?: number;
    contains: string[];
    notContains?: string[];
    plainContains?: string[];
    plainAbsent?: string[];
  }>([
    {
      name: 'inline code in prose uses the highlight slot, not cyan',
      input: 'Run `npm install` to fetch deps',
      contains: [HIGHLIGHT_RGB],
      notContains: ['\x1b[36m'],
      plainContains: ['npm install'],
      plainAbsent: ['`'],
    },
    {
      name: 'inline code in a list item (block→inline path)',
      input: '- the `frobnicate` helper',
      contains: [HIGHLIGHT_RGB],
      notContains: ['\x1b[36m'],
      plainContains: ['- the frobnicate helper'],
    },
    {
      name: 'inline code in a header (block→inline path)',
      input: '# Configure `KIRO_HOME`',
      contains: [HIGHLIGHT_RGB],
      plainContains: ['Configure KIRO_HOME'],
      plainAbsent: ['`'],
    },
    {
      name: 'inline code in a table cell',
      input: [
        '| Setting | Default |',
        '| --- | --- |',
        '| `foo` | `bar` |',
      ].join('\n'),
      termCols: 80,
      contains: [HIGHLIGHT_RGB],
      notContains: ['\x1b[36m'],
    },
    {
      name: 'link in prose: label uses link slot, trailer uses secondary',
      input: 'See [the docs](https://example.com/docs) for details',
      // Underline applied independently so links stay distinct on themes
      // whose link color matches prose.
      contains: [
        '\x1b[4m',
        LINK_RGB,
        SECONDARY_RGB,
        '(https://example.com/docs)',
      ],
      plainContains: ['the docs'],
    },
    {
      name: 'link in a list item picks up the link + secondary slots',
      input: '- see [click here](https://example.com)',
      contains: ['\x1b[4m', LINK_RGB, SECONDARY_RGB],
    },
  ])(
    '$name',
    ({
      input,
      termCols,
      contains,
      notContains,
      plainContains,
      plainAbsent,
    }) => {
      const out = renderAgentMessage(input, 'Kiro', buildTestTheme(), termCols);
      for (const s of contains) expect(out).toContain(s);
      for (const s of notContains ?? []) expect(out).not.toContain(s);
      const plain = stripAnsi(out);
      for (const s of plainContains ?? []) expect(plain).toContain(s);
      for (const s of plainAbsent ?? []) expect(plain).not.toContain(s);
    }
  );

  test('bare-URL link (visible label === URL) skips the secondary-colored trailer', () => {
    const theme = buildTestTheme();
    const out = renderAgentMessage(
      '[https://example.com](https://example.com)',
      'Kiro',
      theme
    );
    expect(out).toContain(LINK_RGB);
    // No trailer means no secondary-colored `(url)` block — just the label.
    expect(out).not.toContain(SECONDARY_RGB);
    // URL appears exactly once (the label), not twice (label + trailer).
    const urlMatches = stripAnsi(out).match(/https:\/\/example\.com/g) ?? [];
    expect(urlMatches.length).toBe(1);
  });

  test('blockquote stays italic-only (no theme color applied to bar/body)', () => {
    // Blockquotes are italic + dim `│ ` bar, NOT themed — lock it so a future
    // theming pass doesn't collapse blockquote and inline code into one color.
    const theme = buildTestTheme();
    const out = renderAgentMessage('> a thoughtful aside', 'Kiro', theme);
    expect(out).toContain('\x1b[3m'); // ANSI italic
    expect(out).not.toContain(HIGHLIGHT_RGB);
    expect(out).not.toContain(LINK_RGB);
    expect(stripAnsi(out)).toContain('a thoughtful aside');
  });

  test('default theme fallback keeps the legacy cyan/dim shape (no theme passed)', () => {
    // No-theme callers still get the prior hardcoded cyan/dim (DEFAULT_RENDER_THEME).
    const out = renderAgentMessage(
      'Run `npm install` to fetch [docs](https://example.com)',
      'Kiro'
    );
    expect(out).toContain('\x1b[36m'); // ANSI cyan (named color, default fallback)
    expect(stripAnsi(out)).toContain('npm install');
    expect(stripAnsi(out)).toContain('docs');
  });

  test('buildRenderTheme falls back to chalk.cyan/chalk.dim when getColor throws or returns non-callable', () => {
    // Safety net: a throwing/misconfigured accessor must not blow up the
    // renderer — each missing slot falls through to the prior hardcoded color.
    const brokenTheme = buildRenderTheme((() => {
      throw new Error('theme not loaded');
    }) as any);
    expect(typeof brokenTheme.inlineCode).toBe('function');
    expect(typeof brokenTheme.link).toBe('function');
    expect(typeof brokenTheme.secondary).toBe('function');
    // Probe each one — should produce the legacy cyan / dim output.
    const codeOut = brokenTheme.inlineCode('x');
    const linkOut = brokenTheme.link('x');
    const secOut = brokenTheme.secondary('x');
    expect(codeOut).toContain('\x1b[36m'); // cyan
    expect(linkOut).toContain('\x1b[36m'); // cyan
    expect(secOut).toContain('\x1b[2m'); // dim
  });
});

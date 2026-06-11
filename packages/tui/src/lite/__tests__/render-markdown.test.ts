import './setup-chalk-level.js';

import { describe, test, expect, beforeAll, afterAll } from 'vitest';
import chalk from 'chalk';
import { mkdtempSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
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

// Redirect KIRO_HOME so the verbose tests don't stomp on the developer's
// real ~/.kiro/settings/lite_verbose.json. The directory is removed
// after the suite finishes.
let tmpHome: string | undefined;
let originalKiroHome: string | undefined;
beforeAll(() => {
  originalKiroHome = process.env.KIRO_HOME;
  tmpHome = mkdtempSync(join(tmpdir(), 'kiro-verbose-test-'));
  process.env.KIRO_HOME = tmpHome;
});
afterAll(() => {
  if (originalKiroHome === undefined) {
    delete process.env.KIRO_HOME;
  } else {
    process.env.KIRO_HOME = originalKiroHome;
  }
  if (tmpHome) {
    try {
      rmSync(tmpHome, { recursive: true, force: true });
    } catch {
      // best-effort cleanup
    }
  }
});

// Force chalk colors for consistent test output
chalk.level = 3;

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

    test('long markdown link wrap does not leak underline/color', () => {
      // 30-col layout forces wrap between the link label and its `(url)`
      // trailer. That space cell carries `[39m[24m[2m` — three closers in
      // a row, all of which the bug used to drop.
      const out = renderAgentMessage(
        'see [click here](https://example.com/path)',
        'Kiro',
        undefined,
        30
      );
      const state = ansiStateAtEnd(out);
      expect(state.underline).toBe(false);
      expect(state.color).toBe(false);
    });

    test('bold span wrap does not leak bold', () => {
      // Padding length tuned so the body width runs out exactly at the
      // space after `**bold**`. That space carries `\x1b[22m`.
      const out = renderAgentMessage(
        'xxxxxxxxxxxxxx **bold** more text',
        'Kiro',
        undefined,
        30
      );
      expect(ansiStateAtEnd(out).bold).toBe(false);
    });

    test('italic span wrap does not leak italic', () => {
      const out = renderAgentMessage(
        'xxxxxxxxxxxx *italic* more text',
        'Kiro',
        undefined,
        30
      );
      expect(ansiStateAtEnd(out).italic).toBe(false);
    });

    test('inline code span wrap does not leak color', () => {
      // `` `code` `` renders via `chalk.cyan(...)` — opener `[36m`, closer
      // `[39m`. The closer attaches to the space after the closing backtick.
      const out = renderAgentMessage(
        'xxxxxxxxxxxxxx `code` more text',
        'Kiro',
        undefined,
        30
      );
      expect(ansiStateAtEnd(out).color).toBe(false);
    });

    test('list item with link wrap does not leak underline/color', () => {
      // The link path goes through `renderListItem` → `renderInlineMarkdown`
      // → `renderInlineSegment` (link branch with `chalk.underline.cyan`).
      // Same wrap mechanism, different block context — pin both code
      // paths so a future "fix" of one doesn't quietly regress the other.
      const out = renderAgentMessage(
        '- see [click here](https://example.com/path)',
        'Kiro',
        undefined,
        30
      );
      const state = ansiStateAtEnd(out);
      expect(state.underline).toBe(false);
      expect(state.color).toBe(false);
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

    test('unclosed bold renders as literal text without applying bold', () => {
      // Mid-stream: the closing `**` hasn't arrived yet. The marker must
      // not flip bold on for the remainder of the buffer; the user sees
      // `**partial` verbatim until the close streams in.
      const out = renderAgentMessage('**partial bold');
      expect(stripAnsi(out)).toContain('**partial bold');
      // Bold SGR may still appear from the role tag's `chalk.bold` — we
      // only care that it's NOT present on the body slice.
      const body = out.slice(out.indexOf('**partial bold'));
      expect(ansiHasBold(body)).toBe(false);
    });

    test('unclosed underscore italic renders as literal text', () => {
      const out = renderAgentMessage('_partial italic');
      expect(stripAnsi(out)).toContain('_partial italic');
      const body = out.slice(out.indexOf('_partial italic'));
      expect(ansiHasItalic(body)).toBe(false);
    });

    test('unclosed inline code renders the backtick as literal', () => {
      const out = renderAgentMessage('a `partial code');
      expect(stripAnsi(out)).toContain('`partial code');
    });

    test('unclosed link renders as literal text', () => {
      // Both the bracket-only and the bracket-plus-paren variants are
      // common mid-stream snapshots of `[label](https://…)`. Either
      // shape must round-trip as literal text — no underline, no link.
      const a = renderAgentMessage('see [link without close');
      expect(stripAnsi(a)).toContain('[link without close');
      const b = renderAgentMessage('see [partial](http');
      expect(stripAnsi(b)).toContain('[partial](http');
    });

    test('unclosed strikethrough renders as literal text', () => {
      const out = renderAgentMessage('~~partial strike');
      expect(stripAnsi(out)).toContain('~~partial strike');
    });

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
  test('list item: **bold** renders as bold without ** markers', () => {
    const out = renderAgentMessage('- **bold** text');
    expect(out).toContain('\x1b[1m'); // ANSI bold
    expect(stripAnsi(out)).toContain('- bold text');
    expect(stripAnsi(out)).not.toContain('**');
  });

  test('list item: `code` renders as cyan without backticks', () => {
    const out = renderAgentMessage('- the `frobnicate` function');
    expect(out).toContain('\x1b[36m'); // ANSI cyan (code span color)
    expect(stripAnsi(out)).toContain('- the frobnicate function');
    expect(stripAnsi(out)).not.toContain('`');
  });

  test('list item: bold + code combined both render', () => {
    const out = renderAgentMessage('- **Setting**: use the `--flag` argument');
    const stripped = stripAnsi(out);
    expect(out).toContain('\x1b[1m'); // bold
    expect(out).toContain('\x1b[36m'); // cyan
    expect(stripped).toContain('- Setting: use the --flag argument');
    expect(stripped).not.toContain('**');
    expect(stripped).not.toContain('`');
  });

  test('list item: italic renders without * markers', () => {
    const out = renderAgentMessage('- this is *important* stuff');
    expect(out).toContain('\x1b[3m'); // italic
    expect(stripAnsi(out)).toContain('- this is important stuff');
    expect(stripAnsi(out)).not.toMatch(/(?<!\*)\*(?!\*)/); // no lone *
  });

  test('list item: link renders with underline + url trailer', () => {
    const out = renderAgentMessage('- see [docs](https://example.com)');
    const stripped = stripAnsi(out);
    expect(out).toContain('\x1b[4m'); // underline
    expect(stripped).toContain('docs');
    expect(stripped).toContain('https://example.com');
    expect(stripped).not.toMatch(/\[docs\]\(/);
  });

  test('ordered list: inline markdown renders inside numbered items', () => {
    const out = renderAgentMessage(
      '1. first **important** step\n2. second `command` step'
    );
    const stripped = stripAnsi(out);
    expect(stripped).toContain('1. first important step');
    expect(stripped).toContain('2. second command step');
    expect(stripped).not.toContain('**');
    expect(stripped).not.toContain('`');
  });

  test('nested list: inline markdown renders at every indent level', () => {
    const out = renderAgentMessage('- top **bold**\n  - nested `code`');
    const stripped = stripAnsi(out);
    expect(stripped).toContain('- top bold');
    expect(stripped).toContain('- nested code');
    expect(stripped).not.toContain('**');
    expect(stripped).not.toContain('`');
  });

  test('header: inline `code` renders without backticks', () => {
    const out = renderAgentMessage('# About `foo`');
    const stripped = stripAnsi(out);
    expect(stripped).toContain('About foo');
    expect(stripped).not.toContain('`');
  });

  test('header: inline bold renders without ** markers', () => {
    const out = renderAgentMessage('## The **important** part');
    const stripped = stripAnsi(out);
    expect(stripped).toContain('The important part');
    expect(stripped).not.toContain('**');
  });

  test('bold heading: inline `code` renders without backticks', () => {
    const out = renderAgentMessage('**Title with `code`**');
    const stripped = stripAnsi(out);
    expect(stripped).toContain('Title with code');
    expect(stripped).not.toContain('`');
  });

  test('blockquote: inline `code` renders without backticks', () => {
    const out = renderAgentMessage('> see the `--help` flag');
    const stripped = stripAnsi(out);
    expect(stripped).toContain('│');
    expect(stripped).toContain('see the --help flag');
    expect(stripped).not.toContain('`');
  });

  test('blockquote: inline bold renders without ** markers', () => {
    const out = renderAgentMessage('> this is **important**');
    const stripped = stripAnsi(out);
    expect(stripped).toContain('│');
    expect(stripped).toContain('this is important');
    expect(stripped).not.toContain('**');
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
  test('returns empty string for empty input so callers can pre-render', () => {
    // The live region calls this every render whether or not the PTY has
    // emitted bytes yet. Returning '' lets the JSX gate on truthiness
    // without an extra "is content empty" check at every call site.
    expect(renderShellOutputBlock('')).toBe('');
    expect(renderShellOutputBlock('   ')).toBe('');
    expect(renderShellOutputBlock('\n\n\n')).toBe('');
  });

  test('prefixes a single line with the brand-purple `! ` gutter', () => {
    const out = renderShellOutputBlock('Enter PIN:');
    // Strip ANSI for the structural check; the color is verified
    // separately via a substring search in the next test.
    expect(stripAnsi(out)).toBe('! Enter PIN:');
  });

  test('uses the brand color for the gutter so /theme swaps reflow it', () => {
    // Lock the brand-color contract: the gutter has to come through the
    // theme accessor (or the brand fallback when no theme is provided)
    // so a /theme swap on a future render produces the correct color.
    // We assert the actual hex is present in the rendered ANSI.
    const out = renderShellOutputBlock('hello');
    const esc = String.fromCharCode(27);
    expect(out).toMatch(new RegExp(`${esc}\\[38;2;193;154;255m`)); // chalk.hex('#C19AFF')
  });

  test('respects an explicit theme.brand override', () => {
    // Pass a custom brand that wraps text in a sentinel so we can prove
    // the theme path is honored over the chalk.hex fallback.
    const themed = renderShellOutputBlock('hello', {
      brand: (s: string) => `<<${s}>>`,
    } as any);
    expect(themed).toContain('<<! >>hello');
  });

  test('emits a gutter on every source line', () => {
    const out = stripAnsi(renderShellOutputBlock('a\nb\nc'));
    expect(out.split('\n')).toEqual(['! a', '! b', '! c']);
  });

  test('preserves blank lines mid-output (programs sometimes pad)', () => {
    // mwinit prints a blank row between its banner and the PIN prompt;
    // dropping it would fight the program's intended spacing.
    const out = stripAnsi(renderShellOutputBlock('banner\n\nEnter PIN:'));
    expect(out.split('\n')).toEqual(['! banner', '! ', '! Enter PIN:']);
  });

  test('trims trailing blank lines so the row sits flush', () => {
    // A live-streaming buffer often ends with a trailing \n right after
    // the last real chunk arrives — without trimming, the row would
    // hold an empty `! ` gutter at the bottom that visually disconnects
    // from the input prompt below. The leading content's blanks are
    // preserved (see previous test).
    const out = stripAnsi(renderShellOutputBlock('done\n\n\n'));
    expect(out).toBe('! done');
  });

  test('preserves embedded ANSI escapes from the PTY untouched', () => {
    // mwinit prints colored "OK" lines, sudo highlights its prompt,
    // many CLIs emit cursor-positioning escapes for in-line spinners.
    // The gutter must not strip or rewrite any of them — the wrapper
    // composes color + content with no normalization in between.
    const colored = '\x1b[32mOK\x1b[0m';
    const out = renderShellOutputBlock(colored);
    expect(out).toContain(colored);
  });
});

// Regression tests for P438908277: markdown inline colors (inline code,
// links, link URL trailers) used to be hardcoded to chalk.cyan / chalk.dim
// regardless of the active /theme. The renderer now reads them from the
// supplied {@link RenderTheme}, sourced via `buildRenderTheme(getColor)`
// from the same `getColor` accessor the modern TUI uses. The default
// fallback (no theme passed) keeps the prior cyan/dim shape so unrelated
// tests stay green; the cases below pass an explicit theme to verify the
// full plumbing.
describe('theme-driven markdown colors', () => {
  // Recognizable RGB triplets for the three slots. Picked far apart from
  // each other and from cyan so we can assert "this slot's color appears
  // exactly here, not elsewhere" without false positives.
  const HIGHLIGHT_RGB = '\x1b[38;2;0;135;255m'; // #0087FF — kiroDark highlight
  const LINK_RGB = '\x1b[38;2;100;200;100m'; //   #64C864 — distinct green
  const SECONDARY_RGB = '\x1b[38;2;128;128;128m'; // #808080 — kiroDark secondary

  /**
   * Build a {@link RenderTheme} via the same accessor lite mode uses, so
   * the test exercises the full `getColor → buildRenderTheme → renderer`
   * path. The mock returns chalk truecolor wrappers for the three new
   * slots (`highlight`, `link`, `secondary`); other slots fall through
   * to the renderer's hardcoded fallbacks via the `safeChalk` probe.
   */
  function buildTestTheme(): RenderTheme {
    const mockGetColor = (path: string): any => {
      if (path === 'highlight') return chalk.hex('#0087FF');
      if (path === 'link') return chalk.hex('#64C864');
      if (path === 'secondary') return chalk.hex('#808080');
      // Unknown slot — return something that fails the (probe('') is string)
      // check so the renderer falls back to its hardcoded default.
      return null;
    };
    return buildRenderTheme(mockGetColor as any);
  }

  test('inline code uses theme.inlineCode (highlight slot), not hardcoded cyan', () => {
    const theme = buildTestTheme();
    const out = renderAgentMessage(
      'Run `npm install` to fetch deps',
      'Kiro',
      theme
    );
    expect(out).toContain(HIGHLIGHT_RGB);
    expect(out).not.toContain('\x1b[36m'); // no plain cyan named-color
    expect(stripAnsi(out)).toContain('npm install');
    expect(stripAnsi(out)).not.toContain('`');
  });

  test('link label uses theme.link, URL trailer uses theme.secondary', () => {
    const theme = buildTestTheme();
    const out = renderAgentMessage(
      'See [the docs](https://example.com/docs) for details',
      'Kiro',
      theme
    );
    // Underline is applied independently of the theme color so links stay
    // visually distinct on themes whose link color matches prose.
    expect(out).toContain('\x1b[4m'); // ANSI underline
    expect(out).toContain(LINK_RGB);
    expect(out).toContain(SECONDARY_RGB);
    // The URL itself should sit inside the secondary-colored trailer.
    expect(out).toContain('(https://example.com/docs)');
    expect(stripAnsi(out)).toContain('the docs');
  });

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

  test('inline code inside a list item picks up theme.inlineCode (block→inline path)', () => {
    // List items are block-level segments; their body re-lexes through
    // `renderInlineMarkdown` which threads the theme. This exercises the
    // renderBlockSegment → renderListItem → renderInlineMarkdown path.
    const theme = buildTestTheme();
    const out = renderAgentMessage('- the `frobnicate` helper', 'Kiro', theme);
    expect(out).toContain(HIGHLIGHT_RGB);
    expect(out).not.toContain('\x1b[36m');
    expect(stripAnsi(out)).toContain('- the frobnicate helper');
  });

  test('link inside a list item picks up theme.link', () => {
    const theme = buildTestTheme();
    const out = renderAgentMessage(
      '- see [click here](https://example.com)',
      'Kiro',
      theme
    );
    expect(out).toContain('\x1b[4m');
    expect(out).toContain(LINK_RGB);
    expect(out).toContain(SECONDARY_RGB);
  });

  test('inline code inside a header retains theme.inlineCode (block→inline)', () => {
    const theme = buildTestTheme();
    const out = renderAgentMessage('# Configure `KIRO_HOME`', 'Kiro', theme);
    expect(out).toContain(HIGHLIGHT_RGB);
    expect(stripAnsi(out)).toContain('Configure KIRO_HOME');
    expect(stripAnsi(out)).not.toContain('`');
  });

  test('inline code inside a table cell picks up theme.inlineCode', () => {
    const theme = buildTestTheme();
    const md = [
      '| Setting | Default |',
      '| --- | --- |',
      '| `foo` | `bar` |',
    ].join('\n');
    const out = renderAgentMessage(md, 'Kiro', theme, 80);
    expect(out).toContain(HIGHLIGHT_RGB);
    expect(out).not.toContain('\x1b[36m');
  });

  test('blockquote stays italic-only (no theme color applied to bar/body)', () => {
    // Blockquotes already render as italic + dim `│ ` bar; they are NOT
    // themed via the new slots. Lock that in so a future "extend theming"
    // pass doesn't accidentally collapse blockquote and inline code into
    // the same color again. The bar is dim chrome; the body is italic
    // prose without an inline-code highlight.
    const theme = buildTestTheme();
    const out = renderAgentMessage('> a thoughtful aside', 'Kiro', theme);
    expect(out).toContain('\x1b[3m'); // ANSI italic
    expect(out).not.toContain(HIGHLIGHT_RGB);
    expect(out).not.toContain(LINK_RGB);
    expect(stripAnsi(out)).toContain('a thoughtful aside');
  });

  test('default theme fallback keeps the legacy cyan/dim shape (no theme passed)', () => {
    // Pure-context callers (tests, sub-renderers without ctx, callers
    // that haven't been wired up yet) should still see the prior
    // hardcoded cyan/dim for inline code and link trailers — that's
    // what {@link DEFAULT_RENDER_THEME} guarantees.
    const out = renderAgentMessage(
      'Run `npm install` to fetch [docs](https://example.com)',
      'Kiro'
    );
    expect(out).toContain('\x1b[36m'); // ANSI cyan (named color, default fallback)
    expect(stripAnsi(out)).toContain('npm install');
    expect(stripAnsi(out)).toContain('docs');
  });

  test('buildRenderTheme falls back to chalk.cyan/chalk.dim when getColor throws or returns non-callable', () => {
    // Locks the safety net: a misconfigured theme accessor (missing slot,
    // throwing accessor) must not blow up the renderer. Each missing slot
    // falls through to the prior hardcoded color.
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

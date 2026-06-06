/**
 * Lightweight inline syntax highlighter for the prompt input buffer.
 *
 * Highlights three token classes that the user routinely types and needs
 * to distinguish from prose:
 *   - file paths (/abs/..., ./rel, ../up, ~/home)
 *   - slash commands at the start of input (`/help`, `/verbose foo`)
 *   - URLs (http://, https://, file://)
 *
 * Tokens are scanned in a single pass over the visible input string. The
 * caller wraps un-tokenized runs with their own primary chalk color so
 * the cursor / theme styling outside of tokens is unaffected.
 *
 * Returns a list of { start, end, color } spans the caller can splice
 * into its segment rendering. Keeps the highlighter free of any React /
 * theme imports so it can be tested as a pure function.
 */

export type InputSpanKind = 'path' | 'slash' | 'url';

export interface InputSpan {
  /** Start index in the input string (inclusive). */
  start: number;
  /** End index (exclusive). */
  end: number;
  kind: InputSpanKind;
}

// URL: http(s):// or file:// up to the next whitespace.
const URL_RE = /\b(?:https?|file):\/\/[^\s)]+/g;

// File path tokens: starts with /, ./, ../, or ~/ and runs to the next
// whitespace. We don't try to validate the path on disk — the goal is
// visual recognition, not correctness. A leading slash followed by a
// non-letter is treated as prose ("/" at the start of a slash command
// is handled separately below).
const PATH_RE = /(^|\s)((?:\/|\.\.?\/|~\/)[^\s)]+)/g;

/**
 * Compute the highlight spans for the given visible input text.
 *
 * Order of detection:
 *   1. URLs first — these would otherwise match the path regex.
 *   2. Slash command prefix — only when the buffer starts with `/<word>` AND
 *      `<word>` is a recognized command name. Without `knownCommands` we
 *      conservatively skip slash highlighting so an absolute path like
 *      `/tmp/screenshot.png` doesn't get its leading `/tmp` recolored as a
 *      command (which made paths read as two-toned and obscured the path
 *      handler downstream).
 *   3. Paths — anything starting with `/`, `./`, `../`, or `~/` that isn't
 *      already covered by an earlier span.
 */
export function computeInputSpans(
  text: string,
  knownCommands?: ReadonlySet<string>
): InputSpan[] {
  if (!text) return [];
  const spans: InputSpan[] = [];

  // URLs
  for (const m of text.matchAll(URL_RE)) {
    if (m.index == null) continue;
    spans.push({ start: m.index, end: m.index + m[0].length, kind: 'url' });
  }

  // Slash command at the very start (covers /help, /verbose, etc.). We only
  // highlight when `<word>` is a real command — otherwise the user is typing
  // a path or a slash-led token that shouldn't borrow the command color. The
  // command word itself is the only span we emit; args can be paths, which
  // the path pass below picks up.
  if (knownCommands && text.startsWith('/')) {
    const word = /^\/[A-Za-z][A-Za-z0-9_-]*/.exec(text);
    if (word) {
      const cmd = word[0]; // includes the leading '/'
      // Treat the token as a command only when it terminates cleanly —
      // followed by whitespace, end-of-input, or an arg separator. Any
      // continuation (`/help/foo`, `/help.tsx`) means we're really looking
      // at a path or an extension, not a command invocation.
      const next = text.charAt(word[0].length);
      const cleanlyTerminated = next === '' || /\s/.test(next);
      if (cleanlyTerminated && knownCommands.has(cmd)) {
        spans.push({ start: 0, end: word[0].length, kind: 'slash' });
      }
    }
  }

  // Paths
  for (const m of text.matchAll(PATH_RE)) {
    if (m.index == null) continue;
    const lead = m[1] ?? '';
    const start = m.index + lead.length;
    const end = start + (m[2] ?? '').length;
    if (end - start <= 1) continue; // bare "/" alone isn't a path
    if (overlapsExisting(spans, start, end)) continue;
    spans.push({ start, end, kind: 'path' });
  }

  // Sort by start so the caller can iterate left-to-right.
  spans.sort((a, b) => a.start - b.start);
  return mergeOverlaps(spans);
}

function overlapsExisting(
  spans: InputSpan[],
  start: number,
  end: number
): boolean {
  for (const s of spans) {
    if (start < s.end && end > s.start) return true;
  }
  return false;
}

/** Drop any later span that overlaps an earlier one. */
function mergeOverlaps(spans: InputSpan[]): InputSpan[] {
  if (spans.length <= 1) return spans;
  const out: InputSpan[] = [spans[0]!];
  for (let i = 1; i < spans.length; i++) {
    const cur = spans[i]!;
    const prev = out[out.length - 1]!;
    if (cur.start < prev.end) continue;
    out.push(cur);
  }
  return out;
}

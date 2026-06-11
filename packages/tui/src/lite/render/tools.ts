import chalk from 'chalk';
import { renderUnifiedDiff } from '../diff.js';
import {
  READ_TOOL_NAMES,
  GREP_TOOL_NAMES,
  GLOB_TOOL_NAMES,
  CODE_TOOL_NAMES,
  INTROSPECT_TOOL_NAMES,
  SHELL_TOOL_NAMES,
  WRITE_TOOL_NAMES,
} from '../../types/agent-events.js';
import { visibleWidth } from '../../utils/text-width.js';
import { shouldShowToolOutput } from '../verbose.js';
import type { Glyphs } from '../../utils/glyphs.js';
import {
  resolveGlyphs,
  brand,
  softSuccessOutput,
  type RenderTheme,
} from './theme.js';
import {
  wrapAnsiLine,
  wrapAtWords,
  wrapKeyedLine,
  wrapPlainLine,
  clipChars,
  clipVisibleWidth,
  highlightLineSafe,
  resolveLanguageFromPathLite,
} from './text.js';

export interface ToolCallRenderInfo {
  name: string;
  /** Reasoning ("why") text — rendered in brand (purple) color. With an
   *  inline arg present, all reasoning lines render below the tool name
   *  on their own lines so "what" (args, white) and "why" (reasoning,
   *  purple) are visually separated. Without an inline arg, the first
   *  line of reasoning sits inline next to the tool name (legacy shape)
   *  and continuation lines indent below.
   */
  description?: string;
  /** Inline arg chip — rendered in default (white) color directly after
   *  the tool name. Looks like `tool [args]`. Independent of `description`
   *  so a tool can show both its args AND its reasoning when the user has
   *  /verbosity inline-args + reasoning enabled. */
  inlineArg?: string;
  mcpServer?: string;
  agentPrefix?: string;
  elapsed?: number;
  status: 'running' | 'done' | 'error' | 'cancelled';
  isTrivial?: boolean;
  rejected?: boolean;
  /** Spinner glyph to substitute for the static ' ...' placeholder when a
   *  non-trivial tool is in `running` status. The lite live region passes
   *  the active spinner frame here so an in-flight tool's chat-log row
   *  picks up motion, while keeping every other dimension of the render
   *  (args, diff, reasoning, output) identical to its eventual settled
   *  appearance. Trivial tools (read/grep/glob) ignore this and stick
   *  with ' ...' — they're short-lived and a spinner is visual noise. */
  runningSpinner?: string;
  /** When true, this tool is the current pending-approval target — the
   *  user hasn't responded to the approval prompt yet. The status slot
   *  renders a yellow ' ...' (matching the approval prompt's yellow
   *  hotkey color) instead of the spinner glyph or dim ellipsis, so the
   *  tool body honestly says "waiting on you, not me" — the agent isn't
   *  actually progressing while approval is pending. Takes precedence
   *  over runningSpinner. Trivial vs non-trivial doesn't matter here:
   *  any tool that needs approval gets the same yellow signal. */
  awaitingApproval?: boolean;
}

export function renderToolCall(
  info: ToolCallRenderInfo,
  theme?: RenderTheme
): string {
  const isTrivial = info.isTrivial ?? TRIVIAL_TOOLS.has(info.name);

  const brandFn = theme?.brand ?? brand;
  const agent = info.agentPrefix ? chalk.blue(info.agentPrefix) : '';
  const source = info.mcpServer ? chalk.dim(`${info.mcpServer}/`) : '';
  const name = isTrivial ? chalk.dim.bold(info.name) : chalk.bold(info.name);
  // Inline arg chip — rendered uncolored (terminal default, which is white in
  // dark themes / black in light themes) so it visually reads as a literal
  // "what was passed" next to the tool name. NOT brand-colored — that color
  // is reserved for reasoning so the eye can distinguish what-vs-why at a
  // glance when both are shown.
  const argChip = info.inlineArg ? ` ${info.inlineArg}` : '';

  let statusStr: string;
  switch (info.status) {
    case 'running':
      // Awaiting-approval signal takes priority. The tool is queued behind
      // the user's y/t/n response, so painting the braille spinner would
      // lie about progress. Yellow ' ...' matches the approval prompt's
      // [t] hotkey color so the eye chains tool body → approval prompt as
      // a single visual unit. Applies to trivial and non-trivial tools
      // alike — any tool sitting in the queue gets the same signal.
      // Live region passes a spinner glyph for non-trivial tools so the
      // user sees motion. Trivial tools (read/grep/glob) keep ' ...' —
      // they're short-lived enough that a spinner adds noise. The static
      // chat-log path passes no spinner and gets ' ...' for both, which
      // is the correct settled appearance once the tool moves to done.
      if (info.awaitingApproval) {
        statusStr = chalk.yellow(' ...');
      } else if (info.runningSpinner && !isTrivial) {
        statusStr = ` ${info.runningSpinner}`;
      } else {
        statusStr = chalk.dim(' ...');
      }
      break;
    case 'done':
      statusStr =
        info.elapsed != null
          ? chalk.dim(` ${formatElapsed(info.elapsed)}`)
          : '';
      break;
    case 'error':
      statusStr = info.rejected ? chalk.red(' DENIED') : chalk.red(' FAILED');
      break;
    case 'cancelled':
      statusStr = chalk.yellow(' ✗ cancelled');
      break;
  }

  // Description (reasoning) layout.
  //
  // With an inline arg chip present, reasoning ALWAYS goes on its own purple
  // line(s) below the tool name — the tool line becomes "tool [args]" and
  // the reasoning sits underneath as a distinct purple block. This is what
  // the user sees with /verbosity inline-args + reasoning enabled: args and
  // reasoning don't compete for the same visual slot.
  //
  // Without an inline arg chip, the first line of reasoning sits inline next
  // to the tool name (legacy shape — the brand-colored "why" was the only
  // signal next to the name) and continuation lines indent below.
  const descLines = info.description ? info.description.split('\n') : [];
  const inlineDesc =
    info.inlineArg || descLines.length === 0
      ? ''
      : ` ${brandFn(descLines[0] ?? '')}`;
  const indentedLines = info.inlineArg ? descLines : descLines.slice(1);
  const firstLine = `${agent}${source}${name}${argChip}${inlineDesc}${statusStr}`;
  if (indentedLines.length === 0) return firstLine;
  const indent = '    '; // indent for continuation / below-name lines
  return [
    firstLine,
    ...indentedLines.map((l) => `${indent}${brandFn(l)}`),
  ].join('\n');
}

/**
 * Render a write/edit tool call with a unified diff (line numbers,
 * surrounding context, ... separators between non-contiguous hunks). Used
 * by both the chat-log finalizer and the approval prompt — the caller
 * decides whether to also render args. When `suppressDiff` is true (e.g.
 * the diff is being rendered above in the approval prompt), only the bare
 * tool-call header line is returned.
 */
export function renderWriteToolCall(
  info: ToolCallRenderInfo,
  content: string,
  opts: {
    suppressDiff?: boolean;
    termCols?: number;
    startLine?: number;
    theme?: RenderTheme;
  } = {}
): string {
  if (opts.suppressDiff) return renderToolCall(info, opts.theme);

  let path: string | undefined;
  let oldText = '';
  let newText = '';
  let startLine = opts.startLine ?? 1;

  try {
    const args = JSON.parse(content);
    path = args.path;
    // Wire format is snake_case (Rust serde — see crates/chat-cli/src/cli/
    // chat/tools/fs_write.rs `enum FsWrite`): command tag is `str_replace`
    // / `create` / `insert` / `append`, fields are `old_str` / `new_str` /
    // `file_text` / `insert_line`. Accept the camelCase variants as
    // fallback so any non-Rust caller that historically sent that shape
    // (KAS native tool, older mocks, future MCP-routed write tool) keeps
    // working.
    const oldStr = args.old_str ?? args.oldStr;
    const newStr = args.new_str ?? args.newStr;
    const fileText = args.file_text ?? args.content;
    const insertLine = args.insert_line ?? args.insertLine;
    if (
      args.command === 'str_replace' ||
      args.command === 'strReplace' ||
      (oldStr && newStr != null)
    ) {
      oldText = String(oldStr ?? '');
      newText = String(newStr ?? '');
    } else if (args.command === 'insert' || insertLine != null) {
      // For insert, oldText is empty and newText is the inserted block.
      oldText = '';
      newText = String(newStr ?? fileText ?? '');
      if (typeof insertLine === 'number') startLine = insertLine + 1;
    } else if (args.command === 'append') {
      // Append: oldText is empty, newText is the appended block. Without
      // a baseline file read we can't show the trailing context, so the
      // diff renders as a pure-add block — same shape as create.
      oldText = '';
      newText = String(newStr ?? fileText ?? '');
    } else if (
      args.command === 'create' ||
      (!oldStr && (fileText != null || newStr != null))
    ) {
      // Brand-new file — show all lines as additions.
      oldText = '';
      newText = String(fileText ?? newStr ?? '');
    }
  } catch {
    return renderToolCall(info, opts.theme);
  }

  // No synthesized "{path} (N lines)" summary on the header. The path is
  // already in the inline arg chip ("[create /tmp/foo.txt]") and the diff
  // footer prints the actual added/removed counts. A third copy as fake
  // purple "reasoning" duplicated the path twice and disagreed with the
  // diff's count (split('\n') vs counted added lines).
  const out: string[] = [renderToolCall(info, opts.theme)];
  // `suppressPathHeader: true` because the inline arg chip on the
  // tool-call header above already shows the path — printing it again
  // as the first row of the diff body would duplicate it. We still
  // pass `path` so the diff renderer can pick a syntax-highlight
  // language. The approval-prompt caller leaves the header on because
  // its surrounding chrome doesn't show the path anywhere else.
  const diff = renderUnifiedDiff(oldText, newText, {
    path,
    suppressPathHeader: true,
    startLine,
    termCols: opts.termCols,
    // Pass through the per-render theme so the diff's bg + bar colors
    // follow /settings theme (kiroDark ↔ kiroLight) the same way the
    // header line above does. Without this, the diff stays on its
    // hardcoded dark-tinted palette regardless of what theme the user
    // selected — fine on dark terminals, wrong on light.
    theme: opts.theme,
  });
  if (diff.length > 0) {
    // Write diffs always render in full — they materialize whole at
    // finish time and the entire change is the payload the user is
    // reviewing, so there's no tail to safely drop. (Read tool bodies
    // still honor outputMaxLines; diffs deliberately opt out.)
    out.push(...diff);
  }
  return out.join('\n');
}

/**
 * Render a read-style tool call (fs_read and friends). Mirrors
 * {@link renderWriteToolCall}'s shape — a header followed by a path line and a
 * numbered, syntax-highlighted body — so reads and writes read the same way
 * in scrollback. The body has no gutter glyph or background tint (nothing was
 * added or removed; this is just inspection), just dim line numbers and the
 * highlighted source.
 *
 * Output is sliced to {@link maxLines} visual rows (after wrapping). The cap
 * is the same `outputMaxLines` knob the bar formatter uses, so the user's
 * /verbosity truncation choice covers both surfaces.
 */
export function renderReadToolCall(
  info: ToolCallRenderInfo,
  content: string,
  result?: { status: string; error?: string; output?: unknown },
  opts: {
    termCols?: number;
    maxLines?: number | null;
    maxCharsPerLine?: number | null;
    theme?: RenderTheme;
    glyphs?: Glyphs;
  } = {}
): string {
  const cols = opts.termCols ?? 80;
  const g = resolveGlyphs(opts.glyphs);
  let path: string | undefined;
  let startLine = 1;
  try {
    const args = JSON.parse(content);
    // Operations is the multi-read shape. Single-op tools may also use
    // top-level path/file_path — fall back to those when present.
    const op = Array.isArray(args.operations) ? args.operations[0] : null;
    path = op?.path ?? args.path ?? args.file_path ?? args.filePath;
    if (op && typeof op.offset === 'number') startLine = op.offset + 1;
    else if (typeof args.offset === 'number') startLine = args.offset + 1;
  } catch {
    // fall through; the path header will simply be skipped.
  }

  const out: string[] = [renderToolCall(info, opts.theme)];

  // Errors take the bar-formatted error path so they stay visible — read
  // errors are usually permission/path issues and deserve the warning chrome.
  if (result?.status === 'error' && result.error) {
    const indent = '    ';
    const barPrefix = `${indent}${g.lineVertical} `;
    const avail = Math.max(20, cols - visibleWidth(barPrefix));
    // Errors keep their loud red across glyph AND body — short, rare, and
    // demand the user's attention. Contrast with normal output, where only
    // the glyph carries chrome styling so the body text reads as content.
    const lines = formatBarBlock(
      result.error,
      avail,
      barPrefix,
      chalk.red,
      chalk.red
    );
    if (lines.length > 0) out.push(...lines);
    return out.join('\n');
  }

  // No body to surface — fall back to the bare tool-call header so the row
  // still says what was attempted.
  if (result?.output == null) return out.join('\n');
  const text =
    typeof result.output === 'string'
      ? result.output
      : unwrapToolOutputAsText(result.output);
  if (!text.trim()) return out.join('\n');

  if (path) out.push(chalk.dim(`  ${path}`));

  const language = resolveLanguageFromPathLite(path);

  // Layout: 2-space indent + line# (right-padded to LINE_NUM_WIDTH) + space.
  // Mirrors the diff renderer's number column so reads and writes line up
  // visually when interleaved in scrollback.
  const LINE_NUM_WIDTH = 4;
  const linePrefixCols = 2 + LINE_NUM_WIDTH + 1;
  const codeCols = Math.max(20, cols - linePrefixCols);
  const sourceLines = text.replace(/\n+$/, '').split('\n');

  // Wrap each highlighted line to the available width. Continuation rows
  // share the line number's column but render its number dim+blank so the
  // eye still tracks the original line.
  const visualRows: string[] = [];
  for (let i = 0; i < sourceLines.length; i++) {
    const line = sourceLines[i] ?? '';
    const numStr = String(startLine + i).padStart(LINE_NUM_WIDTH);
    const dimNum = chalk.dim(`  ${numStr} `);
    const blankNum = chalk.dim('  ' + ' '.repeat(LINE_NUM_WIDTH) + ' ');
    const styled = highlightLineSafe(line, language);
    // Wrap on the *visible* width, not raw chars, so highlighted ANSI bytes
    // don't blow up the chunk count. wrapAtWords handles word boundaries
    // and falls back to a hard cut on long unbreakable runs.
    const chunks = wrapAtWords(styled, codeCols, codeCols);
    if (chunks.length === 0) {
      visualRows.push(dimNum);
      continue;
    }
    visualRows.push(`${dimNum}${chunks[0]}`);
    for (let j = 1; j < chunks.length; j++) {
      visualRows.push(`${blankNum}${chunks[j]}`);
    }
  }

  // Per-row char clip honors the user's /verbosity char cap. Visible-width
  // aware so it doesn't slice mid-ANSI-escape and corrupt downstream rows.
  const clipped =
    opts.maxCharsPerLine && opts.maxCharsPerLine > 0
      ? visualRows.map((r) => clipVisibleWidth(r, opts.maxCharsPerLine!))
      : visualRows;

  // Tail-window at maxLines: keep the LAST N visual rows, prepend the
  // truncation marker above them. Mirrors the output-bar behavior the user
  // saw while the tool was streaming — by the time the read finalizes, the
  // window had scrolled to the file's tail so that's what gets frozen into
  // scrollback. Marker is dimmed so it doesn't compete with the file
  // content. The line-count footer below still names the full source size,
  // so a truncated read still tells the user the file's true length.
  const capped = applyTailLineCap(clipped, opts.maxLines ?? null, (n) =>
    chalk.dim(`  ${' '.repeat(LINE_NUM_WIDTH)} ... (+${n} more lines above)`)
  );

  // Footer: line count summary so the user sees how big the file is even
  // when truncated. Cheap and matches the diff's "added N lines" trailer.
  out.push(...capped);
  const lineCount = sourceLines.length;
  out.push(chalk.dim(`  ${lineCount} ${lineCount === 1 ? 'line' : 'lines'}`));
  return out.join('\n');
}

export function formatElapsed(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

/** Tools that are "trivial" reads — rendered dimmed like Claude Code does. */
const TRIVIAL_TOOLS = new Set([
  ...READ_TOOL_NAMES,
  ...GREP_TOOL_NAMES,
  ...GLOB_TOOL_NAMES,
  ...CODE_TOOL_NAMES,
  ...INTROSPECT_TOOL_NAMES,
]);

const WRITE_TOOLS = new Set([
  'fs_write',
  'str_replace',
  'write',
  'edit',
  'create_file',
  'write_file',
]);

export function isWriteTool(name: string): boolean {
  return WRITE_TOOLS.has(name);
}

export function isReadTool(name: string): boolean {
  return READ_TOOL_NAMES.has(name);
}

/**
 * Render the tool's *output* (the response we got back) so the chat log shows
 * what the command actually did, not just what was asked. The output gets a
 * `│ ` left-bar at column 4 — same indent as the args block above it — so a
 * tool call reads top-to-bottom as { name } → args → response.
 *
 * We show the full output (no line cap). Each line is wrapped to the terminal
 * width, with continuation rows preserving the same indent so unbreakable runs
 * (URLs, paths) don't crash back to column 0. Errors get a red bar instead of
 * dim so they're impossible to miss.
 *
 * Gated by /verbose. When enabled, the per-tool filter list decides whether
 * THIS specific tool's output renders — `["all"]` means everything, otherwise
 * the tool's name or category must appear in the list.
 */
export function renderVerboseOutput(
  toolName: string,
  result?: { status: string; error?: string; output?: unknown },
  maxLines?: number | null,
  filtersOverride?: readonly string[],
  maxCharsPerLine?: number | null,
  termCols?: number,
  glyphs?: Glyphs
): string {
  if (!result) return '';
  // Errors always surface — the verbose filter only gates *successful* output.
  // A failed tool call without its error message reads as a silent "FAILED"
  // chip, which gives the user no path forward. The dropdown bar reuses the
  // same indent/styling as a normal output bar so it reads as part of the
  // tool block, just colored red.
  const isError = result.status === 'error';
  if (!isError && !shouldShowToolOutput(toolName, filtersOverride)) return '';
  // termCols is threaded from RenderContext at flush time (LiteLayout reads
  // process.stdout.columns once per render). A direct read here would diverge
  // from every other tool renderer and bake a different width into already-
  // flushed scrollback rows after a terminal resize.
  const cols = Math.max(40, termCols ?? 120);
  // Bar prefix sits at column 4, leaving room for the bar glyph + space.
  const indent = '    ';
  const g = resolveGlyphs(glyphs);
  const barPrefix = `${indent}${g.lineVertical} `;
  const barCols = visibleWidth(barPrefix);
  const avail = Math.max(20, cols - barCols);

  // Dim "output:" header above the bar — visually parallels args keys
  // ("command:", "path:", etc.) at the same 2-space indent. Tells the
  // user the `│` block below is the tool's output rather than letting
  // the args block and the bar visually merge into one ambiguous chunk.
  // Empty-output paths early-return before the header is emitted, so a
  // tool that produces nothing renders no orphan label.
  const outputHeader = chalk.dim('  output:');

  // Per-line char truncation. When set, each post-wrap row is tail-clipped
  // at `maxCharsPerLine` characters of *visible* width — useful for users
  // who want short output rows even when the terminal is wide. Applies to
  // both error and normal output paths so the user-set cap is consistent.
  const clipRow = (s: string): string => {
    if (maxCharsPerLine == null || maxCharsPerLine <= 0) return s;
    return clipVisibleWidth(s, maxCharsPerLine);
  };

  if (result.status === 'error') {
    // Prefer the explicit error field; fall back to unwrapping the output
    // envelope so failures that surface their reason in the result body
    // (validation messages, exception text) still get visible chrome.
    let errText = result.error ?? '';
    if (!errText && result.output != null) {
      errText =
        typeof result.output === 'string'
          ? result.output
          : unwrapToolOutputAsText(result.output);
    }
    const trimmed = errText.trim();
    if (trimmed.length === 0) return '';
    // Errors keep their loud red across glyph AND body — they're short,
    // rare, and the user wants them to be visible. Normal output below
    // splits the styling so only the glyph carries chrome.
    const lines = formatBarBlock(
      errText,
      avail,
      barPrefix,
      chalk.red,
      chalk.red
    );
    if (lines.length === 0) return '';
    // Errors are already short and matter — apply the cap so the marker
    // formatting stays consistent, but the cap rarely fires here. Tail-cap
    // (marker above) for parity with normal tool output, so a truncated
    // error reads the same way as the success path: the user just saw the
    // tail of stderr scroll past in the live region.
    const capped = applyTailLineCap(lines, maxLines ?? null, (n) =>
      chalk.red(`${barPrefix}... (truncated; +${n} more lines above)`)
    );
    return '\n' + outputHeader + '\n' + capped.map(clipRow).join('\n');
  }
  if (result.output == null) return '';
  const unwrapped: UnwrappedToolOutput =
    typeof result.output === 'string'
      ? { kind: 'text', value: result.output }
      : unwrapToolOutput(result.output);

  // Structured envelope we couldn't reduce to a string — render the parsed
  // object as a key:value tree, same shape as input args (just with the
  // `│` bar prefix so it still reads as tool output). No depth cap — users
  // already control footprint via outputMaxLines/outputMaxChars.
  //
  // Body picks up the same soft sage-green success tint that text-shape
  // outputs get below, so the green-on-success / red-on-error outcome
  // signal is consistent across both branches. Without this, structured-
  // JSON tools (most MCP servers, knowledge searches, code-intel) read
  // as plain chrome while text-shape tools (shell, grep) carry the tint
  // — same successful result, two different visual treatments.
  if (unwrapped.kind === 'json') {
    const treeLines = formatJsonAsBarLines(
      unwrapped.value,
      barPrefix,
      cols,
      maxCharsPerLine ?? null,
      softSuccessOutput
    );
    if (treeLines.length === 0) return '';
    const capped = applyTailLineCap(treeLines, maxLines ?? null, (n) =>
      chalk.dim(`${barPrefix}... (truncated; +${n} more lines above)`)
    );
    return '\n' + outputHeader + '\n' + capped.map(clipRow).join('\n');
  }

  if (!unwrapped.value.trim()) return '';
  // Bar glyph stays dim chrome; body picks up the soft sage-green
  // success tint so the block reads as "result came back, no errors."
  // Errors above use red+red on the same formatter — green vs red on
  // the body color signals the outcome at a glance.
  const lines = formatBarBlock(
    unwrapped.value,
    avail,
    barPrefix,
    chalk.dim,
    softSuccessOutput
  );
  if (lines.length === 0) return '';
  // Tail-window: the user watched output stream past in the live region,
  // and by tool-finish time the visible window had scrolled to the tail.
  // Freeze that view into scrollback so the static render matches what was
  // last on screen, with the truncation marker above naming the dropped
  // count.
  const capped = applyTailLineCap(lines, maxLines ?? null, (n) =>
    chalk.dim(`${barPrefix}... (truncated; +${n} more lines above)`)
  );
  return '\n' + outputHeader + '\n' + capped.map(clipRow).join('\n');
}

/**
 * Render an in-flight tool's accumulated output as a `│`-bar block, sized
 * to the user's truncation cap with the marker positioned ABOVE the visible
 * window. Used by {@link LiteLiveRegion} so users see tool output stream in
 * during the run instead of getting one big chunk at finish time.
 *
 * Behavior contract (mirrors the user-facing /verbosity truncation knob):
 *   - At any given moment, render at most `outputMaxLines` visual rows.
 *   - The visible window shows the *tail* (most recent rows) — as new
 *     output arrives the window scrolls forward, the same way `tail -f`
 *     pinned to the last N lines feels.
 *   - When more rows have arrived than fit, prepend a "(streaming; +N
 *     more lines above)" marker so the user knows content scrolled past.
 *   - When `outputMaxLines` is null/unbounded, every row renders and no
 *     marker appears.
 *
 * Visibility gate: respects `shouldShowToolOutput(toolName)`. Tools the
 * user has filtered out of the output bar don't get a streaming preview
 * either — the spinner line above is enough signal that they're running.
 *
 * Backend dependency note: this helper only produces output when the
 * `liveOutputs` map is populated for this tool. The store fills that map
 * from `ToolCallUpdate` events that carry incremental text content. If
 * the backend (Rust ACP, KAS, etc.) doesn't emit per-chunk text for a
 * given tool — which the V2 Rust backend currently doesn't for shell —
 * the live bar simply never appears for that tool. The static rendering
 * after `ToolCallFinished` still uses the same tail-window cap, so
 * scrollback ends up consistent either way.
 *
 * Returns an empty array when there's nothing to render (no output yet,
 * filtered out, all-whitespace). Caller checks `length === 0` to decide
 * whether to emit the bar block at all.
 */
export function renderLiveStreamingOutputBar(
  toolName: string,
  sourceLines: readonly string[],
  opts: {
    outputMaxLines: number | null;
    outputMaxChars: number | null;
    termCols: number;
    filtersOverride?: readonly string[];
    glyphs?: Glyphs;
  }
): string[] {
  if (!shouldShowToolOutput(toolName, opts.filtersOverride)) return [];
  if (sourceLines.length === 0) return [];

  const cols = Math.max(40, opts.termCols);
  // Bar prefix at column 4, matching the static output bar so the live
  // preview lines up exactly with where the finalized version will land.
  const indent = '    ';
  const g = resolveGlyphs(opts.glyphs);
  const barPrefix = `${indent}${g.lineVertical} `;
  const barCols = visibleWidth(barPrefix);
  const avail = Math.max(20, cols - barCols);

  // Skip purely-whitespace buffers — formatBarBlock would emit empty rows
  // and the live region would flash a sea of `│` glyphs while the user
  // waits for real output. Once a non-whitespace line lands the test
  // passes and we render it.
  const joined = sourceLines.join('\n');
  if (!joined.trim()) return [];

  // Glyph dim, body in the success-green tint — same split as the
  // static finalizer in {@link renderVerboseOutput} so the live preview
  // and the eventual scrollback row match exactly. The green tint
  // signals "successful result" and stays consistent from in-flight
  // through completion. Errors take a separate path with red+red.
  const lines = formatBarBlock(
    joined,
    avail,
    barPrefix,
    chalk.dim,
    softSuccessOutput
  );
  if (lines.length === 0) return [];

  // Per-row char clip for users who set outputMaxChars. Visible-width
  // aware so it doesn't slice mid-ANSI-escape.
  const clipped =
    opts.outputMaxChars != null && opts.outputMaxChars > 0
      ? lines.map((l) => clipVisibleWidth(l, opts.outputMaxChars!))
      : lines;

  // Tail-window with "above" marker. n is in VISUAL rows (post-wrap), the
  // same unit the cap is expressed in, so the user-facing count is
  // self-consistent — no mixing of source-line and visual-row units. The
  // word "streaming" makes the marker distinguishable from the finalized
  // static rendering, which uses "truncated" — readers can tell at a
  // glance whether content is still arriving.
  const tailCapped = applyTailLineCap(
    clipped,
    opts.outputMaxLines ?? null,
    (n) => chalk.dim(`${barPrefix}... (streaming; +${n} more lines above)`)
  );
  // Dim "output:" header above the bar — mirrors the static rendering in
  // {@link renderVerboseOutput}, so the live preview and the finalized
  // scrollback row look identical. Prepended as the first array entry; the
  // caller (LiteLiveRegion) joins these lines with '\n', so the header
  // lands one row above the first bar line. Empty-output paths above
  // already returned [] before reaching here, so the header never appears
  // alone.
  return [chalk.dim('  output:'), ...tailCapped];
}

/**
 * Pretty-print a parsed JSON object as a sequence of bar-prefixed rows,
 * mirroring the input-args tree (`key: value`, nested objects/arrays
 * indented). Reuses {@link formatArgLines} for the body so behavior matches
 * the args block exactly — same dim-key styling, same array `-` markers,
 * same wrap rules — and prepends `│ ` to each emitted row so the result
 * still reads as a tool-output block visually.
 *
 * Maxdepth is set to a very large number rather than capped: the user
 * already controls footprint via outputMaxLines, and capping depth here
 * would hide nested fields the user explicitly opted in to seeing.
 *
 * `bodyColor` (optional) wraps each rendered body line so structured JSON
 * outputs can pick up the same outcome-signal tint that text-shape
 * outputs get from {@link formatBarBlock} — without it, JSON-shape tool
 * results would read as plain chrome while text-shape ones carry the
 * green tint. Wrapping with chalk produces nested SGR sequences (the
 * inner `chalk.dim` for keys stays intact because chalk uses dim-on/off
 * codes, not full resets), so dim keys still read as dim-keys-with-tint.
 */
function formatJsonAsBarLines(
  parsed: unknown,
  barPrefix: string,
  termCols: number,
  maxChars: number | null,
  bodyColor?: (s: string) => string
): string[] {
  // Tree nodes use 2 cols/level of leading indent. We render at indent=0
  // (no leading spaces) and then prefix with `barPrefix`, which lands the
  // first key against the bar. The args path renders with `pad = '  '.repeat(indent)`,
  // so passing indent=0 produces no leading spaces — exactly what we want
  // since the bar itself is the visual anchor.
  const innerCols = Math.max(20, termCols - visibleWidth(barPrefix));
  const rawLines: string[] = [];

  if (parsed == null || typeof parsed !== 'object') {
    // Scalar — print verbatim with the bar prefix. Fall through to the
    // styled formatter so colors stay consistent with the args path.
    rawLines.push(formatScalar(parsed));
  } else if (Array.isArray(parsed)) {
    // Top-level arrays: emit one row per element, mirroring formatArgLines
    // when it descends into an array. We render inline (no key) so the bar
    // prefix sits where a key would.
    for (const item of parsed) {
      if (item != null && typeof item === 'object' && !Array.isArray(item)) {
        rawLines.push(chalk.dim('-'));
        for (const [k, v] of Object.entries(item)) {
          rawLines.push(
            ...formatArgLines(
              k,
              v,
              1,
              Number.POSITIVE_INFINITY,
              innerCols,
              maxChars
            )
          );
        }
      } else {
        const scalar =
          typeof item === 'string'
            ? clipChars(formatScalar(item), maxChars)
            : formatScalar(item);
        rawLines.push(`${chalk.dim('-')} ${scalar}`);
      }
    }
  } else {
    const obj = parsed as Record<string, unknown>;
    for (const [k, v] of Object.entries(obj)) {
      rawLines.push(
        ...formatArgLines(
          k,
          v,
          0,
          Number.POSITIVE_INFINITY,
          innerCols,
          maxChars
        )
      );
    }
  }

  return rawLines.map((l) => {
    const body = bodyColor ? bodyColor(l) : l;
    return chalk.dim(barPrefix) + body;
  });
}

/**
 * Truncate a list of already-wrapped visual rows to `cap` entries, appending
 * a non-interactive marker built by `marker(n)` that names the dropped count.
 *
 * Append-only by design: this runs once, on the FIRST render of a given tool
 * call. Lite mode commits each rendered string to <Static> so the terminal
 * scrollback owns the bytes — there is no re-render path. If the user later
 * raises the cap, prior tool calls keep the truncated form forever.
 *
 * `cap` of `null` (or `<= 0`) disables truncation. `lines` is treated as a
 * sequence of visual rows: caller is responsible for having already wrapped
 * source lines to terminal width before passing them in.
 *
 * Head-cap form: keeps the FIRST `cap` rows and appends the marker BELOW.
 * Used for input args (the leading keys are usually the most informative —
 * `command`, `path` — so dropping the tail is the right policy). For
 * tool *output*, prefer {@link applyTailLineCap}, which keeps the LAST
 * `cap` rows and prepends the marker ABOVE — matching the live-streaming
 * window where the user watched the output scroll past.
 */
export function applyLineCap(
  lines: string[],
  cap: number | null,
  marker: (dropped: number) => string
): string[] {
  if (cap == null || cap <= 0 || lines.length <= cap) return lines;
  const dropped = lines.length - cap;
  return [...lines.slice(0, cap), marker(dropped)];
}

/**
 * Tail-cap variant of {@link applyLineCap}: keeps the LAST `cap` rows and
 * prepends the marker ABOVE the kept rows. Used for tool output bars so
 * the static rendering matches what the user saw in the live region while
 * the tool was streaming — the window slides forward as new output
 * arrives, eventually settling on the tail.
 *
 * Same `cap` semantics as {@link applyLineCap}: `null` or non-positive
 * disables truncation. Caller is responsible for having pre-wrapped source
 * lines to terminal width so the visual-row count matches what the
 * terminal will actually display.
 */
function applyTailLineCap(
  lines: string[],
  cap: number | null,
  marker: (dropped: number) => string
): string[] {
  if (cap == null || cap <= 0 || lines.length <= cap) return lines;
  const dropped = lines.length - cap;
  return [marker(dropped), ...lines.slice(-cap)];
}

/**
 * Result of unwrapping a tool output envelope. `text` means we surfaced a
 * human-readable string (shell stdout, ACP text, file content); the bar
 * renderer can stream it as-is. `json` means the envelope didn't yield a
 * known string shape, so the caller pretty-prints the parsed object via
 * the same key:value tree used for input args.
 */
type UnwrappedToolOutput =
  | { kind: 'text'; value: string }
  | { kind: 'json'; value: unknown };

/**
 * Pull the user-meaningful payload out of a tool result envelope so the
 * output bar shows stdout / file content / structured fields instead of
 * the protocol's wire JSON.
 *
 * Known string shapes (returned as `{ kind: 'text' }`):
 *   - Shell:  {items:[{Json:{stdout, stderr, exit_status}}]}
 *             Renders stdout, then "(exit N)" for non-zero, then "[stderr] ..."
 *   - Generic items[].Text — surface the inner string(s) verbatim. Multiple
 *     items are concatenated (a tool may emit several content blocks).
 *   - Generic items[].Json with `text` / `content` strings — surface those.
 *   - {content:[{text}]} — ACP-style content blocks, surface concatenated text.
 *
 * Unknown shape: returned as `{ kind: 'json', value }` with the most
 * informative parsed object (the inner Json envelope when present, else the
 * full output) so the caller can pretty-print rather than dump raw JSON.
 */
function unwrapToolOutput(output: unknown): UnwrappedToolOutput {
  if (output == null || typeof output !== 'object') {
    return { kind: 'text', value: safeJson(output, 1_000_000) };
  }
  const obj = output as Record<string, unknown>;

  // ACP {items: [...]} envelope — the canonical wire shape.
  if (Array.isArray(obj.items) && obj.items.length > 0) {
    // Resolve the text payload of a single item, or null if it has no known
    // string shape. Shared by the multi-item and single-item paths below.
    const itemText = (raw: unknown): string | null => {
      if (!raw || typeof raw !== 'object') return null;
      const item = raw as Record<string, unknown>;
      if (typeof item.Text === 'string') return item.Text;
      if (item.Json && typeof item.Json === 'object') {
        const inner = item.Json as Record<string, unknown>;
        const shell = formatShellEnvelope(inner);
        if (shell != null) return shell;
        if (typeof inner.text === 'string') return inner.text;
        if (typeof inner.content === 'string') return inner.content;
      }
      return null;
    };

    // Multi-item: concatenate every item that yields text. A single tool can
    // return several content blocks (text + text, text + structured); reading
    // only items[0] silently dropped the rest.
    if (obj.items.length > 1) {
      const parts: string[] = [];
      for (const it of obj.items) {
        const t = itemText(it);
        if (t != null) parts.push(t);
      }
      if (parts.length > 0) return { kind: 'text', value: parts.join('\n') };
    } else {
      const first = obj.items[0] as Record<string, unknown> | undefined;
      const t = itemText(first);
      if (t != null) return { kind: 'text', value: t };
      // Single Json item with an unknown structured shape — hand the parsed
      // inner up so the caller renders the key:value tree.
      if (
        first &&
        typeof first === 'object' &&
        first.Json &&
        typeof first.Json === 'object'
      ) {
        return { kind: 'json', value: first.Json as Record<string, unknown> };
      }
    }
  }

  // {content: [{text}]} — ACP content-block style.
  if (Array.isArray(obj.content) && obj.content.length > 0) {
    const parts: string[] = [];
    for (const item of obj.content) {
      if (
        item &&
        typeof item === 'object' &&
        'text' in item &&
        typeof (item as Record<string, unknown>).text === 'string'
      ) {
        parts.push((item as Record<string, unknown>).text as string);
      }
    }
    if (parts.length > 0) return { kind: 'text', value: parts.join('\n') };
  }

  // Plain shell-shaped object (no items wrapper).
  const shell = formatShellEnvelope(obj);
  if (shell != null) return { kind: 'text', value: shell };

  // Genuine unknown — surface the parsed object so the caller pretty-prints.
  return { kind: 'json', value: output };
}

/**
 * Convenience wrapper for callers that only care about the text form of a
 * tool's output (e.g. the read-tool path renders the file body as a single
 * string, never as a key:value tree). For JSON envelopes we still produce a
 * compact safeJson string so existing consumers keep working unchanged.
 */
function unwrapToolOutputAsText(output: unknown): string {
  const r = unwrapToolOutput(output);
  if (r.kind === 'text') return r.value;
  return unescapeJsonNewlines(safeJson(r.value, 1_000_000));
}

/**
 * Detect a shell-result-shaped object and format it for the output bar.
 * Returns null when the object doesn't look like a shell result so callers
 * can fall through to the generic JSON path.
 */
function formatShellEnvelope(obj: Record<string, unknown>): string | null {
  const hasStdout = typeof obj.stdout === 'string';
  const hasStderr = typeof obj.stderr === 'string';
  const hasExit = 'exit_status' in obj;
  if (!hasStdout && !hasStderr && !hasExit) return null;

  const parts: string[] = [];
  const stdout = hasStdout ? (obj.stdout as string).replace(/\n+$/, '') : '';
  if (stdout.length > 0) parts.push(stdout);

  // Surface non-zero exits — the user needs to see them.
  if (hasExit) {
    let code: number | null = null;
    const exit = obj.exit_status;
    if (typeof exit === 'number') code = exit;
    else if (typeof exit === 'string') {
      const m = exit.match(/(-?\d+)/);
      if (m && m[1]) code = parseInt(m[1], 10);
    }
    if (code != null && code !== 0) parts.push(`(exit ${code})`);
  }

  if (hasStderr) {
    const stderr = (obj.stderr as string).replace(/\n+$/, '');
    if (stderr.length > 0) parts.push(`[stderr] ${stderr}`);
  }

  return parts.join('\n');
}

/**
 * Replace literal `\n` (and `\r\n`) escape sequences in a JSON-stringified
 * blob with real newlines so unknown envelopes don't render as one
 * unreadable mass when they contain multi-line strings.
 */
function unescapeJsonNewlines(s: string): string {
  return s.replace(/\\r\\n/g, '\n').replace(/\\n/g, '\n');
}

/**
 * Wrap each line of `text` to `availCols` and prefix every visual row with
 * `barPrefix`. The bar glyph (the `│ ` prefix) is styled by `glyphColor` —
 * that's the visual anchor that says "this is a contained output block."
 * The body text is styled by `bodyColor` if provided, otherwise rendered
 * plain (default fg) so users can read it like content rather than chrome.
 *
 * Background: dimming the entire row including the body text was the
 * single biggest source of the "everything is gray" feeling in lite —
 * tool outputs are often the longest block on screen, and turning them
 * full-dim made them read as visual noise even when the user had
 * deliberately enabled output for that tool. The fix is to keep the
 * `│` glyph visually muted (chrome) and let the body render at full
 * luminance (content). Errors are an exception: callers pass
 * `chalk.red` for both args so error output reads as a loud problem.
 *
 * Source newlines are preserved, and lines wider than `availCols` are
 * hard-wrapped so each visual row carries its own `│ ` prefix. Without
 * this hard wrap, long stdout/JSON lines soft-wrap at the terminal and
 * the wrapped portion lands at column 0 with no bar — visually breaking
 * the block's left margin. The earlier "logical lines for clean
 * copy-paste" approach (commit 8e65d9311) preserved long URLs/paths as
 * single clipboard lines, but the misaligned bar made the output read
 * as visually broken. Tool output is more often read than copied
 * verbatim, so we accept the copy-paste tradeoff here in exchange for
 * a consistent left margin.
 *
 * Uses {@link wrapAnsiLine} for ANSI-aware wrapping with SGR carryover
 * across rows — so a body wrapped in `chalk.dim(...)` or
 * `softSuccessOutput(...)` stays correctly styled on every visual row,
 * not just the first.
 */
/**
 * Pre-wrap upper bound on a single source line's length. wrapAnsiLine
 * allocates a `{ansi, ch, width}` cell object per code point in the
 * input; for a single multi-MB line — `grep -E` matching a minified
 * bundle or a binary file, a JSON blob streamed back with no newlines,
 * a shell command emitting one giant base64 payload — that's tens of
 * millions of small objects and several GB of heap, which OOMs the
 * renderer (RangeError: Out of memory). The downstream `applyTailLineCap`
 * that would discard most of the wrap output runs *after* the wrap, so
 * it can't help.
 *
 * 200_000 chars is comfortably above any legitimate single-line output
 * (a wrapped 200KB line at 80 cols still produces ~2500 visual rows —
 * far past every default `outputMaxLines` cap and beyond what a human
 * would scroll through anyway), so this cap never trips on normal data.
 * The bounded cell-array allocation at this size is ~16MB, which leaves
 * comfortable headroom under bun's default heap.
 */
const MAX_INPUT_LINE_CHARS = 200_000;

function formatBarBlock(
  text: string,
  availCols: number,
  barPrefix: string,
  glyphColor: (s: string) => string,
  bodyColor?: (s: string) => string
): string[] {
  const out: string[] = [];
  const sourceLines = text.split('\n');
  // Floor the wrap budget so a misconfigured caller (or a very narrow
  // terminal where the bar prefix already eats most of the row) doesn't
  // produce zero-width chunks and burn a row per character.
  const w = Math.max(8, availCols);
  for (const rawLine of sourceLines) {
    if (rawLine.length === 0) {
      out.push(glyphColor(barPrefix.trimEnd()));
      continue;
    }
    // Pre-clip excessively long source lines before they reach
    // wrapAnsiLine — see MAX_INPUT_LINE_CHARS for the rationale. Keep
    // the TAIL of the line so this composes correctly with the
    // tail-keep semantics of `applyTailLineCap` downstream: the
    // wrapped rows that survive the cap should be the most recent /
    // closest-to-the-end content, matching how the rest of the lite
    // tool-output rendering privileges the tail everywhere. Emit a
    // standalone marker BEFORE the clipped content so a user with an
    // unbounded `outputMaxLines` cap still sees that the line was
    // truncated; users with a tighter cap will get tail-windowed past
    // the marker, which is the same way every other truncation
    // marker in this file behaves.
    let line = rawLine;
    if (rawLine.length > MAX_INPUT_LINE_CHARS) {
      const dropped = rawLine.length - MAX_INPUT_LINE_CHARS;
      out.push(
        `${glyphColor(barPrefix)}${chalk.dim(
          `... (line clipped; +${dropped} chars before)`
        )}`
      );
      line = rawLine.slice(rawLine.length - MAX_INPUT_LINE_CHARS);
    }
    const body = bodyColor ? bodyColor(line) : line;
    const chunks = wrapAnsiLine(body, w, w);
    for (const chunk of chunks) {
      if (chunk.length === 0) {
        out.push(glyphColor(barPrefix.trimEnd()));
      } else {
        out.push(`${glyphColor(barPrefix)}${chunk}`);
      }
    }
  }
  return out;
}

/**
 * Shorten an absolute path for display in tool-call chips. Returned form is
 * always the SAME path — not a different file — just a shorter spelling:
 *
 *   1. Already relative (no leading `/` or `~`) → unchanged.
 *   2. Starts with the process cwd → cwd-relative form ("foo/bar.ts").
 *   3. Starts with the user's home → `~`-substituted ("~/.kiro/agents/x").
 *   4. Otherwise (path outside both cwd and home) → unchanged absolute.
 *
 * Reads `process.cwd()` and `process.env.HOME` defensively — the lite
 * render module is sometimes loaded in non-Node contexts (tests, isolated
 * worker runs) where one or both may be absent. Both are stable per
 * process so the dynamic read doesn't introduce a re-render race.
 *
 * Used by inline arg chips where the path itself isn't the interesting
 * info (the agent's grep pattern, or the verb on a write tool, is) — every
 * extra character of "/Users/<name>/projects/<repo>" eats space the chip
 * could be using to show the actually-informative bit.
 */
function shortenPathForChip(path: string): string {
  if (!path) return path;
  // Already relative — no shortening needed and we don't want to accidentally
  // match a non-anchored prefix like "foo/bar" against cwd.
  if (!path.startsWith('/') && !path.startsWith('~')) return path;
  let cwd: string | undefined;
  try {
    cwd =
      typeof process !== 'undefined' && typeof process.cwd === 'function'
        ? process.cwd()
        : undefined;
  } catch {
    cwd = undefined;
  }
  const home =
    typeof process !== 'undefined' && process.env
      ? process.env.HOME || process.env.USERPROFILE
      : undefined;
  if (cwd && path.startsWith(cwd + '/')) return path.slice(cwd.length + 1);
  if (cwd && path === cwd) return '.';
  if (home && path.startsWith(home + '/')) return '~' + path.slice(home.length);
  if (home && path === home) return '~';
  return path;
}

/**
 * Build the inline arg chip used in `inline` arg mode. Picks the most
 * informative single-line summary the tool's args expose, with two
 * patterns layered on top of the older field-priority scan:
 *
 *   - Search-style tools (grep/glob/symbol search) combine the "what am I
 *     looking for" field with the "where" field. The pattern wins
 *     position; the path is appended after " in " when it adds info.
 *     Example: `grep [wrapAnsiLine in packages/tui/src]`. Older behavior
 *     showed only the path because `path` came BEFORE `pattern` in the
 *     priority scan, which masked the pattern entirely on every grep.
 *
 *   - Write tools surface an action verb plus the path, so the chip
 *     differentiates create/edit/insert/delete at a glance instead of
 *     showing the discriminator (`strReplace`) or just the path.
 *     Example: `fs_write [edit src/lite/render.ts]`.
 *
 * Paths are always run through {@link shortenPathForChip} so an absolute
 * path inside the user's cwd renders as a relative path. Empty/`.` paths
 * are dropped entirely from the combined chip — they add no info.
 */
export function extractInlineArg(
  toolName: string,
  content: string,
  maxChars: number | null = 80
): string | undefined {
  if (!content) return undefined;
  let args: Record<string, unknown> | undefined;
  try {
    const parsed = JSON.parse(content);
    if (parsed && typeof parsed === 'object')
      args = parsed as Record<string, unknown>;
  } catch {
    return undefined;
  }
  if (!args) return undefined;

  // Shell tools: the command is the natural inline summary. Gated on
  // SHELL_TOOL_NAMES so write-tool's `command` discriminator (which holds
  // "create" / "strReplace" / "insert" / "delete", NOT a shell command)
  // doesn't accidentally win this branch and surface a useless chip.
  if (
    SHELL_TOOL_NAMES.has(toolName) &&
    typeof args.command === 'string' &&
    args.command.length > 0
  ) {
    return `[${truncateInline(args.command.split('\n')[0] ?? '', maxChars)}]`;
  }

  // Write tools: verb + relative path. Without the verb, the chip looks
  // identical whether the agent is creating a new file, editing in place,
  // or deleting it — path alone hides the actual operation.
  if (WRITE_TOOL_NAMES.has(toolName)) {
    const path = typeof args.path === 'string' ? args.path : null;
    if (path) {
      let verb = 'write';
      // Wire format is snake_case (see renderWriteToolCall for the full
      // rationale); accept the camelCase variants for non-Rust callers.
      const oldStr = args.old_str ?? args.oldStr;
      const fileText = args.file_text ?? args.content;
      const insertLine = args.insert_line ?? args.insertLine;
      // Prefer the explicit `command` discriminator when the agent set it
      // (the canonical fs_write API). Fall back to inferring from the
      // shape of args for older callers that pass old_str/insert_line
      // without a command field.
      if (args.command === 'create') verb = 'create';
      else if (
        args.command === 'str_replace' ||
        args.command === 'strReplace' ||
        oldStr != null
      )
        verb = 'edit';
      else if (args.command === 'insert' || insertLine != null) verb = 'insert';
      else if (args.command === 'append') verb = 'append';
      else if (args.command === 'delete') verb = 'delete';
      else if (fileText != null && oldStr == null) verb = 'create';
      return `[${truncateInline(`${verb} ${shortenPathForChip(path)}`, maxChars)}]`;
    }
  }

  // Pattern/query tools (grep, glob, web/code search). Combine the
  // "what" field with the "where" field via " in ". Older render order
  // put `path` first in the priority list, so a `grep { pattern, path }`
  // call surfaced just the path — the pattern was the very thing the
  // chip was supposed to show.
  const queryField =
    (typeof args.pattern === 'string' && args.pattern) ||
    (typeof args.query === 'string' && args.query) ||
    (typeof args.search_query === 'string' && args.search_query) ||
    (typeof args.symbol_name === 'string' && args.symbol_name) ||
    null;
  if (queryField) {
    const pathField = typeof args.path === 'string' ? args.path : null;
    if (pathField && pathField !== '.' && pathField !== '') {
      const relPath = shortenPathForChip(pathField);
      // Cwd resolves to "."; in that case the path doesn't add info to
      // the chip (the user already knows their cwd from the footer).
      if (relPath !== '.') {
        return `[${truncateInline(`${queryField} in ${relPath}`, maxChars)}]`;
      }
    }
    return `[${truncateInline(queryField, maxChars)}]`;
  }

  if (typeof args.url === 'string' && args.url.length > 0) {
    return `[${truncateInline(args.url, maxChars)}]`;
  }

  // Read tools may pass an `operations: [{ path }]` array (multi-read API).
  const op = Array.isArray(args.operations)
    ? (args.operations[0] as Record<string, unknown> | undefined)
    : undefined;
  if (op && typeof op.path === 'string')
    return `[${truncateInline(shortenPathForChip(op.path), maxChars)}]`;

  // Path-only tools (read, etc.).
  for (const key of ['path', 'file_path', 'filePath']) {
    const v = args[key];
    if (typeof v === 'string' && v.length > 0)
      return `[${truncateInline(shortenPathForChip(v), maxChars)}]`;
  }

  // Other single-field tools — `name` / `key` are usually short identifiers
  // (knowledge entries, settings keys) so they pass through unshortened.
  for (const key of ['name', 'key']) {
    const v = args[key];
    if (typeof v === 'string' && v.length > 0)
      return `[${truncateInline(v, maxChars)}]`;
  }

  // Last resort — fall back to the purpose extractor so unknown tools still
  // get a chip. We deliberately bracket the result so it reads like an arg
  // chip rather than free-form reasoning.
  const fallback = extractToolPurpose(content);
  return fallback ? `[${truncateInline(fallback, maxChars)}]` : undefined;
}

/** Tail-truncate an inline arg chip at `max` chars. `null` (or non-positive)
 *  means unbounded — return the string unchanged. Honors the user's
 *  argsMaxChars cap so the chip respects /verbosity truncation settings
 *  instead of clipping at a hard-coded constant. */
function truncateInline(s: string, max: number | null = 80): string {
  if (max == null || max <= 0) return s;
  return s.length > max ? s.slice(0, max - 1) + '…' : s;
}

/**
 * Extract just the actual LLM-provided reasoning (`__tool_use_purpose`) from
 * a tool call's content. Returns undefined when the agent didn't supply any.
 *
 * Distinct from {@link extractToolPurpose} which falls back to an args
 * summary when no real reasoning is present. Use this when the args are
 * already being shown elsewhere (e.g. inline-args mode) so we don't end up
 * rendering the args twice — once white-bracketed inline, and once again
 * masquerading as purple "reasoning" via the fallback.
 */
export function extractToolReasoning(
  content: string,
  typedPurpose?: string
): string | undefined {
  // Prefer the typed `purpose` sibling on the ToolUse message — it's
  // captured at the ACP boundary from the model's untouched rawInput, so
  // edit-kind tools (whose `content` is rebuilt by the ToolCall handler
  // and loses `__tool_use_purpose`) still surface their reasoning.
  // Falls back to parsing `content` for tool kinds whose handler keeps
  // the field in the JSON blob.
  if (typeof typedPurpose === 'string' && typedPurpose.trim().length > 0) {
    return typedPurpose;
  }
  if (!content) return undefined;
  try {
    const parsed = JSON.parse(content) as Record<string, unknown>;
    const purpose = parsed.__tool_use_purpose;
    if (typeof purpose === 'string' && purpose.trim().length > 0) {
      return purpose;
    }
  } catch {
    // Non-JSON content — treat as no reasoning.
  }
  return undefined;
}

function extractToolPurpose(content: string): string | undefined {
  if (!content) return undefined;
  try {
    const args = JSON.parse(content);
    // LLM-provided reasoning ("why") wins — this is what shows in purple next
    // to the tool name. Args (the "what") render separately on the lines below
    // via formatToolArgs.
    if (
      args.__tool_use_purpose &&
      typeof args.__tool_use_purpose === 'string'
    ) {
      return args.__tool_use_purpose;
    }
    // No reasoning provided — fall back to the most useful single-line summary
    // we can derive from the args themselves so the line isn't empty. Wire
    // format is snake_case (see renderWriteToolCall); accept camelCase as
    // fallback for non-Rust callers.
    const oldStr = args.old_str ?? args.oldStr;
    if (
      args.command === 'str_replace' ||
      args.command === 'strReplace' ||
      (oldStr && args.path)
    ) {
      return `edit ${args.path}`;
    }
    if (args.command === 'create' && args.path) {
      return `create ${args.path}`;
    }
    if (args.command === 'insert' && args.path) {
      return `insert ${args.path}`;
    }
    if (args.command === 'append' && args.path) {
      return `append ${args.path}`;
    }
    if (args.command === 'delete' || args.command === 'remove') {
      return `delete ${args.path || ''}`;
    }
    if (args.command && typeof args.command === 'string') return args.command;
    if (args.pattern) return args.pattern;
    if (args.query) return args.query;
    if (args.symbol_name) return args.symbol_name;
    if (args.search_query) return args.search_query;
    if (args.key) return args.key;
    if (args.name && typeof args.name === 'string') return args.name;
    if (args.operation)
      return `${args.operation}${args.symbol_name ? ` ${args.symbol_name}` : ''}${args.file_path ? ` ${args.file_path}` : ''}`;
    if (args.operations?.[0]?.path) return args.operations[0].path;
    if (args.file_path) return args.file_path;
    if (args.filePath) return args.filePath;
    if (args.path && args.path !== '.') return args.path;
    for (const [key, val] of Object.entries(args)) {
      if (key.startsWith('_')) continue;
      if (typeof val === 'string' && val.length > 0 && val.length < 200) {
        return val;
      }
    }
  } catch {
    // not JSON
  }
  return undefined;
}

/**
 * Format tool args for scrollback display.
 * Shows key: value pairs, skipping internal fields.
 * Returns null if no meaningful args to show beyond what extractToolPurpose already shows.
 */
export function formatToolArgs(
  toolName: string,
  content: string
): string | null {
  const lines = formatToolArgLines(toolName, content);
  return lines && lines.length > 0 ? lines.join('\n') : null;
}

/**
 * Same as {@link formatToolArgs} but returns the raw lines so callers (e.g. the
 * approval prompt, which renders one Text per line) can wrap them in their own
 * containers.
 *
 * `perValueLineCap` controls the per-string-value multi-line clamp. The
 * block-mode tool renderer sets it to null when the user has chosen
 * "unlimited" for argsMaxLines so the unlimited toggle actually means "no
 * truncation anywhere" (P438130055). All other callers (approval prompt,
 * output-bar JSON pretty-printer) keep the historical 5-line default.
 */
export function formatToolArgLines(
  toolName: string,
  content: string,
  termCols?: number,
  maxChars: number | null = null,
  perValueLineCap: number | null = 5
): string[] | null {
  if (!content) return null;
  try {
    const args = JSON.parse(content);
    if (!args || typeof args !== 'object') return null;

    const cols =
      termCols ??
      (typeof process !== 'undefined' ? process.stdout?.columns : undefined) ??
      120;

    // Special handling for read tool: flatten operations array into readable fields
    if (
      args.operations &&
      Array.isArray(args.operations) &&
      args.operations.length > 0
    ) {
      const lines: string[] = [];
      for (const op of args.operations) {
        if (op.path)
          lines.push(
            ...wrapKeyedLine(
              '  path: ',
              clipChars(String(op.path), maxChars),
              4,
              cols
            )
          );
        if (op.depth != null)
          lines.push(chalk.dim('  depth: ') + String(op.depth));
        if (op.limit != null)
          lines.push(chalk.dim('  lines: ') + String(op.limit));
        if (op.offset != null)
          lines.push(chalk.dim('  offset: ') + String(op.offset));
      }
      return lines.length > 0 ? lines : null;
    }

    const entries = Object.entries(args).filter(([key]) => {
      if (key.startsWith('_')) return false;
      // __tool_use_purpose is the LLM's reasoning; it already renders inline as
      // the description, so don't repeat it here.
      if (key === '__tool_use_purpose') return false;
      return true;
    });
    if (entries.length === 0) return null;
    const lines: string[] = [];
    for (const [key, val] of entries) {
      lines.push(
        ...formatArgLines(key, val, 1, 4, cols, maxChars, perValueLineCap)
      );
    }
    return lines.length > 0 ? lines : null;
  } catch {
    return null;
  }
}

/**
 * Render a single (key, value) pair with nested indentation. `indent` is the
 * number of 2-space levels (the tool name sits at level 0, top-level args at
 * level 1, nested fields at level 2+).
 *
 * String values that exceed the terminal width are wrapped at word boundaries
 * with each continuation line padded to the parent's indent so the visual
 * nesting is preserved. Without this, a long path or query renders past the
 * right edge and continuation glyphs land at column 0, breaking the tree.
 */
function formatArgLines(
  key: string,
  val: unknown,
  indent: number,
  maxDepth = 4,
  termCols = 120,
  maxChars: number | null = null,
  // Cap on the number of source lines a single multi-line string value
  // renders before we collapse the rest behind a `(+N more lines)` marker.
  // `null` lifts the cap entirely — used by the block-mode renderer when
  // the user has set argsMaxLines to "unlimited", so the unlimited toggle
  // actually means "show me everything" instead of secretly clamping each
  // value to 5 lines (P438130055).
  //
  // Default 5 preserves back-compat for the approval prompt and the
  // output-bar pretty-printer, both of which still want a sensible
  // per-value bound regardless of the user's args/output cap settings.
  perValueLineCap: number | null = 5
): string[] {
  const pad = '  '.repeat(indent);
  const dimKey = chalk.dim(`${pad}${key}:`);
  const continuationCols = (indent + 1) * 2;
  if (val == null) {
    return [`${dimKey} ${chalk.dim('null')}`];
  }
  if (typeof val === 'string') {
    // Multi-line values render up to `perValueLineCap` source lines + a
    // delta marker. argsMaxChars (passed as `maxChars`) controls per-line
    // clipping ONLY — it's intentionally unrelated to how many lines we
    // show, so its name and behavior match. Total args-block height is
    // bounded separately by argsMaxLines at the outer applyLineCap site,
    // which can clamp this further if the user wants a tighter footprint.
    //
    // Earlier behavior tied "argsMaxChars set" to "collapse multi-line
    // values to 1 line" which surprised users — they'd set a 120-char
    // value cap (the default) and discover their 50-line shell heredoc
    // suddenly rendered as a single line + "(50 lines)" marker. The two
    // knobs are decoupled here so each does what its name says.
    if (val.includes('\n')) {
      const valLines = val.split('\n');
      // null cap = render every line, no marker. Otherwise show up to
      // `perValueLineCap` and emit a delta marker for the hidden tail.
      const visible =
        perValueLineCap == null
          ? valLines.length
          : Math.min(perValueLineCap, valLines.length);
      // First line attaches to the key on the head row; subsequent lines
      // render at the value's continuation indent. Each line gets the
      // per-line char cap applied before wrap so the cap is visible on
      // every row, not just the first.
      const head = wrapKeyedLine(
        `${pad}${key}: `,
        clipChars(valLines[0] ?? '', maxChars),
        continuationCols,
        termCols
      );
      const tail: string[] = [];
      for (const l of valLines.slice(1, visible)) {
        tail.push(
          ...wrapPlainLine(clipChars(l, maxChars), continuationCols, termCols)
        );
      }
      const out = [...head, ...tail];
      // Delta marker — counts the LINES HIDDEN, mirroring the output bar's
      // "+N more lines above" idiom. Switching from total-count to delta
      // means the args marker and the output marker can sit next to each
      // other without reading as two different things.
      if (valLines.length > visible) {
        const hidden = valLines.length - visible;
        out.push(
          chalk.dim(`${'  '.repeat(indent + 1)}... (+${hidden} more lines)`)
        );
      }
      return out;
    }
    return wrapKeyedLine(
      `${pad}${key}: `,
      clipChars(val, maxChars),
      continuationCols,
      termCols
    );
  }
  if (typeof val === 'number' || typeof val === 'boolean') {
    return [`${dimKey} ${String(val)}`];
  }
  if (Array.isArray(val)) {
    if (val.length === 0) return [`${dimKey} ${chalk.dim('[]')}`];
    if (indent >= maxDepth) {
      return [`${dimKey} ${chalk.dim(safeJson(val, 200))}`];
    }
    const out: string[] = [`${dimKey}`];
    for (let i = 0; i < val.length; i++) {
      const item = val[i];
      if (item != null && typeof item === 'object' && !Array.isArray(item)) {
        // object element — emit "- " marker then nested keys
        const childPad = '  '.repeat(indent + 1);
        out.push(`${childPad}${chalk.dim('-')}`);
        for (const [k, v] of Object.entries(item)) {
          out.push(
            ...formatArgLines(k, v, indent + 2, maxDepth, termCols, maxChars)
          );
        }
      } else {
        const childPad = '  '.repeat(indent + 1);
        const scalar =
          typeof item === 'string'
            ? clipChars(formatScalar(item), maxChars)
            : formatScalar(item);
        out.push(`${childPad}${chalk.dim('-')} ${scalar}`);
      }
    }
    return out;
  }
  if (typeof val === 'object') {
    const obj = val as Record<string, unknown>;
    const entries = Object.entries(obj);
    if (entries.length === 0) return [`${dimKey} ${chalk.dim('{}')}`];
    if (indent >= maxDepth) {
      return [`${dimKey} ${chalk.dim(safeJson(obj, 200))}`];
    }
    const out: string[] = [`${dimKey}`];
    for (const [k, v] of entries) {
      out.push(
        ...formatArgLines(k, v, indent + 1, maxDepth, termCols, maxChars)
      );
    }
    return out;
  }
  return [`${dimKey} ${String(val)}`];
}

function formatScalar(val: unknown): string {
  if (val == null) return chalk.dim('null');
  if (typeof val === 'string') {
    return val.includes('\n') ? val.split('\n')[0] + chalk.dim(' …') : val;
  }
  return String(val);
}

function safeJson(val: unknown, max: number): string {
  try {
    const s = JSON.stringify(val);
    return s.length > max ? s.slice(0, max - 1) + '…' : s;
  } catch {
    return String(val);
  }
}

interface TaskInputArg {
  task_description?: unknown;
  details?: unknown;
}

/**
 * Format the body of the built-in task list tool (wire name `todo_list` /
 * `task` / `todo`). The agent crate owns the exact schema (see
 * `crates/agent/src/agent/tools/task/task_tool.rs`), so this renderer
 * relies on the discriminator + per-command field names being stable. If
 * a future schema change breaks parsing, the function falls through to
 * `null` and {@link renderMessageToText} bakes the generic JSON pretty-
 * printer instead — same fallback shape used everywhere else in lite.
 *
 * Rendering policy per command:
 *   - `create` / `add` — list the new tasks numbered with tree connectors
 *     (mirrors what {@link LiteTaskTray} shows for the same data when
 *     the user expands the activity tray) plus the optional description /
 *     new_description as a labeled block above. Multi-line task subjects
 *     wrap at the available column budget with continuation lines aligned
 *     under the first character of the subject.
 *   - `complete` — show the completed task ids inline (`completed: #1, #2`)
 *     plus the agent's `context_update` notes and any `modified_files`
 *     dump. The IDs themselves are cyan so they read as link-y references
 *     to the tray rows.
 *   - `remove` — show the removed ids with the same chip styling as
 *     `complete`, plus the optional new_description block.
 *   - `list` — no body; the bare `tasks list` header is enough.
 *
 * Returns the parsed `command` (so the caller can use it as the tool's
 * inline-arg chip) and the body lines. The output bar is suppressed at
 * the call site because the {@link LiteTaskTray} already surfaces the
 * authoritative tasks state — the raw tool result is a JSON dump of every
 * task, which would just duplicate the tray.
 */
export function formatTaskToolBody(
  content: string,
  termCols?: number,
  glyphsArg?: Glyphs
): { command: string; bodyLines: string[] } | null {
  if (!content) return null;
  let args: Record<string, unknown>;
  try {
    args = JSON.parse(content) as Record<string, unknown>;
  } catch {
    return null;
  }
  if (!args || typeof args !== 'object') return null;
  const command = args.command;
  if (typeof command !== 'string') return null;
  if (!['create', 'complete', 'add', 'remove', 'list'].includes(command)) {
    return null;
  }

  const cols = Math.max(
    40,
    termCols ??
      (typeof process !== 'undefined' ? process.stdout?.columns : undefined) ??
      120
  );
  const g = resolveGlyphs(glyphsArg);
  const indent = '  ';

  // Render a numbered list of tasks with tree-connector glyphs on the left
  // (treeBranch for every row except the last, where it's treeCorner). The
  // visual is intentionally close to {@link LiteTaskTray}'s expanded view so
  // when the user sees a `create` call commit to scrollback, the same shape
  // is mirrored in the tray above the input — same data, two surfaces, one
  // mental model. ASCII mode degrades the connectors via {@link resolveGlyphs}.
  //
  // Each task subject wraps to the available column budget. Continuation
  // rows align under the first character of the subject (i.e. past the
  // connector + index prefix) so a long subject doesn't crash to col 0.
  // Optional `details` render dim under the subject at the same indent.
  const renderTaskList = (tasks: TaskInputArg[]): string[] => {
    const out: string[] = [];
    if (tasks.length === 0) return out;
    // Visible-col width of the prefix on each line:
    //   '  ' (2) + connector (3) + ' ' (1) + index padded (idxWidth + 1)
    // We pad the index to the width of the largest one so a 10-task list
    // doesn't shift "Task 10" rightward relative to "Task 1".
    const idxWidth = `${tasks.length}.`.length;
    const prefixCols = 2 + 3 + 1 + idxWidth + 1;
    const subjectAvail = Math.max(20, cols - prefixCols);
    const continuationIndent = ' '.repeat(prefixCols);
    for (let i = 0; i < tasks.length; i++) {
      const t = tasks[i] ?? {};
      const subject =
        typeof t.task_description === 'string' ? t.task_description.trim() : '';
      if (!subject) continue;
      const isLast = i === tasks.length - 1;
      const connector = isLast ? g.treeCorner : g.treeBranch;
      const num = `${i + 1}.`.padEnd(idxWidth + 1);
      const wrapped = wrapAtWords(subject, subjectAvail, subjectAvail);
      out.push(
        `${indent}${chalk.dim(connector)} ${chalk.dim(num)}${wrapped[0] ?? ''}`
      );
      for (const line of wrapped.slice(1)) {
        out.push(`${continuationIndent}${line}`);
      }
      const details = typeof t.details === 'string' ? t.details.trim() : '';
      if (details) {
        const wrappedDetails = wrapAtWords(details, subjectAvail, subjectAvail);
        for (const line of wrappedDetails) {
          out.push(`${continuationIndent}${chalk.dim(line)}`);
        }
      }
    }
    return out;
  };

  // Wrap a multi-line description block under a dim `description:` label
  // at column 4 (one indent level below the body's 2-space pad). Used by
  // `create`'s task_list_description and `add`/`remove`'s new_description
  // — same shape, three call sites.
  const renderDescriptionBlock = (label: string, text: string): string[] => {
    const out: string[] = [`${indent}${chalk.dim(`${label}:`)}`];
    const avail = Math.max(20, cols - 4);
    const wrapped = wrapAtWords(text, avail, avail);
    for (const line of wrapped) out.push(`    ${line}`);
    return out;
  };

  const stringList = (val: unknown): string[] =>
    Array.isArray(val)
      ? val.filter((x): x is string => typeof x === 'string')
      : [];

  const lines: string[] = [];

  if (command === 'create' || command === 'add') {
    const description =
      command === 'create'
        ? typeof args.task_list_description === 'string'
          ? args.task_list_description.trim()
          : ''
        : typeof args.new_description === 'string'
          ? args.new_description.trim()
          : '';
    if (description) {
      lines.push(...renderDescriptionBlock('description', description));
    }
    const taskKey = command === 'create' ? 'tasks' : 'new_tasks';
    const tasks = Array.isArray(args[taskKey])
      ? (args[taskKey] as TaskInputArg[])
      : [];
    if (tasks.length > 0) {
      lines.push(...renderTaskList(tasks));
    }
    return { command, bodyLines: lines };
  }

  if (command === 'complete') {
    const ids = stringList(args.completed_task_ids);
    if (ids.length > 0) {
      const idChips = ids
        .map((id) => chalk.cyan(`#${id}`))
        .join(chalk.dim(', '));
      lines.push(`${indent}${chalk.dim('completed:')} ${idChips}`);
    }
    const ctxUpdate =
      typeof args.context_update === 'string' ? args.context_update.trim() : '';
    if (ctxUpdate) {
      lines.push(`${indent}${chalk.dim('notes:')}`);
      const avail = Math.max(20, cols - 4);
      const wrapped = wrapAtWords(ctxUpdate, avail, avail);
      for (const line of wrapped) lines.push(`    ${chalk.dim(line)}`);
    }
    const files = stringList(args.modified_files);
    if (files.length > 0) {
      lines.push(`${indent}${chalk.dim('files:')}`);
      for (const f of files) lines.push(`    ${chalk.dim('-')} ${f}`);
    }
    return { command, bodyLines: lines };
  }

  if (command === 'remove') {
    const ids = stringList(args.remove_task_ids);
    if (ids.length > 0) {
      const idChips = ids
        .map((id) => chalk.cyan(`#${id}`))
        .join(chalk.dim(', '));
      lines.push(`${indent}${chalk.dim('removed:')} ${idChips}`);
    }
    const newDesc =
      typeof args.new_description === 'string'
        ? args.new_description.trim()
        : '';
    if (newDesc) {
      lines.push(...renderDescriptionBlock('description', newDesc));
    }
    return { command, bodyLines: lines };
  }

  // command === 'list' — no args, no body. The bare `tasks list` header
  // line carries all the meaning the call has.
  return { command, bodyLines: [] };
}

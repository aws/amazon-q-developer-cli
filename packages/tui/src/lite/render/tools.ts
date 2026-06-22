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
  /** Reasoning ("why"), brand (purple). With an inline arg it renders on its
   *  own line(s) below the name so what (white args) and why (purple) don't
   *  share a slot; without one, the first line sits inline (legacy). */
  description?: string;
  /** Inline arg chip in default color: `tool [args]`. Independent of
   *  description so both can show under inline-args + reasoning. */
  inlineArg?: string;
  mcpServer?: string;
  agentPrefix?: string;
  elapsed?: number;
  status: 'running' | 'done' | 'error' | 'cancelled';
  isTrivial?: boolean;
  rejected?: boolean;
  /** STATUS-SLOT CONTRACT (running status only): awaitingApproval wins and
   *  paints a yellow ' ...' (the agent isn't progressing while approval is
   *  pending, so a spinner would lie; matches the prompt's hotkey color).
   *  Else a non-trivial tool with runningSpinner shows the spinner glyph for
   *  motion; trivial tools (read/grep/glob) and the static path keep ' ...'. */
  runningSpinner?: string;
  /** See STATUS-SLOT CONTRACT — takes precedence over runningSpinner. */
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
  // Uncolored chip (default fg) reads as literal "what was passed"; brand
  // color is reserved for reasoning so what-vs-why is distinguishable.
  const argChip = info.inlineArg ? ` ${info.inlineArg}` : '';

  let statusStr: string;
  switch (info.status) {
    case 'running':
      // See STATUS-SLOT CONTRACT on ToolCallRenderInfo.
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

  // Reasoning layout (see ToolCallRenderInfo.description): with an inline arg
  // it goes on its own line(s) below; without one, the first line is inline.
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
 * Render a write/edit tool call with a unified diff. Used by the chat-log
 * finalizer and the approval prompt. `suppressDiff` returns the bare header.
 *
 * WIRE FORMAT (referenced throughout this file): fs_write args are snake_case
 * (Rust serde, crates/.../tools/fs_write.rs): command `str_replace`/`create`/
 * `insert`/`append`, fields `old_str`/`new_str`/`file_text`/`insert_line`.
 * camelCase variants are accepted as a fallback for non-Rust callers.
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
    // snake_case + camelCase fallback — see WIRE FORMAT above.
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
      oldText = '';
      newText = String(newStr ?? fileText ?? '');
      if (typeof insertLine === 'number') startLine = insertLine + 1;
    } else if (args.command === 'append') {
      // Pure-add block (no baseline read for trailing context), like create.
      oldText = '';
      newText = String(newStr ?? fileText ?? '');
    } else if (
      args.command === 'create' ||
      (!oldStr && (fileText != null || newStr != null))
    ) {
      oldText = '';
      newText = String(fileText ?? newStr ?? '');
    }
  } catch {
    return renderToolCall(info, opts.theme);
  }

  const out: string[] = [renderToolCall(info, opts.theme)];
  // suppressPathHeader: the inline arg chip already shows the path; `path` is
  // still passed for syntax-highlight language detection.
  const diff = renderUnifiedDiff(oldText, newText, {
    path,
    suppressPathHeader: true,
    startLine,
    termCols: opts.termCols,
    theme: opts.theme,
  });
  // Diffs render in full (the payload being reviewed; no safe tail to drop).
  if (diff.length > 0) {
    out.push(...diff);
  }
  return out.join('\n');
}

/**
 * Render a read-style tool call (fs_read et al.): header + path line + numbered,
 * highlighted body (no gutter/bg — this is inspection, not a change). Capped to
 * maxLines visual rows via the same outputMaxLines knob the bar formatter uses.
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
    // `operations` is the multi-read shape; fall back to top-level path.
    const op = Array.isArray(args.operations) ? args.operations[0] : null;
    path = op?.path ?? args.path ?? args.file_path ?? args.filePath;
    if (op && typeof op.offset === 'number') startLine = op.offset + 1;
    else if (typeof args.offset === 'number') startLine = args.offset + 1;
  } catch {
    // fall through; the path header will simply be skipped.
  }

  const out: string[] = [renderToolCall(info, opts.theme)];

  // Errors take the loud red bar path (red glyph AND body) so they stay visible.
  if (result?.status === 'error' && result.error) {
    const indent = '    ';
    const barPrefix = `${indent}${g.lineVertical} `;
    const avail = Math.max(20, cols - visibleWidth(barPrefix));
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

  // Number column mirrors the diff renderer's so reads/writes line up.
  const LINE_NUM_WIDTH = 4;
  const linePrefixCols = 2 + LINE_NUM_WIDTH + 1;
  const codeCols = Math.max(20, cols - linePrefixCols);
  const sourceLines = text.replace(/\n+$/, '').split('\n');

  // Wrap each highlighted line; continuation rows blank the number column.
  const visualRows: string[] = [];
  for (let i = 0; i < sourceLines.length; i++) {
    const line = sourceLines[i] ?? '';
    const numStr = String(startLine + i).padStart(LINE_NUM_WIDTH);
    const dimNum = chalk.dim(`  ${numStr} `);
    const blankNum = chalk.dim('  ' + ' '.repeat(LINE_NUM_WIDTH) + ' ');
    const styled = highlightLineSafe(line, language);
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

  // Per-row char clip honoring maxCharsPerLine (visible-width aware).
  const clipped =
    opts.maxCharsPerLine && opts.maxCharsPerLine > 0
      ? visualRows.map((r) => clipVisibleWidth(r, opts.maxCharsPerLine!))
      : visualRows;

  // Tail-window at maxLines (keep the LAST N, marker above) — matches the
  // tail the user saw scroll past in the live region.
  const capped = applyTailLineCap(clipped, opts.maxLines ?? null, (n) =>
    chalk.dim(`  ${' '.repeat(LINE_NUM_WIDTH)} ... (+${n} more lines above)`)
  );

  // Footer line count so truncated reads still name the file's true length.
  out.push(...capped);
  const lineCount = sourceLines.length;
  out.push(chalk.dim(`  ${lineCount} ${lineCount === 1 ? 'line' : 'lines'}`));
  return out.join('\n');
}

export function formatElapsed(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

/** "Trivial" read-style tools — rendered dimmed. */
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
 * Render the tool's output as a `│ `-bar block at column 4 (so a call reads
 * name → args → response). Gated by the /verbose filter list; errors always
 * surface (red bar) regardless of the filter.
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
  // Errors always surface; the filter only gates successful output.
  const isError = result.status === 'error';
  if (!isError && !shouldShowToolOutput(toolName, filtersOverride)) return '';
  // termCols is threaded from RenderContext so all renderers agree on width
  // (and resizes don't re-flow already-flushed rows differently).
  const cols = Math.max(40, termCols ?? 120);
  const indent = '    ';
  const g = resolveGlyphs(glyphs);
  const barPrefix = `${indent}${g.lineVertical} `;
  const barCols = visibleWidth(barPrefix);
  const avail = Math.max(20, cols - barCols);

  // Dim "output:" header so the args block and the bar don't merge visually.
  // Empty-output paths early-return before this, so no orphan label.
  const outputHeader = chalk.dim('  output:');

  // Per-row visible-width clip at maxCharsPerLine (both error + normal paths).
  const clipRow = (s: string): string => {
    if (maxCharsPerLine == null || maxCharsPerLine <= 0) return s;
    return clipVisibleWidth(s, maxCharsPerLine);
  };

  if (result.status === 'error') {
    // Prefer the explicit error field; fall back to the output envelope so
    // failures that put their reason in the body still surface.
    let errText = result.error ?? '';
    if (!errText && result.output != null) {
      errText =
        typeof result.output === 'string'
          ? result.output
          : unwrapToolOutputAsText(result.output);
    }
    const trimmed = errText.trim();
    if (trimmed.length === 0) return '';
    const lines = formatBarBlock(
      errText,
      avail,
      barPrefix,
      chalk.red,
      chalk.red
    );
    if (lines.length === 0) return '';
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

  // Structured envelope → key:value tree (same green success tint as text
  // outputs so the success/error signal is consistent across both branches).
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
  // Dim glyph (chrome) + sage-green body (content); red+red on errors above.
  const lines = formatBarBlock(
    unwrapped.value,
    avail,
    barPrefix,
    chalk.dim,
    softSuccessOutput
  );
  if (lines.length === 0) return '';
  // Tail-window matching what the user last saw stream in the live region.
  const capped = applyTailLineCap(lines, maxLines ?? null, (n) =>
    chalk.dim(`${barPrefix}... (truncated; +${n} more lines above)`)
  );
  return '\n' + outputHeader + '\n' + capped.map(clipRow).join('\n');
}

/**
 * Render an in-flight tool's output as a `│`-bar block, tail-windowed to
 * outputMaxLines (marker above) so the live preview matches the eventual
 * static rendering. Respects shouldShowToolOutput; returns [] when there's
 * nothing to show. (Only fires when the store's liveOutputs map is fed by
 * per-chunk ToolCallUpdate text, which some backends don't emit for shell —
 * the static finalizer uses the same cap, so scrollback stays consistent.)
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
  const indent = '    ';
  const g = resolveGlyphs(opts.glyphs);
  const barPrefix = `${indent}${g.lineVertical} `;
  const barCols = visibleWidth(barPrefix);
  const avail = Math.max(20, cols - barCols);

  // Skip whitespace-only buffers so the region doesn't flash empty `│` rows.
  const joined = sourceLines.join('\n');
  if (!joined.trim()) return [];

  // Dim glyph + green body, same split as the static finalizer so the live
  // preview and eventual scrollback row match.
  const lines = formatBarBlock(
    joined,
    avail,
    barPrefix,
    chalk.dim,
    softSuccessOutput
  );
  if (lines.length === 0) return [];

  const clipped =
    opts.outputMaxChars != null && opts.outputMaxChars > 0
      ? lines.map((l) => clipVisibleWidth(l, opts.outputMaxChars!))
      : lines;

  // Tail-window in visual rows; "streaming" distinguishes it from the
  // finalized "truncated" marker.
  const tailCapped = applyTailLineCap(
    clipped,
    opts.outputMaxLines ?? null,
    (n) => chalk.dim(`${barPrefix}... (streaming; +${n} more lines above)`)
  );
  return [chalk.dim('  output:'), ...tailCapped];
}

/**
 * Pretty-print parsed JSON as `│ `-prefixed key:value rows, reusing
 * {@link formatArgLines} so styling matches the args block. No depth cap (the
 * user controls footprint via outputMaxLines). Optional `bodyColor` tints each
 * row (chalk's dim-on/off keeps dim keys intact under the tint).
 */
function formatJsonAsBarLines(
  parsed: unknown,
  barPrefix: string,
  termCols: number,
  maxChars: number | null,
  bodyColor?: (s: string) => string
): string[] {
  // Render at indent=0 (bar prefix is the visual anchor).
  const innerCols = Math.max(20, termCols - visibleWidth(barPrefix));
  const rawLines: string[] = [];

  if (parsed == null || typeof parsed !== 'object') {
    rawLines.push(formatScalar(parsed));
  } else if (Array.isArray(parsed)) {
    // One row per element (no key — the bar sits where a key would).
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
 * Head-cap: keep the FIRST `cap` rows, append a marker below. Used for input
 * args (leading keys like `command`/`path` are the most informative). For
 * output use {@link applyTailLineCap}. `cap` null/<=0 disables; caller must
 * pre-wrap `lines` to visual rows.
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
 * Tail-cap: keep the LAST `cap` rows, prepend the marker above. Used for output
 * bars so static rendering matches the tail the user saw stream past. Same
 * semantics as {@link applyLineCap}.
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

/** `text` = a human-readable string the bar can stream as-is; `json` = an
 *  unknown shape the caller pretty-prints as a key:value tree. */
type UnwrappedToolOutput =
  | { kind: 'text'; value: string }
  | { kind: 'json'; value: unknown };

/**
 * Pull the meaningful payload out of a tool result envelope. Known text shapes:
 * shell {items:[{Json:{stdout,stderr,exit_status}}]}, items[].Text,
 * items[].Json.{text,content}, {content:[{text}]}. Unknown → {kind:'json'}
 * with the most informative parsed object.
 */
function unwrapToolOutput(output: unknown): UnwrappedToolOutput {
  if (output == null || typeof output !== 'object') {
    return { kind: 'text', value: safeJson(output, 1_000_000) };
  }
  const obj = output as Record<string, unknown>;

  // ACP {items: [...]} envelope — the canonical wire shape.
  if (Array.isArray(obj.items) && obj.items.length > 0) {
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

    // Multi-item: concatenate every item that yields text (reading only
    // items[0] silently dropped the rest).
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
      // Single Json item, unknown shape — hand the inner up for the tree.
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

/** Text-only form of unwrapToolOutput; JSON envelopes become a compact
 *  safeJson string (used by the read-tool path). */
function unwrapToolOutputAsText(output: unknown): string {
  const r = unwrapToolOutput(output);
  if (r.kind === 'text') return r.value;
  return unescapeJsonNewlines(safeJson(r.value, 1_000_000));
}

/** Format a shell-result-shaped object (stdout, then "(exit N)", then
 *  "[stderr] ..."); null when it isn't shell-shaped. */
function formatShellEnvelope(obj: Record<string, unknown>): string | null {
  const hasStdout = typeof obj.stdout === 'string';
  const hasStderr = typeof obj.stderr === 'string';
  const hasExit = 'exit_status' in obj;
  if (!hasStdout && !hasStderr && !hasExit) return null;

  const parts: string[] = [];
  const stdout = hasStdout ? (obj.stdout as string).replace(/\n+$/, '') : '';
  if (stdout.length > 0) parts.push(stdout);

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
 * Hard-wrap each line of `text` to `availCols`, prefixing every visual row
 * with `barPrefix` (styled by `glyphColor`; body by `bodyColor` or plain).
 * Hard wrap (not terminal soft-wrap) so wrapped rows keep the `│` margin
 * instead of crashing to col 0 — accepts the copy-paste tradeoff since tool
 * output is read more than copied. Uses {@link wrapAnsiLine} (SGR carryover).
 */
/**
 * Pre-wrap cap on a single source line. wrapAnsiLine allocates a cell object
 * per code point, so one multi-MB line (minified bundle, no-newline JSON blob,
 * giant base64) is tens of millions of objects and OOMs the renderer — and the
 * downstream cap runs AFTER the wrap, too late. 200K is far above any
 * legitimate single line (~2500 rows at 80 cols, ~16MB cells).
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
  // Floor the wrap budget so a narrow terminal doesn't burn a row per char.
  const w = Math.max(8, availCols);
  for (const rawLine of sourceLines) {
    if (rawLine.length === 0) {
      out.push(glyphColor(barPrefix.trimEnd()));
      continue;
    }
    // Pre-clip very long lines (see MAX_INPUT_LINE_CHARS), keeping the TAIL to
    // match downstream applyTailLineCap; marker before the clipped content.
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
 * Shorten an absolute path for chips to the same file, shorter spelling:
 * relative → unchanged; under cwd → cwd-relative; under home → `~`-form;
 * else unchanged. Reads cwd/HOME defensively (may be absent in test workers;
 * both are per-process stable so no re-render race).
 */
function shortenPathForChip(path: string): string {
  if (!path) return path;
  // Already relative — don't accidentally match an unanchored prefix vs cwd.
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
 * Build the inline arg chip: the most informative single-line summary of the
 * tool's args. Search tools combine "what" + " in " + "where"; write tools
 * surface a verb + path (so create/edit/insert/delete differ at a glance).
 * Paths go through {@link shortenPathForChip}; empty/`.` paths are dropped.
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

  // Shell tools: the command is the chip. Gated on SHELL_TOOL_NAMES so the
  // write-tool `command` discriminator doesn't win and show a useless chip.
  if (
    SHELL_TOOL_NAMES.has(toolName) &&
    typeof args.command === 'string' &&
    args.command.length > 0
  ) {
    return `[${truncateInline(args.command.split('\n')[0] ?? '', maxChars)}]`;
  }

  // Write tools: verb + relative path (path alone hides the operation).
  if (WRITE_TOOL_NAMES.has(toolName)) {
    const path = typeof args.path === 'string' ? args.path : null;
    if (path) {
      let verb = 'write';
      // snake_case + camelCase fallback (see WIRE FORMAT).
      const oldStr = args.old_str ?? args.oldStr;
      const fileText = args.file_text ?? args.content;
      const insertLine = args.insert_line ?? args.insertLine;
      // Prefer the explicit `command`; infer from shape for older callers.
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

  // Pattern/query tools (grep, glob, search): combine "what" + " in " + path.
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
      // cwd resolves to "." — adds no info (footer already shows cwd).
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

  // `name` / `key` are short identifiers — pass through unshortened.
  for (const key of ['name', 'key']) {
    const v = args[key];
    if (typeof v === 'string' && v.length > 0)
      return `[${truncateInline(v, maxChars)}]`;
  }

  // Last resort: the purpose extractor, bracketed so it reads as a chip.
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
 * Extract only the real LLM reasoning (`__tool_use_purpose`); undefined when
 * absent. Unlike {@link extractToolPurpose} it never falls back to an args
 * summary — used in inline-args mode so args aren't shown twice (once as a
 * chip, once masquerading as purple reasoning).
 */
export function extractToolReasoning(
  content: string,
  typedPurpose?: string
): string | undefined {
  // Prefer the typed `purpose` sibling (captured at the ACP boundary from
  // untouched rawInput) so edit-kind tools, whose rebuilt content loses
  // __tool_use_purpose, still surface reasoning; else parse content.
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
    if (
      args.__tool_use_purpose &&
      typeof args.__tool_use_purpose === 'string'
    ) {
      return args.__tool_use_purpose;
    }
    // No reasoning — derive a single-line summary from args (snake_case +
    // camelCase fallback, see WIRE FORMAT).
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

/** key: value pairs for scrollback (skips internal fields); null when empty. */
export function formatToolArgs(
  toolName: string,
  content: string
): string | null {
  const lines = formatToolArgLines(toolName, content);
  return lines && lines.length > 0 ? lines.join('\n') : null;
}

/**
 * Like {@link formatToolArgs} but returns raw lines (so callers can wrap each
 * in their own container). `perValueLineCap` is the per-value multi-line clamp;
 * the block-mode renderer passes null when argsMaxLines is "unlimited" so that
 * toggle means no truncation anywhere (P438130055). Others keep the 5-line default.
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

    // Read tool: flatten the operations array into readable fields.
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
      // __tool_use_purpose already renders inline as the description.
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
 * Render a (key, value) pair with nested indentation (`indent` = 2-space
 * levels). Long string values wrap at word boundaries, padded to the parent
 * indent so the tree doesn't crash to col 0.
 */
function formatArgLines(
  key: string,
  val: unknown,
  indent: number,
  maxDepth = 4,
  termCols = 120,
  maxChars: number | null = null,
  // Per-value multi-line cap; null lifts it (block mode under "unlimited",
  // P438130055). Default 5 keeps a sensible bound for other callers.
  perValueLineCap: number | null = 5
): string[] {
  const pad = '  '.repeat(indent);
  const dimKey = chalk.dim(`${pad}${key}:`);
  const continuationCols = (indent + 1) * 2;
  if (val == null) {
    return [`${dimKey} ${chalk.dim('null')}`];
  }
  if (typeof val === 'string') {
    // Multi-line values: up to perValueLineCap source lines + a delta marker.
    // maxChars (argsMaxChars) clips per-line only — decoupled from line count
    // so each knob does what its name says; total height is bounded by
    // argsMaxLines at the outer applyLineCap.
    if (val.includes('\n')) {
      const valLines = val.split('\n');
      const visible =
        perValueLineCap == null
          ? valLines.length
          : Math.min(perValueLineCap, valLines.length);
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
      // Delta marker (lines hidden), matching the output bar's idiom.
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
 * Format the built-in task list tool body (wire name `todo_list`/`task`/`todo`).
 * Relies on the agent-crate schema being stable; malformed args → null (caller
 * falls back to the generic JSON printer). Per command: create/add → numbered
 * tree of tasks + optional description block; complete → cyan id chips + notes
 * + files; remove → removed id chips + new_description; list → no body.
 * Returns the parsed command (for the inline chip) and body lines.
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

  // Numbered task list with tree connectors (treeCorner on the last row),
  // mirroring LiteTaskTray's expanded view; subjects wrap with continuation
  // rows aligned under the subject, optional dim `details` below.
  const renderTaskList = (tasks: TaskInputArg[]): string[] => {
    const out: string[] = [];
    if (tasks.length === 0) return out;
    // Pad the index to the widest so "Task 10" doesn't shift vs "Task 1".
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

  // Multi-line description block under a dim `label:` at column 4.
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

  // command === 'list' — the bare header carries all the meaning.
  return { command, bodyLines: [] };
}

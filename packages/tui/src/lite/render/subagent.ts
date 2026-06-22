import chalk from 'chalk';
import { visibleWidth } from '../../utils/text-width.js';
import {
  getVerboseDisplay,
  shouldShowToolOutput,
  type VerboseDisplayConfig,
} from '../verbose.js';
import type { Glyphs } from '../../utils/glyphs.js';
import { resolveGlyphs, responseChip } from './theme.js';
import { wrapAnsiLine, wrapAtWords } from './text.js';
import { renderMarkdownToLines } from './markdown.js';
import { formatElapsed, type ToolCallRenderInfo } from './tools.js';
import type { SubagentStageSummary } from './message.js';

export function extractSubagentOutput(result?: {
  status: string;
  error?: string;
  output?: unknown;
}): string | null {
  if (!result) return null;
  if (result.status === 'error') {
    if (result.error) return result.error;
    if (typeof result.output === 'string') return result.output;
    if (result.output != null) {
      try {
        return JSON.stringify(result.output, null, 2);
      } catch {
        return null;
      }
    }
    return null;
  }
  if (!result.output) return null;
  if (typeof result.output === 'string') return result.output;
  try {
    return JSON.stringify(result.output, null, 2);
  } catch {
    return null;
  }
}

interface SubagentStage {
  name?: string;
  role?: string;
  prompt_template?: string;
  depends_on?: string[];
}

/**
 * Render a stage's `prompt_template` as styled markdown for both the approval
 * prompt and the final block. Unlike agent prose (which stays unwrapped at
 * col 0), stage prompts live under a tree-stem indent, so each markdown line
 * is re-wrapped through {@link wrapAnsiLine} (which preserves SGR closers
 * across wrap boundaries so styles don't bleed) and re-indented per visual row.
 */
export function renderStagePromptLines(
  prompt: string,
  avail: number,
  indent: string,
  glyphs?: Glyphs
): string[] {
  if (!prompt) return [];
  const out: string[] = [];
  for (const md of renderMarkdownToLines(prompt, avail, avail, glyphs)) {
    if (md.length === 0) {
      out.push(''); // preserve paragraph separators
      continue;
    }
    for (const visual of wrapAnsiLine(md, avail, avail)) {
      if (visual.trim().length === 0) continue; // skip orphan-space rows
      out.push(`${indent}${visual}`);
    }
  }
  return out;
}

/**
 * Per-stage pipeline tree (branch/stem glyphs, `[name]` chip, role/deps chips,
 * {task}-substituted prompt) shared by the approval prompt and the final block.
 * Approval always shows role/deps/prompts; the final block gates each via `sub.*`.
 */
function renderPipelineStages(
  stages: SubagentStage[],
  task: string | null | undefined,
  opts: {
    inputColor: (name: string) => (text: string) => string;
    cols: number;
    glyphs?: Glyphs;
    showRoles: boolean;
    showDeps: boolean;
    showPrompts: boolean;
  }
): string[] {
  const out: string[] = [];
  const g = resolveGlyphs(opts.glyphs);
  out.push(chalk.dim('  pipeline:'));
  for (let i = 0; i < stages.length; i++) {
    const stage = stages[i] ?? {};
    const isLast = i === stages.length - 1;
    const branch = isLast
      ? `${g.cornerBottomLeft}${g.lineHorizontal}`
      : `${g.teeRight}${g.lineHorizontal}`;
    const stem = isLast ? '  ' : `${g.lineVertical} `;
    const name = stage.name || `stage-${i + 1}`;
    const role =
      opts.showRoles && stage.role ? chalk.dim(` (${stage.role})`) : '';
    const deps =
      opts.showDeps &&
      Array.isArray(stage.depends_on) &&
      stage.depends_on.length > 0
        ? chalk.dim(` ← ${stage.depends_on.join(', ')}`)
        : '';
    out.push(
      `    ${chalk.dim(branch)} ${opts.inputColor(name)(`[${name}]`)}${role}${deps}`
    );
    // {task} substituted on render so the display matches what the spawned
    // subagent receives (older binaries ship the raw template; backend also subs).
    const rawPrompt = stage.prompt_template;
    const prompt =
      rawPrompt && task ? rawPrompt.replace(/\{task\}/g, task) : rawPrompt;
    if (
      opts.showPrompts &&
      prompt &&
      typeof prompt === 'string' &&
      prompt.length > 0
    ) {
      const promptIndent = `    ${chalk.dim(stem)} `;
      // 7 = width of "    │ " + 1-col safety margin (stdout.columns can be off
      // by one, otherwise causing stray col-0 soft-wraps).
      const avail = Math.max(20, opts.cols - 7);
      out.push(
        ...renderStagePromptLines(prompt, avail, promptIndent, opts.glyphs)
      );
    }
  }
  return out;
}

/**
 * "Chip at col 7 + markdown body at col 9" digest section (shared by `full
 * output:` and `response summary:`), pre-wrapped via wrapAnsiLine (SGR carryover)
 * so continuations don't crash to col 0. `truncatedBy` (summary only) appends a
 * "(+N more lines)" row.
 */
function renderDigestSection(
  header: string,
  entries: { stageName: string; body: string; truncatedBy?: number }[],
  opts: { chipFn: (stageName: string) => string; cols: number; glyphs?: Glyphs }
): string[] {
  const out: string[] = [];
  const chipIndent = '       ';
  const bodyIndent = '         ';
  const avail = Math.max(20, opts.cols - visibleWidth(bodyIndent));
  out.push(header);
  for (let i = 0; i < entries.length; i++) {
    const stage = entries[i]!;
    out.push(`${chipIndent}${opts.chipFn(stage.stageName)}`);
    for (const ml of renderMarkdownToLines(
      stage.body,
      avail,
      avail,
      opts.glyphs
    )) {
      if (ml.length === 0) {
        out.push('');
        continue;
      }
      for (const visual of wrapAnsiLine(ml, avail, avail)) {
        out.push(`${bodyIndent}${visual}`);
      }
    }
    if (stage.truncatedBy && stage.truncatedBy > 0) {
      out.push(
        `${bodyIndent}${chalk.dim(`(+${stage.truncatedBy} more lines)`)}`
      );
    }
    if (i < entries.length - 1) out.push('');
  }
  return out;
}

/**
 * Approval-prompt renderer for the `subagent` tool: a per-stage pipeline tree
 * the user can read at a glance instead of a raw key:value JSON dump. Returns
 * one string per line (formatToolArgLines's contract, one <Text> per line).
 */
export function formatSubagentApprovalLines(
  content: string,
  termCols?: number,
  colors?: {
    getStageInputColor?: (stageName: string) => (text: string) => string;
    /** Active glyph set (Unicode/ASCII connectors). */
    glyphs?: Glyphs;
  }
): string[] | null {
  if (!content) return null;
  let args: { task?: string; stages?: SubagentStage[] };
  try {
    args = JSON.parse(content);
  } catch {
    return null;
  }
  if (!args || typeof args !== 'object') return null;

  const cols = Math.max(
    40,
    termCols ??
      (typeof process !== 'undefined' ? process.stdout?.columns : undefined) ??
      120
  );
  const lines: string[] = [];

  // Labels + structural glyphs stay dim; stage names render as `[name]` in the
  // per-agent color (matching the footer/final block) so a row maps to a stage.
  const inputColor = (name: string): ((text: string) => string) =>
    colors?.getStageInputColor?.(name) ?? chalk.blue;
  const stages = Array.isArray(args.stages) ? args.stages : [];
  // No standalone `task:` line — already surfaced via each stage's {task} sub.
  if (stages.length > 0) {
    lines.push(
      ...renderPipelineStages(stages, args.task, {
        inputColor,
        cols,
        glyphs: colors?.glyphs,
        showRoles: true,
        showDeps: true,
        showPrompts: true,
      })
    );
  }

  return lines.length > 0 ? lines : null;
}

/**
 * Render the subagent tool's final state in scrollback: header, pipeline tree,
 * optional raw `full output:` (verbose) and `response summary:` sections, and
 * an error block. The summary prefers each stage's `contextSummary`, falling
 * back to `taskResult` capped at TASK_RESULT_MAX_LINES; stages with neither
 * are skipped.
 */
export function renderSubagentFinalBlock(
  content: string,
  result: { status: string; error?: string; output?: unknown } | undefined,
  status: ToolCallRenderInfo['status'],
  elapsed?: number,
  stageSummaries?: SubagentStageSummary[],
  colors?: {
    getStageInputColor?: (stageName: string) => (text: string) => string;
    getStageOutputColor?: (stageName: string) => (text: string) => string;
    /**
     * Display knobs from /verbose config. Tests that don't pass this fall
     * back to disk via getVerboseDisplay() — same pattern as renderMessageToText.
     */
    display?: VerboseDisplayConfig;
    /** Filter override for the `subagent` output gate; reads disk when omitted. */
    filtersOverride?: readonly string[];
    /** Active glyph set, threaded to each stage's markdown body. */
    glyphs?: Glyphs;
    /** Running-tail spinner glyph (see STATUS-SLOT CONTRACT in tools.ts). */
    runningSpinner?: string;
    /** Paints a yellow ' ...' tail when this tool awaits approval; precedence over spinner. */
    awaitingApproval?: boolean;
    /** User denied at the prompt → tail is `DENIED` not `FAILED`. */
    rejected?: boolean;
  }
): string {
  const display = colors?.display ?? getVerboseDisplay();
  const sub = display.subagent;
  const cols = Math.max(
    40,
    (typeof process !== 'undefined' ? process.stdout?.columns : undefined) ??
      120
  );
  const lines: string[] = [];
  const inputColor = (name: string): ((text: string) => string) =>
    colors?.getStageInputColor?.(name) ?? chalk.blue;
  const outputColor = (name: string): ((text: string) => string) =>
    colors?.getStageOutputColor?.(name) ?? responseChip;

  // Flip FAILED → DENIED when the user rejected: either the explicit flag, or
  // the backend's canonical "denied by the user" text on replayed denials.
  const errText = result?.status === 'error' ? (result.error ?? '') : '';
  const wasDeniedByUser =
    !!colors?.rejected ||
    (typeof errText === 'string' &&
      /denied by the user|rejected because the arguments supplied are forbidden/i.test(
        errText
      ));
  const tail =
    status === 'error'
      ? wasDeniedByUser
        ? chalk.red(' DENIED')
        : chalk.red(' FAILED')
      : status === 'done'
        ? display.showElapsed && elapsed != null
          ? chalk.dim(` ${formatElapsed(elapsed)}`)
          : chalk.dim(' done')
        : status === 'cancelled'
          ? // Must precede the running fallbacks: a cancelled subagent is
            // finished, else the append-only row sticks on `subagent ...`.
            chalk.yellow(' ✗ cancelled')
          : colors?.awaitingApproval
            ? chalk.yellow(' ...')
            : colors?.runningSpinner
              ? ` ${colors.runningSpinner}`
              : chalk.dim(' ...');
  lines.push(`${chalk.bold('subagent')}${tail}`);

  let task: string | null = null;
  let stages: SubagentStage[] = [];
  try {
    const args = JSON.parse(content);
    if (typeof args.task === 'string') task = args.task;
    if (Array.isArray(args.stages)) stages = args.stages;
  } catch {
    // args unparsable — fall through; the error block below still renders.
  }
  // No standalone `task:` line (duplicates the {task} substitution below);
  // `task` is parsed above only for that substitution.

  if (sub.pipeline && stages.length > 0) {
    lines.push(
      ...renderPipelineStages(stages, task, {
        inputColor,
        cols,
        glyphs: colors?.glyphs,
        showRoles: sub.roles,
        showDeps: sub.deps,
        showPrompts: sub.prompts,
      })
    );
  }

  // Verbose mode (subagent passes the filter): surface the FULL per-stage
  // taskResult with red ▸ chips — what the parent literally received before
  // the joiner discarded it. Order is pipeline → raw → summary so the eye
  // lands on the digest last.
  const showRawSection =
    status === 'done' &&
    result?.status !== 'error' &&
    Array.isArray(stageSummaries) &&
    stageSummaries.length > 0 &&
    shouldShowToolOutput('subagent', colors?.filtersOverride);
  if (showRawSection) {
    const rawStages = stageSummaries!
      .filter((s) => (s.taskResult ?? '').trim().length > 0)
      .map((s) => ({ stageName: s.stageName, body: s.taskResult }));
    if (rawStages.length > 0) {
      lines.push(
        ...renderDigestSection(chalk.red.bold('  full output:'), rawStages, {
          chipFn: (n) => chalk.red.bold(`▸ ${n}`),
          cols,
          glyphs: colors?.glyphs,
        })
      );
    }
  }

  if (
    sub.responses &&
    status === 'done' &&
    result?.status !== 'error' &&
    Array.isArray(stageSummaries) &&
    stageSummaries.length > 0
  ) {
    type RenderableStage = {
      stageName: string;
      body: string;
      truncatedBy: number;
    };
    // Cap fallback taskResult bodies (contextSummary is already a digest).
    const TASK_RESULT_MAX_LINES = 30;
    const renderable: RenderableStage[] = [];
    for (const s of stageSummaries) {
      const ctx = (s.contextSummary ?? '').trim();
      if (ctx.length > 0) {
        renderable.push({
          stageName: s.stageName,
          body: s.contextSummary,
          truncatedBy: 0,
        });
        continue;
      }
      const tr = (s.taskResult ?? '').trim();
      if (tr.length === 0) continue;
      const trLines = s.taskResult.split('\n');
      if (trLines.length <= TASK_RESULT_MAX_LINES) {
        renderable.push({
          stageName: s.stageName,
          body: s.taskResult,
          truncatedBy: 0,
        });
      } else {
        const truncated = trLines.slice(0, TASK_RESULT_MAX_LINES).join('\n');
        renderable.push({
          stageName: s.stageName,
          body: truncated,
          truncatedBy: trLines.length - TASK_RESULT_MAX_LINES,
        });
      }
    }
    if (renderable.length > 0) {
      // Always emit the chip (even single-stage) as the "returned" signal.
      lines.push(
        ...renderDigestSection(chalk.dim('  response summary:'), renderable, {
          chipFn: (n) => chalk.bold(outputColor(n)(`▸ ${n}`)),
          cols,
          glyphs: colors?.glyphs,
        })
      );
    }
  }

  // Errors still surface.
  if (result?.status === 'error') {
    const errText = result.error ?? extractSubagentOutput(result) ?? '';
    if (errText) {
      const indent = '    ';
      const g = resolveGlyphs(colors?.glyphs);
      const avail = Math.max(
        20,
        cols - visibleWidth(`${indent}${g.lineVertical} `) - 1
      );
      lines.push(chalk.dim(`    ${g.cornerTopLeft}${g.lineHorizontal} error:`));
      for (const raw of errText.split('\n')) {
        const chunks = wrapAtWords(raw, avail, avail);
        if (chunks.length === 0) {
          lines.push(`${indent}${chalk.red(g.lineVertical)}`);
          continue;
        }
        for (const chunk of chunks) {
          lines.push(`${indent}${chalk.red(`${g.lineVertical} ${chunk}`)}`);
        }
      }
      lines.push(chalk.dim(`    ${g.cornerBottomLeft}${g.lineHorizontal}`));
    }
  }

  return lines.join('\n');
}

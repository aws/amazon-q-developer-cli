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
 * Approval-prompt renderer for the `subagent` tool. Replaces the generic
 * key:value JSON dump with a per-stage tree so the user can read the
 * pipeline at a glance: task line, then one block per stage with its role,
 * dependencies, and prompt soft-wrapped to terminal width.
 *
 * Returns one string per line (matches formatToolArgLines's contract so the
 * approval prompt can render each line in its own <Text>).
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
  // No standalone `task:` line — it's already surfaced via each stage's {task}
  // substitution, so printing it here would duplicate it.
  if (stages.length > 0) {
    const g = resolveGlyphs(colors?.glyphs);
    lines.push(chalk.dim('  pipeline:'));
    for (let i = 0; i < stages.length; i++) {
      const stage = stages[i] ?? {};
      const isLast = i === stages.length - 1;
      const branch = isLast
        ? `${g.cornerBottomLeft}${g.lineHorizontal}`
        : `${g.teeRight}${g.lineHorizontal}`;
      const stem = isLast ? '  ' : `${g.lineVertical} `;
      const name = stage.name || `stage-${i + 1}`;
      const role = stage.role ? chalk.dim(` (${stage.role})`) : '';
      const deps =
        Array.isArray(stage.depends_on) && stage.depends_on.length > 0
          ? chalk.dim(` ← ${stage.depends_on.join(', ')}`)
          : '';
      lines.push(
        `    ${chalk.dim(branch)} ${inputColor(name)(`[${name}]`)}${role}${deps}`
      );
      // Substitute {task} on render so the display matches what the spawned
      // subagent receives (belt-and-suspenders for older agent binaries that
      // ship the raw template; the backend also substitutes).
      const rawPrompt = stage.prompt_template;
      const prompt =
        rawPrompt && args.task
          ? rawPrompt.replace(/\{task\}/g, args.task)
          : rawPrompt;
      if (prompt && typeof prompt === 'string') {
        const promptIndent = `    ${chalk.dim(stem)} `;
        // 7 = width of "    │ " + 1-col safety margin (stdout.columns is
        // sometimes one off, causing stray col-0 soft-wraps otherwise).
        const indentVisibleCols = 7;
        const avail = Math.max(20, cols - indentVisibleCols);
        lines.push(
          ...renderStagePromptLines(prompt, avail, promptIndent, colors?.glyphs)
        );
      }
    }
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
    const g = resolveGlyphs(colors?.glyphs);
    lines.push(chalk.dim('  pipeline:'));
    for (let i = 0; i < stages.length; i++) {
      const stage = stages[i] ?? {};
      const isLast = i === stages.length - 1;
      const branch = isLast
        ? `${g.cornerBottomLeft}${g.lineHorizontal}`
        : `${g.teeRight}${g.lineHorizontal}`;
      const stem = isLast ? '  ' : `${g.lineVertical} `;
      const name = stage.name || `stage-${i + 1}`;
      const role = sub.roles && stage.role ? chalk.dim(` (${stage.role})`) : '';
      const deps =
        sub.deps &&
        Array.isArray(stage.depends_on) &&
        stage.depends_on.length > 0
          ? chalk.dim(` ← ${stage.depends_on.join(', ')}`)
          : '';
      lines.push(
        `    ${chalk.dim(branch)} ${inputColor(name)(`[${name}]`)}${role}${deps}`
      );
      // {task} substitution — see formatSubagentApprovalLines.
      const rawPrompt = stage.prompt_template;
      const prompt =
        rawPrompt && task ? rawPrompt.replace(/\{task\}/g, task) : rawPrompt;
      if (
        sub.prompts &&
        prompt &&
        typeof prompt === 'string' &&
        prompt.length > 0
      ) {
        const promptIndent = `    ${chalk.dim(stem)} `;
        const avail = Math.max(20, cols - 7);
        lines.push(
          ...renderStagePromptLines(prompt, avail, promptIndent, colors?.glyphs)
        );
      }
    }
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
      // Chip at col 7 (parallel to the pipeline `[stage]` chip), body at col 9,
      // pre-wrapped via wrapAnsiLine (SGR carryover) so continuations don't
      // crash to col 0.
      const chipIndent = '       ';
      const bodyIndent = '         ';
      const avail = Math.max(20, cols - visibleWidth(bodyIndent));
      lines.push(chalk.red.bold('  full output:'));
      for (let i = 0; i < rawStages.length; i++) {
        const stage = rawStages[i]!;
        lines.push(`${chipIndent}${chalk.red.bold(`▸ ${stage.stageName}`)}`);
        for (const ml of renderMarkdownToLines(
          stage.body,
          avail,
          avail,
          colors?.glyphs
        )) {
          if (ml.length === 0) {
            lines.push('');
            continue;
          }
          for (const visual of wrapAnsiLine(ml, avail, avail)) {
            lines.push(`${bodyIndent}${visual}`);
          }
        }
        if (i < rawStages.length - 1) {
          lines.push('');
        }
      }
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
      // Parallels the pipeline section (chip at col 7, body at col 9,
      // pre-wrapped) but differentiates via header text, pink chip color, and
      // the ▸ glyph.
      const chipIndent = '       ';
      const bodyIndent = '         ';
      const avail = Math.max(20, cols - visibleWidth(bodyIndent));
      lines.push(chalk.dim('  response summary:'));
      for (let i = 0; i < renderable.length; i++) {
        const stage = renderable[i]!;
        // Always emit the chip (even single-stage) as the "returned" signal.
        lines.push(
          `${chipIndent}${chalk.bold(outputColor(stage.stageName)(`▸ ${stage.stageName}`))}`
        );
        for (const ml of renderMarkdownToLines(
          stage.body,
          avail,
          avail,
          colors?.glyphs
        )) {
          if (ml.length === 0) {
            lines.push('');
            continue;
          }
          for (const visual of wrapAnsiLine(ml, avail, avail)) {
            lines.push(`${bodyIndent}${visual}`);
          }
        }
        if (stage.truncatedBy > 0) {
          lines.push(
            `${bodyIndent}${chalk.dim(`(+${stage.truncatedBy} more lines)`)}`
          );
        }
        if (i < renderable.length - 1) {
          lines.push('');
        }
      }
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

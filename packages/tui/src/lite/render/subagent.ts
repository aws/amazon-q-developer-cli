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
  // Show error text when the call failed
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
 * Render a subagent stage's `prompt_template` as styled markdown safely.
 * Used by both the approval-prompt tree ({@link formatSubagentApprovalLines})
 * and the post-completion chat-log block ({@link renderSubagentFinalBlock}),
 * so both surfaces show the same `**bold**` / `` `code` `` / list / etc.
 * styling that agent prose already gets — instead of dumping the prompt
 * as literal markdown source.
 *
 * Safety-critical pipeline (mirrors how agent prose is rendered):
 *
 *   1. {@link renderMarkdownToLines} — parses the prompt with `parseMarkdown` /
 *      `parseInlineMarkdown`, which by contract treat unclosed inline markers
 *      (`**foo`, `*foo`, `` `foo ``, `[foo`) as literal text. Pinned by
 *      render.test.ts:367-386. So a partial bold/italic/code/link in a
 *      multi-line prompt can't open a styled span that never closes.
 *
 *   2. {@link wrapAnsiLine} per emitted line. The agent-prose pipeline
 *      deliberately leaves paragraphs UNWRAPPED (`wrapStyled(s, 0, 0)`) so
 *      copy-paste preserves logical lines and the terminal handles soft-
 *      wrap. That works for agent prose because agent prose lives at
 *      column 0 — terminal soft-wrap to column 0 is the correct
 *      continuation indent. Stage prompts live under a tree-stem indent
 *      (`    │ ` for non-last stages, `      ` for the last), so an
 *      unwrapped long paragraph would let the terminal crash continuation
 *      rows back to column 0 — the exact regression the
 *      `formatSubagentApprovalLines` long-prompt-wrap test guards against.
 *      Re-wrapping each markdown line through `wrapAnsiLine(md, avail,
 *      avail)` bounds width within the indent's column budget AND
 *      preserves the ANSI closers (`\x1b[22m`, `\x1b[23m`,
 *      `\x1b[24m\x1b[39m\x1b[2m`, `\x1b[39m`) on every wrap boundary so
 *      bold/italic/underline/color don't bleed into the next visual row,
 *      adjacent stages, or the rest of the message.
 *
 *   3. Prepend `indent` on every non-empty visual row so wrapped
 *      continuations stay under the tree stem.
 *
 *   4. Preserve markdown's blank-row paragraph separators (`''`) so a
 *      multi-paragraph prompt visibly separates. Same convention the
 *      responses section in {@link renderSubagentFinalBlock} already uses
 *      for stage body text.
 *
 * Why not call {@link renderAgentMessage} directly: it bakes in a
 * `<agent>:` role tag and a structural-block-needs-own-line first-line
 * rule — both designed for top-level chat replies and both at odds with
 * the tree-stem indent here. Going through the lower-level
 * {@link renderMarkdownToLines} keeps the body styled without any of that
 * framing chrome.
 *
 * Why not stick with `wrapAtWords` (the previous plain-text wrapper): it
 * does no markdown parsing, so users saw `**bold**` and `` `code` ``
 * literally in the approval prompt and the post-completion block. It also
 * has no ANSI awareness, which would be unsafe the moment we started
 * emitting styled text through it.
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
      // Markdown paragraph break — preserve as a blank row so the body
      // visually separates paragraphs/lists/code blocks within the
      // pipeline tree. Matches the responses-section convention in
      // renderSubagentFinalBlock (it pushes `''` for blank lines too).
      out.push('');
      continue;
    }
    // Re-wrap with ANSI closer preservation. Block segments (lists,
    // tables, blockquotes, code) come out of renderMarkdownToLines
    // already wrapped at `avail`, so this pass is a no-op for them;
    // paragraph segments arrive unwrapped and this is what bounds them.
    for (const visual of wrapAnsiLine(md, avail, avail)) {
      // Skip whitespace-only continuation rows (orphan space cells from
      // the wrap). The previous wrapAtWords-based code skipped these
      // too — they'd otherwise render as an indent-only ghost row mid-
      // paragraph.
      if (visual.trim().length === 0) continue;
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
    /**
     * Active glyph set — switches the pipeline tree connectors (`├─`, `└─`,
     * `│ `) to ASCII (`+-`, `+-`, `| `) when `chat.allowAsciiArt=false`.
     */
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

  // Color hierarchy: labels ("task:", "pipeline:") and structural glyphs
  // (├─, │, ←) stay dim. Stage names render as `[name]` in the per-agent
  // color used everywhere else (footer activity strip, chat-log final block,
  // panel header), so the user can match a row anywhere to a stage at a
  // glance. Task value + prompt bodies stay default — fewer competing
  // colors, less visual noise.
  const inputColor = (name: string): ((text: string) => string) =>
    colors?.getStageInputColor?.(name) ?? chalk.blue;
  const stages = Array.isArray(args.stages) ? args.stages : [];
  // The standalone `task:` line is intentionally omitted: the task is the
  // overall input prompt and is already surfaced inside the pipeline (each
  // stage's prompt substitutes {task}), so printing it here duplicated it —
  // once raw above the pipeline, once styled within it.
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
      // The schema instructs the model to use `{task}` literally as a
      // placeholder for the overall task (see crates agent_crew.rs
      // TOOL_SCHEMA). The backend substitutes when feeding the spawned
      // subagent; substituting on render keeps the UI display 1:1 with
      // what the spawned subagent actually receives. Backend mod.rs also
      // substitutes on the display copy of the tool input — this is a
      // belt-and-suspenders fallback for sessions running an older agent
      // binary that still ships the raw template.
      const rawPrompt = stage.prompt_template;
      const prompt =
        rawPrompt && args.task
          ? rawPrompt.replace(/\{task\}/g, args.task)
          : rawPrompt;
      if (prompt && typeof prompt === 'string') {
        const promptIndent = `    ${chalk.dim(stem)} `;
        // 7 = visible width of "    │ " (4 spaces + │ + space) plus a 1-col
        // safety margin. Without the margin, the terminal occasionally
        // soft-wraps the longest unbreakable runs (URLs, long paths) onto
        // a second physical row at column 0 because process.stdout.columns
        // is one off from the actual viewport width.
        const indentVisibleCols = 7;
        const avail = Math.max(20, cols - indentVisibleCols);
        // Markdown-render the prompt body — see renderStagePromptLines
        // for the full safety story (parseMarkdown literal-fallback for
        // unclosed inline markers, wrapAnsiLine ANSI-closer preservation,
        // indent reapplied on every visual row so continuations don't
        // crash to column 0).
        lines.push(
          ...renderStagePromptLines(prompt, avail, promptIndent, colors?.glyphs)
        );
      }
    }
  }

  return lines.length > 0 ? lines : null;
}

/**
 * Render the subagent tool's final state in scrollback.
 *
 * Layout:
 *   subagent <elapsed>
 *     task: <task>
 *     pipeline:
 *       ├─ [<name>] (<role>)
 *       │  <prompt_template>
 *       └─ [<name>] ← <deps>
 *          <prompt_template>
 *     responses:
 *       ▸ <name>
 *         <stage's contextSummary, plain-text wrapped>
 *
 *       ▸ <name>
 *         ...
 *
 * The summary block prefers each stage's `contextSummary` (a parent-
 * consumable digest the agent_crew joiner otherwise discards before the
 * parent agent sees it). When a stage skipped contextSummary — short
 * single-turn answers, or the Rust backend's failsafe path — we fall
 * back to `taskResult` capped at TASK_RESULT_MAX_LINES with an overflow
 * footnote. Stages that wrote neither are skipped silently.
 *
 * Errors still render in full so failures are visible.
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
    /**
     * Filter list override for the `subagent` output gate. The /verbosity
     * preview pane uses this to reflect a draft filter list without writing
     * to disk. Falls back to disk via getVerboseConfig() when omitted.
     */
    filtersOverride?: readonly string[];
    /**
     * Active glyph set — passed through to the markdown body renderer for
     * each stage's response so any tables, blockquotes, or HRs in the
     * combined summary use the same Unicode/ASCII set as the surrounding
     * scrollback.
     */
    glyphs?: Glyphs;
    /**
     * Spinner glyph (or {@link SPINNER_PLACEHOLDER} sentinel) for the
     * running tail. When set and `status === 'running'`, the header line
     * paints `subagent <spinner>` instead of the static dim ' ...'.
     * Without this thread-through, the subagent tool was the only non-
     * trivial tool that didn't pick up motion in the live region while
     * actively running — the tail hardcoded ' ...' regardless.
     */
    runningSpinner?: string;
    /**
     * When true, the parent subagent tool is the current pending-approval
     * target — paint the running tail as a yellow ' ...' instead of the
     * spinner or dim ellipsis. Mirrors the same flag on
     * {@link ToolCallRenderInfo}; takes precedence over `runningSpinner`.
     */
    awaitingApproval?: boolean;
    /**
     * True when the user denied the parent subagent tool at the approval
     * prompt (msg.status === 'rejected'). Paints the tail as `DENIED`
     * instead of `FAILED` so the chat log distinguishes "user said no"
     * from "backend reported an error". Without this flag, a rejected
     * subagent rendered identically to a backend-failed one and the
     * user couldn't tell which had happened.
     */
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
  // Per-stage input chip color: defer to the caller's resolver when present
  // (so each subagent gets its own palette entry), otherwise fall back to the
  // pre-existing blue so this function still works in unit tests / pure
  // contexts that don't supply a theme.
  const inputColor = (name: string): ((text: string) => string) =>
    colors?.getStageInputColor?.(name) ?? chalk.blue;
  // Output chip color: the caller's brightened-shade resolver, falling back
  // to the pink response chip used before per-agent colors landed.
  const outputColor = (name: string): ((text: string) => string) =>
    colors?.getStageOutputColor?.(name) ?? responseChip;

  // Distinguish user-rejection from genuine failure on the parent subagent
  // tool. Two signals: (1) explicit rejected flag passed by the caller when
  // msg.status === 'rejected' (user pressed n at the approval prompt), and
  // (2) result.error text matching the canonical "denied by the user" string
  // that the backend stamps on replayed denials (see app-store.ts replay
  // detection). Either signal flips `FAILED` → `DENIED` so the chat log
  // distinguishes "user said no" from "backend reported an error".
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
          ? // Terminal cancelled state — mirror renderToolCall's generic arm
            // (`✗ cancelled`). Must precede the running fallbacks below: a
            // cancelled subagent is finished, so without this arm it falls
            // through to the running ' ...' and the scrollback row is stuck
            // showing `subagent ...` forever (static is append-only).
            chalk.yellow(' ✗ cancelled')
          : colors?.awaitingApproval
            ? // Yellow ' ...' matches the approval prompt's [t] hotkey color
              // so the tool body and the prompt below the input read as one
              // visual unit. Takes precedence over the spinner — the agent
              // isn't progressing while approval is pending.
              chalk.yellow(' ...')
            : colors?.runningSpinner
              ? // Live-region spinner glyph (or the SPINNER_PLACEHOLDER sentinel
                // which LiteLiveRegion swaps per frame). Without this, the
                // subagent tail was the only non-trivial tool that didn't
                // pick up motion while running.
                ` ${colors.runningSpinner}`
              : chalk.dim(' ...');
  lines.push(`${chalk.bold('subagent')}${tail}`);

  let task: string | null = null;
  let stages: SubagentStage[] = [];
  try {
    const args = JSON.parse(content);
    if (typeof args.task === 'string') task = args.task;
    if (Array.isArray(args.stages)) stages = args.stages;
  } catch {
    // args unparsable — fall through; error block below still renders if present
  }
  // The standalone `task:` line is intentionally omitted: the task is the
  // overall input prompt and is already surfaced inside the pipeline (each
  // stage's prompt substitutes {task}), so printing it here duplicated it —
  // once raw above the pipeline, once styled within it. `task` is still
  // parsed above for the {task} substitution in the stage prompts below.

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
      // See formatSubagentApprovalLines for the {task} substitution
      // rationale — same belt-and-suspenders fallback for the post-
      // completion final block.
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
        // Same markdown pipeline used in formatSubagentApprovalLines so the
        // approval prompt and the post-completion scrollback block render
        // the prompt identically. See renderStagePromptLines for the
        // safety contract.
        lines.push(
          ...renderStagePromptLines(prompt, avail, promptIndent, colors?.glyphs)
        );
      }
    }
  }

  // Compact summary block: per-stage `contextSummary` rendered through the
  // same markdown pipeline as the parent agent's message (renderAgentMessage)
  // so headings, lists, code, and emphasis surface in subagent output the
  // same way they do in the main reply. Scope is strict — only stage body
  // text goes through markdown; the pipeline tree, task line, and error
  // block stay plain so structural framing isn't reflowed.
  //
  // Fallback chain per stage: contextSummary → taskResult (capped). Short
  // tasks and the Rust backend's failsafe path leave contextSummary empty,
  // so without the taskResult fallback the responses section would be
  // suppressed and the user sees an empty pipeline tree.
  // In verbose mode (when the subagent tool passes the filter), surface the
  // FULL per-stage `taskResult` with red ▸ chips. This is what the parent
  // agent literally received in its context window before the joiner
  // discarded it — the point of /verbose is to see what the parent saw.
  // The summary block below still renders so the user can compare the
  // long raw text to the compressed digest. Order is intentional: pipeline
  // tree → raw output → summary, so the eye lands on the digest last.
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
      // Chip lives at col 7 — same column the pipeline tree's `[stage]`
      // chip lands at (4 spaces + `├─` + space = 7 visible cols, so `[`
      // sits at col 7), so the eye tracks `▸ scan` and `[scan]` as
      // parallel structure across sections. Body indents one level past
      // the chip (col 9) and is PRE-WRAPPED here so long lines get a
      // clean continuation indent instead of crashing back to col 0
      // via terminal soft-wrap. The pipeline tree above already pre-
      // wraps via renderStagePromptLines — the response section was
      // the only inconsistent surface.
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
          // Pre-wrap so each visual row carries the response indent;
          // continuations don't fall back to col 0. wrapAnsiLine
          // preserves SGR carryover across rows so styled spans don't
          // bleed past the wrap boundary.
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
    // Cap fallback `taskResult` bodies at this many lines. Anything longer
    // is tail-truncated with a "(+N more lines)" footnote so the parent
    // summary block stays terminal-friendly. `contextSummary` is already
    // a backend-side digest so it isn't capped.
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
      // Differentiate response output from pipeline input on three axes:
      // header phrasing ("responses:" vs "pipeline:"), chip color (pink
      // vs blue), and chip glyph (▸ name vs [name]). The previous
      // ┌─ │ └─ box framing made every section look the same.
      //
      // Indent layout matches the pipeline section above so the eye reads
      // both as parallel structure: chip at col 7 (same column as
      // `[stage]` after `    ├─ ` — 4 spaces + tree connector + space),
      // body at col 9. Bodies are PRE-WRAPPED here via wrapAnsiLine so
      // long lines indent cleanly on continuation rows instead of
      // crashing back to col 0 via terminal soft-wrap — matching what
      // renderStagePromptLines does for the pipeline prompts above.
      // Trade-off vs unwrapped agent prose: agent prose lives at col 0
      // so terminal soft-wrap to col 0 IS the right continuation indent;
      // subagent summaries live in chrome at col 7+, so soft-wrap to
      // col 0 breaks the visual frame for marginal copy-paste benefit.
      const chipIndent = '       ';
      const bodyIndent = '         ';
      const avail = Math.max(20, cols - visibleWidth(bodyIndent));
      lines.push(chalk.dim('  response summary:'));
      for (let i = 0; i < renderable.length; i++) {
        const stage = renderable[i]!;
        // Header chip is always emitted — even single-stage runs — so the
        // brightened-shade chip consistently signals "this is what the agent
        // returned" rather than relying on the upstream pipeline tree.
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

  // Errors still surface — failures need to be visible.
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

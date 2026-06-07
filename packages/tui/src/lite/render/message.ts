import chalk from 'chalk';
import {
  getVerboseDisplay,
  shouldShowToolOutput,
  categorize,
  type VerboseDisplayConfig,
} from '../verbose.js';
import { TASK_TOOL_NAMES } from '../../types/agent-events.js';
import { needsLeadingBlankByRole } from '../blank-rules.js';
import type { Glyphs } from '../../utils/glyphs.js';
import { isErrorContent, type RenderTheme } from './theme.js';
import {
  renderUserMessage,
  renderAgentMessage,
  renderThinkingBlock,
  renderShellOutputBlock,
} from './markdown.js';
import {
  renderToolCall,
  renderWriteToolCall,
  renderReadToolCall,
  renderVerboseOutput,
  formatToolArgLines,
  formatTaskToolBody,
  extractInlineArg,
  extractToolReasoning,
  isWriteTool,
  isReadTool,
  applyLineCap,
  type ToolCallRenderInfo,
} from './tools.js';
import { renderSubagentFinalBlock } from './subagent.js';

export function renderSystemError(message: string): string {
  // Only color the first line's `error:` prefix red. Subsequent lines (e.g.
  // a per-item list under the header) keep their own embedded chalk codes,
  // which would otherwise be clobbered by an outer chalk.red wrap.
  // No trailing newline — vertical spacing is owned by needsLeadingBlank.
  const [first, ...rest] = message.split('\n');
  const head = chalk.red(`error: ${first ?? ''}`);
  if (rest.length === 0) return head;
  return [head, ...rest].join('\n');
}

export function renderSystemInfo(message: string): string {
  return chalk.dim(message);
}

export interface TurnSummaryInfo {
  meteringUsage: Array<{ value: number; unit: string; unitPlural: string }>;
  durationMs?: number;
}

export function renderTurnSummary(info: TurnSummaryInfo): string {
  const parts = info.meteringUsage.map(
    (u) => `${u.value} ${u.value === 1 ? u.unit : u.unitPlural}`
  );
  const duration =
    info.durationMs != null ? ` • ${formatDuration(info.durationMs)}` : '';
  return chalk.dim.italic(`${parts.join(' · ')}${duration}`);
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(0)}s`;
  const mins = Math.floor(ms / 60000);
  const secs = Math.floor((ms % 60000) / 1000);
  return secs > 0 ? `${mins}m ${secs}s` : `${mins}m`;
}

export interface MessageLike {
  id: string;
  role: 'user' | 'model' | 'tool_use' | 'system';
  content: string;
  name?: string;
  isFinished?: boolean;
  result?: { status: string; error?: string; output?: unknown };
  /**
   * Approval-prompt outcome stamped on the store message (`ToolUseStatus`);
   * `'rejected'` when the user denied the tool call. Kept as `string` (like
   * `result.status` above) so the store's `MessageType` stays structurally
   * assignable without the render layer importing the store enum. Distinct
   * from a `result.status` of `'error'`/`'cancelled'` — a rejected call may
   * carry no `result` at all. Read in the `tool_use` branch to paint the
   * `DENIED` chip.
   */
  status?: string;
  success?: boolean;
  standalone?: boolean;
  agentName?: string;
  startTime?: number;
  finishTime?: number;
  /**
   * Freeform thinking text the model produced before its spoken reply.
   * Only set on Model messages (other roles ignore it). Rendered as a
   * dedicated block above the agent's text when
   * {@link VerboseDisplayConfig.showThinkingContent} is on. Distinct from
   * `__tool_use_purpose` (the per-tool-call "why" surfaced via
   * {@link VerboseDisplayConfig.showToolReasoning}).
   */
  thinking?: string;
  /**
   * Per-tool "why" preserved verbatim from the model's `__tool_use_purpose`
   * field. Captured at the ACP boundary in app-store.ts before the
   * ToolCall handler's per-shape synthesis rebuilds `content` (which drops
   * the field for edit-kind tools). Read by
   * {@link extractToolReasoning} as the primary source for the reasoning
   * slot; the JSON-blob fallback handles tool kinds whose handler keeps
   * the field in `content`.
   */
  purpose?: string;
  /**
   * `true` when this Model message is carrying the streaming output of a
   * `!` shell-escape command (handled in app-store's shell-escape branch),
   * not the agent's inference output. Routes rendering to
   * {@link renderShellOutputBlock} instead of {@link renderAgentMessage}
   * so the row gets a brand-purple `! ` left gutter — visually distinct
   * from agent prose, and signaling that any keystrokes the user types
   * while this is in flight are forwarded to the PTY rather than into
   * the prompt buffer.
   */
  shellOutput?: boolean;
}

export interface SubagentStageSummary {
  /** Stage name (matches the stage's agentName / session name). */
  stageName: string;
  /**
   * The compressed `contextSummary` field from the stage's `summary` tool
   * call — meant by Kiro's backend as a parent-consumable digest distinct
   * from the long `taskResult` body. We harvest it directly off the inner
   * `summary` tool message because the agent_crew joiner discards it
   * before it reaches the parent's combined output string.
   *
   * May be empty when a stage answered in a single turn and skipped the
   * compressed digest — render falls back to {@link taskResult}.
   */
  contextSummary: string;
  /**
   * Long-form result body from the same `summary` tool call. Used as a
   * fallback when `contextSummary` is empty (short tasks, failsafe-path
   * summaries from the Rust agent_crew backend). Capped at render time
   * to keep the section terminal-friendly.
   */
  taskResult: string;
}

export interface RenderContext {
  /**
   * Tool-call ID currently awaiting approval (if any). Render of that
   * tool's chat-log entry suppresses its diff, since the diff is also
   * shown above in the approval prompt and we don't want it twice.
   */
  pendingApprovalToolCallId?: string | null;
  /** Terminal columns for diff full-row backgrounds. */
  termCols?: number;
  /**
   * Per-subagent-invocation stage summaries, keyed by the parent
   * `subagent` tool's message id. Built once in LiteLayout from the
   * inner `summary` tool calls; used by renderSubagentFinalBlock to
   * render a compact "Summary of findings" body in non-verbose mode.
   */
  subagentSummariesById?: Map<string, SubagentStageSummary[]>;
  /**
   * Resolves a stage name to its per-agent input color (used for `[stage]`
   * chips in the pipeline tree). Optional — when omitted, callers fall back
   * to a single neutral accent so the renderer stays usable in tests / pure
   * contexts that don't have a theme available.
   */
  getStageInputColor?: (stageName: string) => (text: string) => string;
  /**
   * Brighter shade of the same per-agent color, used for response chips
   * (▸ stage) so the eye separates "what we sent in" from "what the agent
   * returned" while keeping a single agent identity.
   */
  getStageOutputColor?: (stageName: string) => (text: string) => string;
  /**
   * Display knobs from /verbose config. Optional — when omitted the renderer
   * reads from disk via getVerboseDisplay(). Tests pass an explicit override
   * to keep render output deterministic.
   */
  display?: VerboseDisplayConfig;
  /**
   * Filter list override for `shouldShowToolOutput` gating. When omitted the
   * renderer reads filters from disk via getVerboseConfig(). The /verbosity
   * preview pane passes a draft list so toggling output-bar filters reflects
   * in the synthetic preview without touching saved config.
   */
  filtersOverride?: readonly string[];
  /**
   * Theme accessors for the user-visible colors that depend on /theme. When
   * omitted, the renderer falls back to its hardcoded kiroDark defaults so
   * tests / pure-context callers don't have to wire a theme through. Lite
   * mode passes a real theme via {@link buildRenderTheme}.
   */
  theme?: RenderTheme;
  /**
   * Resolves the active agent's name (custom agents, swapped via /agent) to
   * the per-agent color the footer uses for its agent chip. Lite mode wires
   * this from `getAgentColor` in `agentColors.ts` so scrollback's role tag
   * matches the footer color for the same agent. When omitted, the role tag
   * falls back to `theme.brand` (kiro_default's color), keeping tests and
   * pure-context callers usable without a theme.
   */
  getAgentTagColor?: (agentName: string) => (text: string) => string;
  /**
   * Spinner glyph to substitute into the status slot of running, non-trivial
   * tool calls. Set by the live region so an in-flight tool's row in the
   * chat log picks up motion while every other dimension (args, diff,
   * reasoning) renders identically to its eventual settled state. Omit
   * (or leave undefined) on the static path so finalized rows show the
   * post-run elapsed/done status instead of a frozen spinner glyph baked
   * into already-flushed scrollback.
   */
  runningSpinner?: string;
  /**
   * Active glyph set (Unicode or ASCII fallback). Threaded from
   * `useGlyphs()` in lite components — switches the box-drawing chars used
   * by the markdown table renderer, blockquote bar (`│`), bar-prefixed
   * tool output (`│`), thinking-block rules (`─`), and HR. When absent,
   * defaults to `UNICODE_GLYPHS` via {@link resolveGlyphs} so tests and
   * pure-context callers don't need to wire the field through.
   */
  glyphs?: Glyphs;
}

/**
 * Render any message type to a plain text string for Static output.
 */
export function renderMessageToText(
  msg: MessageLike,
  mainAgentName?: string,
  ctx: RenderContext = {}
): string {
  switch (msg.role) {
    case 'user':
      return renderUserMessage(msg.content, ctx.theme);

    case 'model': {
      if (isErrorContent(msg.content)) {
        return renderSystemError(msg.content);
      }
      // Shell-escape Model messages carry the streaming output of a `!`
      // bash command — not agent inference. Render with the gutter
      // formatter so they're visually distinct from agent prose. Same
      // helper drives the in-flight live region row in LiteLiveRegion,
      // so the live→static flush is a no-op visual transition (the
      // committed scrollback row looks identical to the last live frame).
      //
      // Bypasses the agentText / thinking compose path entirely:
      //   - No `Kiro:` tag — bash output isn't from the agent.
      //   - No thinking block — shell-escape commands don't emit Thought
      //     events; `msg.thinking` is unset for these rows.
      //   - No markdown rendering — bash output is raw text + ANSI escapes,
      //     and forcing markdown through it would mangle programs that
      //     emit `*` (e.g. shell glob output, fzf prompts) or `_` (e.g.
      //     filenames with underscores) as italic.
      if (msg.shellOutput) {
        return renderShellOutputBlock(msg.content, ctx.theme, ctx.termCols);
      }
      // Use the message's own agentName when set (subagent stages), falling
      // back to mainAgentName so the role tag matches whichever persona
      // produced the line. termCols drives the markdown wrapper's per-line
      // column budget so paragraphs/lists/tables/code wrap cleanly without
      // tripping the terminal's own wrap on top of pre-wrapped ANSI.
      const display = ctx.display ?? getVerboseDisplay();
      const agentText = renderAgentMessage(
        msg.content,
        msg.agentName ?? mainAgentName,
        ctx.theme,
        ctx.termCols,
        ctx.getAgentTagColor,
        ctx.glyphs
      );
      // Persisted thinking block: shows the model's freeform thinking above
      // its spoken text in scrollback. Gated by display.showThinkingContent
      // so the lean and minimal density presets don't bloat the chat with
      // thinking text. Borders use the active theme's brand color so
      // /theme bundled:dark|light reflows them consistently with the agent
      // role tag below.
      const thinkingBlock =
        display.showThinkingContent && msg.thinking
          ? renderThinkingBlock(
              msg.thinking,
              ctx.theme,
              ctx.termCols,
              ctx.glyphs
            )
          : '';
      if (!thinkingBlock) return agentText;
      if (!agentText) return thinkingBlock;
      // Blank row between the bottom rule and the spoken `Kiro:` line — without
      // it the rule glues directly to the role tag and the section reads as
      // one tightly-packed slab. The leading blank above the block is supplied
      // by needsLeadingBlank when the prior row is a User or ToolUse.
      return thinkingBlock + '\n\n' + agentText;
    }

    case 'tool_use': {
      const isRejected = msg.status === 'rejected';
      const status: ToolCallRenderInfo['status'] = isRejected
        ? 'error'
        : msg.result
          ? msg.result.status === 'error'
            ? 'error'
            : msg.result.status === 'cancelled'
              ? 'cancelled'
              : 'done'
          : msg.isFinished
            ? 'done'
            : 'running';
      const showAgent = msg.agentName && msg.agentName !== mainAgentName;
      const agentPrefix = showAgent ? `[${msg.agentName}] ` : undefined;
      const isWrite = isWriteTool(msg.name || '');
      const isRead = isReadTool(msg.name || '');
      const display = ctx.display ?? getVerboseDisplay();

      // Subagent tool: render a single canonical block per pipeline run.
      // The parent's tool result IS what the parent agent sees, so we show
      // it in full with no truncation. Per-stage tool calls (Read/Grep/etc.)
      // are hidden from the chat log entirely — they live only in the
      // footer activity strip while running.
      if (msg.name === 'subagent') {
        const elapsed =
          msg.startTime && msg.finishTime
            ? msg.finishTime - msg.startTime
            : undefined;
        const stageSummaries = ctx.subagentSummariesById?.get(msg.id);
        return renderSubagentFinalBlock(
          msg.content,
          msg.result,
          status,
          elapsed,
          stageSummaries,
          {
            getStageInputColor: ctx.getStageInputColor,
            getStageOutputColor: ctx.getStageOutputColor,
            display,
            filtersOverride: ctx.filtersOverride,
            glyphs: ctx.glyphs,
            rejected: isRejected,
            // Live-region spinner glyph (or SPINNER_PLACEHOLDER) for the
            // running tail. Without this, the subagent header showed a
            // static ' ...' even when actively running, while every other
            // non-trivial tool got a braille spin — visual inconsistency
            // and a missed motion signal.
            runningSpinner: ctx.runningSpinner,
            // Same yellow-' ...' override as renderToolCall when the
            // parent subagent tool itself is awaiting approval. Stage
            // approvals are surfaced separately by the footer activity
            // strip's `requesting-permission` phase, so this only fires
            // for the rare case where the subagent tool is in the user's
            // approval list.
            awaitingApproval:
              !!ctx.pendingApprovalToolCallId &&
              ctx.pendingApprovalToolCallId === msg.id,
          }
        );
      }

      // Task list tool (todo_list / task / todo): the schema is owned by
      // the agent crate (stable wire format), so we render a per-command
      // structured body instead of dumping the raw args JSON. Display name
      // overridden to `tasks` so it matches what {@link LiteTaskTray} and
      // /verbose call this surface — same word, two places, one mental
      // model. The output bar is suppressed entirely: the tray already
      // surfaces the authoritative tasks state, and the raw tool result
      // is a JSON dump of every task that would just duplicate it.
      //
      // Falls through to the generic path when the args don't parse or
      // the command is unrecognized, so a future schema change shows
      // SOMETHING in scrollback instead of a bare tool name.
      if (msg.name && TASK_TOOL_NAMES.has(msg.name)) {
        const taskBlock = formatTaskToolBody(
          msg.content,
          ctx.termCols,
          ctx.glyphs
        );
        if (taskBlock) {
          const reasoning = display.showToolReasoning
            ? extractToolReasoning(msg.content, msg.purpose)
            : undefined;
          const info: ToolCallRenderInfo = {
            // Override the wire name so users see "tasks" everywhere —
            // /verbose category is "task", the tray header says "tasks",
            // and the user's note "it's called task list, not todo list"
            // applies. The wire name (`todo_list`) is a legacy alias.
            name: 'tasks',
            status,
            description: reasoning,
            inlineArg: chalk.dim(taskBlock.command),
            agentPrefix,
            rejected: isRejected,
            elapsed:
              display.showElapsed && msg.startTime && msg.finishTime
                ? msg.finishTime - msg.startTime
                : undefined,
            runningSpinner: ctx.runningSpinner,
            awaitingApproval:
              !!ctx.pendingApprovalToolCallId &&
              ctx.pendingApprovalToolCallId === msg.id,
          };
          const header = renderToolCall(info, ctx.theme);
          // Honor `toolArgsMode === 'off'`: minimal density preset users
          // get the bare header line. Other modes both show the full body
          // — the structured task-list view is the whole point of the
          // special case, so collapsing it to a single arg chip in
          // `inline` mode (the way generic tools do) would defeat it.
          if (
            display.toolArgsMode === 'off' ||
            taskBlock.bodyLines.length === 0
          ) {
            return header;
          }
          // argsMaxLines bounds the body's visual height so a 50-task
          // create call doesn't dominate scrollback. The marker counts
          // dropped lines so the user knows there's more above the cap.
          const capped = applyLineCap(
            taskBlock.bodyLines,
            display.argsMaxLines,
            (n) => chalk.dim(`  ... (truncated; +${n} more lines)`)
          );
          const body = header + '\n' + capped.join('\n');
          // On success we suppress the verbose output bar — the
          // {@link LiteTaskTray} already surfaces the authoritative state
          // and the raw tool result is a JSON dump that just duplicates
          // the tray. On error, surface the error body so the user has
          // something to act on — without this, a TaskStore filesystem
          // failure would render only `tasks <cmd> FAILED` and leave the
          // user blind to the actual cause unless the agent's follow-up
          // prose happened to explain. Matches how write tools (also
          // diff-rendered, also tray-adjacent) handle errors.
          if (msg.result?.status !== 'error') return body;
          return (
            body +
            renderVerboseOutput(
              msg.name || '',
              msg.result,
              display.outputMaxLines,
              ctx.filtersOverride,
              display.outputMaxChars,
              ctx.termCols,
              ctx.glyphs
            )
          );
        }
        // taskBlock === null — args malformed; fall through to generic.
      }

      // Reasoning ("why") rendering is gated by display.showToolReasoning.
      // The slot only ever surfaces the agent's actual `__tool_use_purpose`
      // — never a synthesized one-liner from args. The previous block/off-
      // mode fallback (extractToolPurpose's args waterfall: command → path
      // → pattern → query) painted whatever string it found in brand purple,
      // which made every tool-call line look like the agent had reasoned
      // about the call. In block mode that string then duplicated the first
      // row of the args tree below it (path: foo.ts in purple, then path:
      // foo.ts in white). In off mode it lied about being reasoning at all.
      //
      // Symmetric across modes now: purple = real reasoning the agent gave
      // us. When the agent omits __tool_use_purpose, the slot stays empty
      // and the args block (block mode) or args chip (inline mode) speaks
      // for itself. Off mode without reasoning shows a bare tool name —
      // acceptable given off mode is opt-in minimalism.
      const inlineArg =
        display.toolArgsMode === 'inline'
          ? extractInlineArg(msg.name || '', msg.content, display.argsMaxChars)
          : undefined;
      const reasoning = display.showToolReasoning
        ? extractToolReasoning(msg.content, msg.purpose)
        : undefined;

      const info: ToolCallRenderInfo = {
        name: msg.name || 'unknown',
        status,
        description: reasoning,
        inlineArg,
        agentPrefix,
        rejected: isRejected,
        elapsed:
          display.showElapsed && msg.startTime && msg.finishTime
            ? msg.finishTime - msg.startTime
            : undefined,
        // Live region threads its spinner glyph here so a running tool's
        // chat-log row picks up motion. Static path leaves it unset and
        // gets ' ...' for the running slot, which is the correct
        // post-flush appearance for any settled row.
        runningSpinner: ctx.runningSpinner,
        // When this tool is the current pending-approval target, the
        // running-status slot flips to a yellow ' ...' instead of the
        // spinner. Truthful "waiting on you, not me" signal — the agent
        // isn't progressing while approval is pending, so painting the
        // braille spin would lie. Same trust color as the approval
        // prompt's [t] hotkey, so the eye links the tool body to the
        // prompt below the input as a single visual unit.
        awaitingApproval:
          !!ctx.pendingApprovalToolCallId &&
          ctx.pendingApprovalToolCallId === msg.id,
      };
      if (isWrite && msg.content) {
        // Suppress the diff body in two cases: (1) a pending approval is
        // active for this tool call (the diff is already shown above the
        // input as part of the approval prompt — duplicating it in
        // scrollback wastes vertical space), or (2) the user has turned
        // off the per-/verbosity "Write diffs" toggle. Both fall through
        // to renderToolCall's bare-header path; the row still appears in
        // scrollback so there's evidence the write fired. Errors still
        // surface via the renderVerboseOutput call below.
        const suppressDiff =
          (!!ctx.pendingApprovalToolCallId &&
            ctx.pendingApprovalToolCallId === msg.id) ||
          !display.showWriteDiffs;
        // Write tools always show their diff (the actionable preview),
        // including denied calls — the user just saw the diff on the
        // approval prompt above, so falling back to a raw key:value
        // args tree (`command:`, `path:`, `content:` rows) for the
        // post-deny scrollback row is jarring and reads worse than the
        // diff form. The DENIED status renders in the header line via
        // `info.rejected`. The block-args tree is suppressed in non-
        // block modes, but the diff itself is the point of the call so
        // we keep it.
        //
        // Write diffs render in full — they're the payload the user is
        // reviewing and materialize whole at finish time, so unlike read
        // tool bodies they deliberately opt out of the outputMaxLines
        // cap. The trailing success line that used to render via
        // renderVerboseOutput is gone — the diff already shows the
        // change happened, the literal
        // "Successfully created X (N lines)." that the agent returns
        // is just chrome noise. Errors still surface (renderVerboseOutput
        // bypasses the filter check on the error path), so a failed
        // write still gets its red bar block under the diff.
        const writeRender = renderWriteToolCall(info, msg.content, {
          suppressDiff,
          termCols: ctx.termCols,
          theme: ctx.theme,
        });
        if (msg.result?.status !== 'error') return writeRender;
        return (
          writeRender +
          renderVerboseOutput(
            msg.name || '',
            msg.result,
            display.outputMaxLines,
            ctx.filtersOverride,
            display.outputMaxChars,
            ctx.termCols,
            ctx.glyphs
          )
        );
      }
      // Read tools render the file body in a structured form (path header
      // + numbered, syntax-highlighted body) — same shape as write/diff so
      // scrollback reads consistently across reads and writes. The output
      // bar is skipped because the structured body already shows the file
      // content. Filter list still gates rendering: when read isn't in
      // the user's filters, fall through to the bare tool-call line.
      if (
        isRead &&
        msg.content &&
        !isRejected &&
        shouldShowToolOutput(msg.name || '', ctx.filtersOverride)
      ) {
        return renderReadToolCall(info, msg.content, msg.result, {
          termCols: ctx.termCols,
          maxLines: display.outputMaxLines,
          maxCharsPerLine: display.outputMaxChars,
          theme: ctx.theme,
          glyphs: ctx.glyphs,
        });
      }
      const toolLine = renderToolCall(info, ctx.theme);
      // Block mode: full key:value tree under the name. Inline + off: nothing
      // below the tool line — the chip on the same line is all the user gets.
      // Use the line-array form so we can cap visual rows before joining;
      // the array entries are already post-wrap, so each one is one visual
      // row in the terminal.
      if (display.toolArgsMode === 'block') {
        // P438130055 + follow-up: pass perValueLineCap=null so the
        // block-level applyLineCap below is the single source of truth
        // for capping in block mode.
        //
        // Earlier iteration kept a hardcoded 5-line per-value cap when
        // argsMaxLines was finite, intending to give multi-arg tools
        // fairness ("one noisy multi-line value can't dominate the
        // bounded block"). The cost was a confusing marker count: the
        // per-value clamp emits its own "(+N more lines)" marker, then
        // applyLineCap chops that marker off as one row, so the block-
        // level marker reports "+1 more lines" while dozens of source
        // lines are actually hidden.
        //
        // Block-mode rendering is now deterministic — the user's
        // argsMaxLines is the only cap that fires, and its marker
        // counts visual rows that ARE source lines (since
        // formatArgLines no longer collapses values internally). The
        // unlimited-toggle fix from P438130055 still works the same:
        // null cap, no truncation anywhere.
        const argsLines = formatToolArgLines(
          msg.name || '',
          msg.content,
          undefined,
          display.argsMaxChars,
          null
        );
        if (argsLines && argsLines.length > 0) {
          const capped = applyLineCap(argsLines, display.argsMaxLines, (n) =>
            chalk.dim(`  ... (truncated; +${n} more lines)`)
          );
          return (
            toolLine +
            '\n' +
            capped.join('\n') +
            renderVerboseOutput(
              msg.name || '',
              msg.result,
              display.outputMaxLines,
              ctx.filtersOverride,
              display.outputMaxChars,
              ctx.termCols,
              ctx.glyphs
            )
          );
        }
      }
      return (
        toolLine +
        renderVerboseOutput(
          msg.name || '',
          msg.result,
          display.outputMaxLines,
          ctx.filtersOverride,
          display.outputMaxChars,
          ctx.termCols,
          ctx.glyphs
        )
      );
    }

    case 'system':
      return msg.success !== false
        ? renderSystemInfo(msg.content)
        : renderSystemError(msg.content);

    default:
      return '';
  }
}

/**
 * Which fixture-set the /verbosity menu wants previewed. Each preview pane is
 * sized for one specific submenu's knobs:
 *
 *   `top` / `density` — generic mix (shell + read + finished subagent).
 *   `tool`           — same generic mix; emphasizes args / reasoning / elapsed.
 *   `subagent`       — pipeline+responses-rich subagent so subagent toggles
 *                      reshape it visibly.
 *   `output`         — generic mix; bars appear/vanish per filter list.
 *   `truncation:args`   — single tool with a 50-key args fixture.
 *   `truncation:output` — single tool with a 60-line output fixture.
 *
 * Each fixture is module-level so the preview is cheap and never allocates
 * lazily during a render frame.
 */
export type VerbosityPreviewKey =
  | 'top'
  | 'density'
  | 'tool'
  | 'subagent'
  | 'output'
  | 'truncation:args'
  | 'truncation:output';

const PREVIEW_FIXTURE_SHELL: MessageLike = {
  id: 'preview-shell',
  role: 'tool_use',
  name: 'shell',
  content: JSON.stringify({
    command: 'git status',
    __tool_use_purpose: 'check working tree state before commit',
  }),
  result: {
    status: 'success',
    output: [
      'On branch feature/lite-tui-mode',
      'Changes not staged for commit:',
      '  (use "git add <file>..." to update what will be committed)',
      '  (use "git restore <file>..." to discard changes in working directory)',
      '\tmodified:   packages/tui/src/lite/render.ts',
      '\tmodified:   packages/tui/src/commands/effects.ts',
      '',
      'no changes added to commit (use "git add" and/or "git commit -a")',
    ].join('\n'),
  },
  startTime: 0,
  finishTime: 1240,
  isFinished: true,
};

const PREVIEW_FIXTURE_READ: MessageLike = {
  id: 'preview-read',
  role: 'tool_use',
  name: 'fs_read',
  content: JSON.stringify({
    operations: [{ path: '/etc/hosts', limit: 50 }],
    __tool_use_purpose: 'inspect hostnames for the dev cluster',
  }),
  result: {
    status: 'success',
    output: [
      '127.0.0.1 localhost',
      '::1       localhost',
      '127.0.1.1 cloud-desktop',
    ].join('\n'),
  },
  startTime: 0,
  finishTime: 18,
  isFinished: true,
};

/** Grep fixture — long pattern argument so the preview demonstrates how
 *  argsMaxChars clips a single value without affecting other rows. */
const PREVIEW_FIXTURE_GREP: MessageLike = {
  id: 'preview-grep',
  role: 'tool_use',
  name: 'grep',
  content: JSON.stringify({
    pattern: 'legacy_auth_middleware|legacyAuthMiddleware|LegacyAuthMiddleware',
    path: 'packages/tui/src',
    __tool_use_purpose: 'enumerate every flavor of the legacy middleware name',
  }),
  result: {
    status: 'success',
    output: [
      'packages/tui/src/api/auth/legacy.ts:42:    legacy_auth_middleware,',
      'packages/tui/src/edge/handlers/login.ts:15:  wrap(legacyAuthMiddleware)',
      'packages/tui/src/middleware/legacy.ts:1:export const legacy_auth_middleware = (',
      'packages/tui/src/scripts/migrate.ts:88:// references legacyAuthMiddleware',
    ].join('\n'),
  },
  startTime: 0,
  finishTime: 96,
  isFinished: true,
};

/** Long-output fixture (12 lines) so outputMaxLines=5 is visibly clipped
 *  without needing the dedicated truncation:output fixture. Shell tool, so
 *  it shares the most common filter category. */
const PREVIEW_FIXTURE_LONG_OUTPUT: MessageLike = {
  id: 'preview-long',
  role: 'tool_use',
  name: 'shell',
  content: JSON.stringify({
    command: 'cat package.json',
    __tool_use_purpose: 'inspect dependencies',
  }),
  result: {
    status: 'success',
    output: [
      '{',
      '  "name": "@kiro/tui",',
      '  "version": "0.1.0",',
      '  "type": "module",',
      '  "scripts": {',
      '    "build": "tsc -b",',
      '    "test": "vitest run",',
      '    "lint": "eslint src"',
      '  },',
      '  "dependencies": {',
      '    "ink": "^4.0.0",',
      '    "react": "^18.2.0"',
      '  }',
      '}',
    ].join('\n'),
  },
  startTime: 0,
  finishTime: 32,
  isFinished: true,
};

/** MCP-routed fixture — name is `mcp__*` so the `mcp` filter category gates
 *  it. Useful for showing how filter choices reshape the preview. */
const PREVIEW_FIXTURE_MCP: MessageLike = {
  id: 'preview-mcp',
  role: 'tool_use',
  name: 'mcp__nova-memory-mcp__recall',
  content: JSON.stringify({
    query: 'legacy auth middleware migration',
    __tool_use_purpose: 'check prior context for the migration plan',
  }),
  result: {
    status: 'success',
    output: [
      '3 memories matched:',
      '  · 2026-04-02 — legacy_auth_middleware deprecation announcement',
      '  · 2026-04-15 — session-token store rollout plan',
      '  · 2026-05-01 — compliance review of token storage',
    ].join('\n'),
  },
  startTime: 0,
  finishTime: 240,
  isFinished: true,
};

/** Write fixture — exercises the diff renderer so users can see what
 *  write-tool calls look like under different verbosity settings. */
const PREVIEW_FIXTURE_WRITE: MessageLike = {
  id: 'preview-write',
  role: 'tool_use',
  name: 'fs_write',
  content: JSON.stringify({
    command: 'str_replace',
    path: 'packages/tui/src/middleware/legacy.ts',
    old_str: 'export const legacy_auth_middleware = (req, res, next) => {',
    new_str: 'export const legacyAuthMiddleware = (req, res, next) => {',
    __tool_use_purpose: 'rename the legacy middleware export to camelCase',
  }),
  result: { status: 'success', output: '' },
  startTime: 0,
  finishTime: 64,
  isFinished: true,
};

/** Agent-message fixture — demonstrates how reasoning + plain prose render
 *  alongside tool calls. Without it the preview is wall-to-wall tool blocks. */
const PREVIEW_FIXTURE_AGENT: MessageLike = {
  id: 'preview-agent',
  role: 'model',
  content:
    "Found four call sites for the legacy middleware. I'll rename the export to camelCase, then fix the import sites in order: edge handler, API auth, migration script.",
};

const PREVIEW_SUBAGENT_CONTENT = JSON.stringify({
  task: 'find every place the legacy auth middleware is wired up',
  stages: [
    {
      name: 'scan',
      role: 'searcher',
      prompt_template:
        'Search the codebase for references to legacy_auth_middleware. Return a list of file:line locations.',
      depends_on: [],
    },
    {
      name: 'summarize',
      role: 'synthesizer',
      prompt_template:
        'Given the scan results, group call sites by component and summarize the migration impact.',
      depends_on: ['scan'],
    },
  ],
});

const PREVIEW_SUBAGENT_SUMMARIES: SubagentStageSummary[] = [
  {
    stageName: 'scan',
    contextSummary:
      '4 call sites: api/auth/, edge/handlers/, middleware/legacy.ts, scripts/migrate.ts',
    taskResult: [
      'api/auth/legacy.ts:42 — imports legacy_auth_middleware',
      'edge/handlers/login.ts:15 — wraps the login route',
      'middleware/legacy.ts:1 — defines the export',
      'scripts/migrate.ts:88 — references it for migration metadata',
    ].join('\n'),
  },
  {
    stageName: 'summarize',
    contextSummary:
      'Three production paths (API, edge, scripts). Migration unblocks the new session store.',
    taskResult:
      'Three production paths use legacy_auth_middleware: the public-facing API, the edge login handler, and the offline migration script. All three need updating before the new session-token store can ship.',
  },
];

const PREVIEW_FIXTURE_SUBAGENT: MessageLike = {
  id: 'preview-subagent',
  role: 'tool_use',
  name: 'subagent',
  content: PREVIEW_SUBAGENT_CONTENT,
  result: { status: 'success', output: '' },
  startTime: 0,
  finishTime: 4200,
  isFinished: true,
};

const PREVIEW_FIXTURE_USER: MessageLike = {
  id: 'preview-user',
  role: 'user',
  content: 'find the legacy auth middleware',
};

/** Build a 50-key args fixture for the truncation:args preview. The block-args
 *  renderer emits one visual row per key, so 50 keys produces ~50 rows under
 *  the tool name and a tight cap clips visibly. */
function buildTruncationArgsFixture(): MessageLike {
  const args: Record<string, unknown> = {
    __tool_use_purpose: 'demo a tool with many args',
  };
  for (let i = 1; i <= 50; i++) {
    args[`key_${String(i).padStart(2, '0')}`] = `value-${i}`;
  }
  return {
    id: 'preview-trunc-args',
    role: 'tool_use',
    name: 'mcp__demo__many_args',
    content: JSON.stringify(args),
    result: { status: 'success', output: '' },
    startTime: 0,
    finishTime: 50,
    isFinished: true,
  };
}

/** Build a 60-line output fixture for the truncation:output preview. The
 *  tool name is chosen so the fixture's output bar always renders under
 *  the caller-provided filter list — when the user has narrowed filters
 *  (e.g. only `read` enabled), we pick a tool from one of the enabled
 *  categories so the cap demo uses a tool they actually see in real
 *  scrollback. Falls back to `shell` when no narrow filters apply. */
function buildTruncationOutputFixture(
  filters: readonly string[] = ['all']
): MessageLike {
  const lines: string[] = [];
  for (let i = 1; i <= 60; i++) {
    lines.push(
      `line ${String(i).padStart(2, '0')}: lorem ipsum dolor sit amet`
    );
  }
  // Pick a tool name from the user's enabled categories so the fixture
  // matches a tool they'd see for real. Order of preference: shell (most
  // common) → read → grep → mcp. If filters is `['all']` or empty, default
  // to shell; we'll widen filters later for empty.
  const isAll = filters.includes('all');
  const choose = (): { name: string; command: string; purpose: string } => {
    if (isAll || filters.includes('shell')) {
      return {
        name: 'shell',
        command: 'cat fixture.txt',
        purpose: 'demo a tool with long output',
      };
    }
    if (filters.includes('read')) {
      return {
        name: 'fs_read',
        command: '',
        purpose: 'demo a long file read',
      };
    }
    if (filters.includes('grep')) {
      return {
        name: 'grep',
        command: '',
        purpose: 'demo a grep with many matches',
      };
    }
    if (filters.includes('mcp')) {
      return {
        name: 'mcp__demo__long-output',
        command: '',
        purpose: 'demo an MCP tool with long output',
      };
    }
    // Fall back to shell — the widening logic in renderVerbosityPreview
    // will add the shell category to the override list so the bar shows.
    return {
      name: 'shell',
      command: 'cat fixture.txt',
      purpose: 'demo a tool with long output',
    };
  };
  const pick = choose();
  // fs_read uses the `operations: [{ path }]` shape; everything else takes
  // a generic command/query.
  const content =
    pick.name === 'fs_read'
      ? JSON.stringify({
          operations: [{ path: '/tmp/fixture.txt' }],
          __tool_use_purpose: pick.purpose,
        })
      : pick.name.startsWith('mcp__')
        ? JSON.stringify({
            query: 'fixture',
            __tool_use_purpose: pick.purpose,
          })
        : pick.name === 'grep'
          ? JSON.stringify({
              pattern: 'fixture',
              path: '.',
              __tool_use_purpose: pick.purpose,
            })
          : JSON.stringify({
              command: pick.command,
              __tool_use_purpose: pick.purpose,
            });
  return {
    id: 'preview-trunc-output',
    role: 'tool_use',
    name: pick.name,
    content,
    result: { status: 'success', output: lines.join('\n') },
    startTime: 0,
    finishTime: 80,
    isFinished: true,
  };
}

/**
 * Section-spacing rule for preview rendering. Delegates to the shared
 * {@link needsLeadingBlankByRole} so this preview layer stays in lockstep
 * with the chat log + live region without a hand-sync comment.
 */
function previewNeedsLeadingBlank(
  prev: MessageLike,
  next: MessageLike
): boolean {
  return needsLeadingBlankByRole(prev.role, next.role);
}

/**
 * Render a synthetic example of what scrollback looks like with the given
 * display config and filter list. Pure — no disk I/O, no shared state.
 *
 * `display` and `filters` are taken as-is so the menu can pass an in-progress
 * draft (e.g. the current cap value being edited) without writing to disk.
 *
 * The output is capped at MAX_PREVIEW_ROWS visual rows so the pane doesn't
 * eat the whole screen on short terminals. Truncation is tail-side with a
 * dim marker — the eye stays anchored at the top so the visual diff between
 * two settings doesn't move.
 *
 * Truncation:output specifically: when the caller wants to preview an
 * in-progress cap value (different from `display.outputMaxLines`), pass the
 * draft cap as the optional `outputCapOverride` argument; the fixture is
 * rendered with that cap instead. Same idea for args.
 */
export function renderVerbosityPreview(
  key: VerbosityPreviewKey,
  display: VerboseDisplayConfig,
  filters: readonly string[],
  options: { expanded?: boolean; theme?: RenderTheme } = {}
): string {
  // For the truncation:output preview, pick the fixture tool first (based
  // on the user's enabled categories) so we widen filters only when needed.
  // When the user has at least one filter that already covers the chosen
  // tool, leave their filters alone — the cap demo then matches what they'd
  // see in real scrollback.
  let outputFixture: MessageLike | null = null;
  if (key === 'truncation:output') {
    outputFixture = buildTruncationOutputFixture(filters);
  }
  const previewFilters: readonly string[] =
    key === 'truncation:output' && outputFixture
      ? widenFiltersForPreview(filters, outputFixture.name ?? 'shell')
      : filters;

  const ctx: RenderContext = {
    display,
    filtersOverride: previewFilters,
    subagentSummariesById: new Map([
      [PREVIEW_FIXTURE_SUBAGENT.id, PREVIEW_SUBAGENT_SUMMARIES],
    ]),
    theme: options.theme,
  };

  const messages: MessageLike[] = [];
  switch (key) {
    case 'top':
    case 'density':
    case 'tool': {
      // Generic mix — short and long outputs, write w/ diff, MCP, agent
      // message — so the user sees one of each kind they'll actually
      // encounter. Order: user → quick read → write → grep → MCP → long
      // shell → agent message → subagent.
      messages.push(
        PREVIEW_FIXTURE_USER,
        PREVIEW_FIXTURE_READ,
        PREVIEW_FIXTURE_WRITE,
        PREVIEW_FIXTURE_GREP,
        PREVIEW_FIXTURE_MCP,
        PREVIEW_FIXTURE_LONG_OUTPUT,
        PREVIEW_FIXTURE_AGENT,
        PREVIEW_FIXTURE_SUBAGENT
      );
      break;
    }
    case 'output':
      // The output submenu's whole job is to show what filters do — we want
      // a tool from each major category present so toggles produce visible
      // changes. Keeps the agent message + subagent so the picture is whole.
      messages.push(
        PREVIEW_FIXTURE_USER,
        PREVIEW_FIXTURE_SHELL,
        PREVIEW_FIXTURE_READ,
        PREVIEW_FIXTURE_GREP,
        PREVIEW_FIXTURE_MCP,
        PREVIEW_FIXTURE_AGENT,
        PREVIEW_FIXTURE_SUBAGENT
      );
      break;
    case 'subagent':
      messages.push(PREVIEW_FIXTURE_USER, PREVIEW_FIXTURE_SUBAGENT);
      break;
    case 'truncation:args':
      // Pair the synthetic 50-key fixture with a representative real tool
      // so the user sees how the cap interacts with realistic args (the
      // shell command, the grep pattern) rather than only synthetic key_NN
      // pairs. Realistic tools come first so the cap impact is the first
      // thing the eye lands on.
      messages.push(
        PREVIEW_FIXTURE_GREP,
        PREVIEW_FIXTURE_SHELL,
        buildTruncationArgsFixture()
      );
      break;
    case 'truncation:output':
      // Reuse the fixture computed above so the widened previewFilters and
      // the rendered fixture stay aligned (avoids picking 'fs_read' but
      // widening for 'shell').
      if (outputFixture) messages.push(outputFixture);
      break;
  }

  const blocks: string[] = [];

  // Hint when we had to widen filters to make the fixture's bar render.
  // Without this, a user with empty filters would assume the cap "doesn't
  // work" because the bar happens to not surface in their normal scrollback
  // either. We only nudge when the widening actually changed something —
  // when the user's filters already cover the chosen tool, the cap demo
  // mirrors their real behavior and no nudge is needed.
  if (
    key === 'truncation:output' &&
    outputFixture &&
    !shouldShowToolOutput(outputFixture.name ?? 'shell', filters)
  ) {
    blocks.push(
      chalk.dim(
        `(preview-only: your filters hide output for ${outputFixture.name}. Enable it in /verbosity → Show output to see this cap in real scrollback.)`
      )
    );
  }

  let prevMsg: MessageLike | null = null;
  for (const msg of messages) {
    const text = renderMessageToText(msg, 'Kiro', ctx);
    if (!text) continue;
    // Mirror the chat log's section-spacing rules so the preview shows the
    // same visual rhythm the user gets in real scrollback. See the
    // {@link needsLeadingBlank} helper for the canonical rules; we inline a
    // copy here to avoid a layer-crossing import (this module sits below
    // the components/layout/lite folder).
    const blank = prevMsg ? previewNeedsLeadingBlank(prevMsg, msg) : false;
    blocks.push(blank ? `\n${text}` : text);
    prevMsg = msg;
  }
  const joined = blocks.join('\n');
  // Expanded mode skips the row clip — the pane viewer paginates with its
  // own scroll offset, so cutting at 16 rows would defeat the point of
  // the expanded view.
  if (options.expanded) return joined;
  const lines = joined.split('\n');
  const MAX_PREVIEW_ROWS = 16;
  if (lines.length <= MAX_PREVIEW_ROWS) return joined;
  const head = lines.slice(0, MAX_PREVIEW_ROWS);
  head.push(
    chalk.dim(
      `… (preview clipped, +${lines.length - MAX_PREVIEW_ROWS} more rows)`
    )
  );
  return head.join('\n');
}

/**
 * Widen the user's filter list so the synthetic preview can demonstrate a
 * cap that depends on the output bar being visible. We don't want to mutate
 * the saved config — this only runs when rendering the truncation:output
 * fixture. The widened list is the user's own list with the fixture's
 * matching category added (or `['all']` if the user already has 'all').
 */
function widenFiltersForPreview(
  filters: readonly string[],
  needTool: string
): readonly string[] {
  if (filters.includes('all')) return filters;
  if (shouldShowToolOutput(needTool, filters)) return filters;
  // Add the matching category so the fixture's output renders. Using the
  // category (rather than the exact tool name) means a `shell` widen also
  // covers any other shell-categorized tools the fixture set might gain.
  const cat = categorize(needTool);
  if (cat == null) return [...filters, needTool];
  return [...filters, cat];
}

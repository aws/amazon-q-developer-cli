import chalk from 'chalk';
import {
  getVerboseDisplay,
  shouldShowToolOutput,
  categorize,
  type VerboseDisplayConfig,
} from '../verbose.js';
import {
  TASK_TOOL_NAMES,
  isParentSubagentTool,
} from '../../types/agent-events.js';
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
  // Only red the first line's `error:` prefix — wrapping the rest in chalk.red
  // would clobber embedded chalk codes on per-item lines below the header.
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
  /** `'rejected'` when the user denied the call. Kept as `string` (like
   *  result.status) so the store's MessageType stays structurally assignable
   *  without importing its enum. Distinct from result.status 'error'/
   *  'cancelled' — a rejected call may carry no `result` at all. */
  status?: string;
  success?: boolean;
  standalone?: boolean;
  agentName?: string;
  startTime?: number;
  finishTime?: number;
  /** Model's freeform reasoning, shown above its reply when
   *  showThinkingContent is on. Distinct from `purpose` (per-tool-call why). */
  thinking?: string;
  /** Per-tool "why" preserved verbatim from `__tool_use_purpose`, captured at
   *  the ACP boundary before per-shape synthesis rebuilds `content` and drops
   *  it for edit-kind tools. Primary source for extractToolReasoning. */
  purpose?: string;
  /** `true` when this Model message carries `!` shell-escape output, not agent
   *  inference — routes to renderShellOutputBlock (`! ` gutter) and bypasses
   *  markdown so shell `*`/`_` aren't styled. */
  shellOutput?: boolean;
}

export interface SubagentStageSummary {
  stageName: string;
  /** Compressed digest from the stage's `summary` tool call, harvested off
   *  the inner message (the agent_crew joiner discards it before the parent's
   *  combined output). May be empty — render falls back to taskResult. */
  contextSummary: string;
  /** Fallback when contextSummary is empty. */
  taskResult: string;
}

export interface RenderContext {
  /** Tool-call awaiting approval; its scrollback entry suppresses the diff
   *  (already shown in the approval prompt above). */
  pendingApprovalToolCallId?: string | null;
  termCols?: number;
  /** Per-invocation stage summaries keyed by parent `subagent` tool id;
   *  feeds renderSubagentFinalBlock's compact "Summary of findings". */
  subagentSummariesById?: Map<string, SubagentStageSummary[]>;
  /** Stage → per-agent input color for `[stage]` chips; neutral fallback. */
  getStageInputColor?: (stageName: string) => (text: string) => string;
  /** Brighter shade of the same color for `▸ stage` response chips. */
  getStageOutputColor?: (stageName: string) => (text: string) => string;
  /** /verbose display knobs; read from disk when omitted (tests override). */
  display?: VerboseDisplayConfig;
  /** Filter override for shouldShowToolOutput; the /verbosity preview passes
   *  a draft list so toggles reflect without touching saved config. */
  filtersOverride?: readonly string[];
  /** /theme accessors; falls back to kiroDark defaults for pure contexts. */
  theme?: RenderTheme;
  /** Agent name → footer chip color, so the role tag matches the footer. */
  getAgentTagColor?: (agentName: string) => (text: string) => string;
  /** Spinner glyph for the running status slot. Set only on the live path;
   *  left unset on the static path so flushed rows show settled status, not a
   *  frozen spinner. */
  runningSpinner?: string;
  /** Active glyph set (Unicode/ASCII); defaults to UNICODE_GLYPHS. */
  glyphs?: Glyphs;
}

/** Render any message type to a plain text string for Static output. */
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
      // Shell-escape output bypasses the agent/thinking/markdown path: no
      // `Kiro:` tag, no thinking block, and no markdown (raw bash output's
      // `*`/`_` would otherwise render as italic).
      if (msg.shellOutput) {
        return renderShellOutputBlock(msg.content, ctx.theme, ctx.termCols);
      }
      // Prefer the message's own agentName (subagent stages); fall back to main.
      const display = ctx.display ?? getVerboseDisplay();
      const agentText = renderAgentMessage(
        msg.content,
        msg.agentName ?? mainAgentName,
        ctx.theme,
        ctx.termCols,
        ctx.getAgentTagColor,
        ctx.glyphs
      );
      // Thinking block above the spoken text, gated by showThinkingContent.
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
      // Blank row so the bottom rule doesn't glue to the `Kiro:` line.
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

      // Subagent tool: one canonical block per pipeline run, shown in full
      // (it's what the parent agent sees). Per-stage tool calls are hidden
      // from the chat log (they live in the footer activity strip).
      if (isParentSubagentTool(msg.name)) {
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
            runningSpinner: ctx.runningSpinner,
            // Fires only when the parent subagent tool itself awaits approval;
            // stage approvals go through the footer activity strip instead.
            awaitingApproval:
              !!ctx.pendingApprovalToolCallId &&
              ctx.pendingApprovalToolCallId === msg.id,
          }
        );
      }

      // Task list tool (todo_list / task / todo): render a per-command
      // structured body instead of the raw args JSON; display name → `tasks`
      // to match the tray + /verbose. Falls through to generic on malformed
      // args so a schema change still shows something.
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
            // Override the wire name (legacy alias `todo_list`) → "tasks".
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
          // `off` mode gets the bare header; other modes show the full body
          // (the structured task view is the point — don't collapse to a chip).
          if (
            display.toolArgsMode === 'off' ||
            taskBlock.bodyLines.length === 0
          ) {
            return header;
          }
          // argsMaxLines bounds body height; marker counts dropped lines.
          const capped = applyLineCap(
            taskBlock.bodyLines,
            display.argsMaxLines,
            (n) => chalk.dim(`  ... (truncated; +${n} more lines)`)
          );
          const body = header + '\n' + capped.join('\n');
          // Suppress the output bar on success (the tray is authoritative);
          // on error, surface the body so the user sees the cause.
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

      // Reasoning slot (gated by showToolReasoning) only ever surfaces the
      // agent's real `__tool_use_purpose`, never a synthesized args one-liner
      // — so purple always means "the agent reasoned about this call".
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
        runningSpinner: ctx.runningSpinner,
        // Pending-approval target: running slot flips to a yellow ' ...'
        // (the agent isn't progressing, so the spinner would lie).
        awaitingApproval:
          !!ctx.pendingApprovalToolCallId &&
          ctx.pendingApprovalToolCallId === msg.id,
      };
      if (isWrite && msg.content) {
        // Suppress the diff when this call is awaiting approval (already shown
        // in the prompt) or the user turned off the Write-diffs toggle; the
        // header row still records that the write fired.
        const suppressDiff =
          (!!ctx.pendingApprovalToolCallId &&
            ctx.pendingApprovalToolCallId === msg.id) ||
          !display.showWriteDiffs;
        // Write diffs render in full (the payload being reviewed), including
        // denied calls — falling back to a raw args tree post-deny reads
        // worse than the diff. No trailing success line; errors still surface
        // below via renderVerboseOutput (which bypasses the filter check).
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
      // Read tools render a structured body (path header + numbered,
      // highlighted lines); the output bar is skipped (body shows content).
      // Filters still gate it — fall through to the bare line when read isn't enabled.
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
      // Block mode: full key:value tree under the name. Inline/off: nothing
      // (the chip is all the user gets).
      if (display.toolArgsMode === 'block') {
        // perValueLineCap=null so the block-level applyLineCap below is the
        // single cap (P438130055): a per-value clamp would emit its own
        // marker that applyLineCap then miscounts as one row.
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

/** Which fixture-set the /verbosity menu wants previewed (one per submenu). */
export type VerbosityPreviewKey =
  | 'top'
  | 'density'
  | 'tool'
  | 'subagent'
  | 'output'
  | 'truncation:args'
  | 'truncation:output';

/** Build a finished, successful tool-use preview fixture, hoisting the shared
 *  role/startTime/isFinished/result-envelope boilerplate the literals repeat. */
function toolFixture(f: {
  id: string;
  name: string;
  content: Record<string, unknown>;
  output?: string;
  finishTime: number;
}): MessageLike {
  return {
    id: f.id,
    role: 'tool_use',
    name: f.name,
    content: JSON.stringify(f.content),
    result: { status: 'success', output: f.output ?? '' },
    startTime: 0,
    finishTime: f.finishTime,
    isFinished: true,
  };
}

const PREVIEW_FIXTURE_SHELL = toolFixture({
  id: 'preview-shell',
  name: 'shell',
  content: {
    command: 'git status',
    __tool_use_purpose: 'check working tree state before commit',
  },
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
  finishTime: 1240,
});

const PREVIEW_FIXTURE_READ = toolFixture({
  id: 'preview-read',
  name: 'fs_read',
  content: {
    operations: [{ path: '/etc/hosts', limit: 50 }],
    __tool_use_purpose: 'inspect hostnames for the dev cluster',
  },
  output: [
    '127.0.0.1 localhost',
    '::1       localhost',
    '127.0.1.1 cloud-desktop',
  ].join('\n'),
  finishTime: 18,
});

/** Grep fixture — long pattern argument so the preview demonstrates how
 *  argsMaxChars clips a single value without affecting other rows. */
const PREVIEW_FIXTURE_GREP = toolFixture({
  id: 'preview-grep',
  name: 'grep',
  content: {
    pattern: 'legacy_auth_middleware|legacyAuthMiddleware|LegacyAuthMiddleware',
    path: 'packages/tui/src',
    __tool_use_purpose: 'enumerate every flavor of the legacy middleware name',
  },
  output: [
    'packages/tui/src/api/auth/legacy.ts:42:    legacy_auth_middleware,',
    'packages/tui/src/edge/handlers/login.ts:15:  wrap(legacyAuthMiddleware)',
    'packages/tui/src/middleware/legacy.ts:1:export const legacy_auth_middleware = (',
    'packages/tui/src/scripts/migrate.ts:88:// references legacyAuthMiddleware',
  ].join('\n'),
  finishTime: 96,
});

/** Long-output (12-line) shell fixture so outputMaxLines=5 is visibly clipped. */
const PREVIEW_FIXTURE_LONG_OUTPUT = toolFixture({
  id: 'preview-long',
  name: 'shell',
  content: {
    command: 'cat package.json',
    __tool_use_purpose: 'inspect dependencies',
  },
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
  finishTime: 32,
});

/** MCP-routed fixture — name is `mcp__*` so the `mcp` filter category gates
 *  it. Useful for showing how filter choices reshape the preview. */
const PREVIEW_FIXTURE_MCP = toolFixture({
  id: 'preview-mcp',
  name: 'mcp__nova-memory-mcp__recall',
  content: {
    query: 'legacy auth middleware migration',
    __tool_use_purpose: 'check prior context for the migration plan',
  },
  output: [
    '3 memories matched:',
    '  · 2026-04-02 — legacy_auth_middleware deprecation announcement',
    '  · 2026-04-15 — session-token store rollout plan',
    '  · 2026-05-01 — compliance review of token storage',
  ].join('\n'),
  finishTime: 240,
});

/** Write fixture — exercises the diff renderer so users can see what
 *  write-tool calls look like under different verbosity settings. */
const PREVIEW_FIXTURE_WRITE = toolFixture({
  id: 'preview-write',
  name: 'fs_write',
  content: {
    command: 'str_replace',
    path: 'packages/tui/src/middleware/legacy.ts',
    old_str: 'export const legacy_auth_middleware = (req, res, next) => {',
    new_str: 'export const legacyAuthMiddleware = (req, res, next) => {',
    __tool_use_purpose: 'rename the legacy middleware export to camelCase',
  },
  finishTime: 64,
});

/** Agent prose fixture so the preview isn't wall-to-wall tool blocks. */
const PREVIEW_FIXTURE_AGENT: MessageLike = {
  id: 'preview-agent',
  role: 'model',
  content:
    "Found four call sites for the legacy middleware. I'll rename the export to camelCase, then fix the import sites in order: edge handler, API auth, migration script.",
};

const PREVIEW_SUBAGENT_CONTENT = {
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
};

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

const PREVIEW_FIXTURE_SUBAGENT = toolFixture({
  id: 'preview-subagent',
  name: 'subagent',
  content: PREVIEW_SUBAGENT_CONTENT,
  finishTime: 4200,
});

const PREVIEW_FIXTURE_USER: MessageLike = {
  id: 'preview-user',
  role: 'user',
  content: 'find the legacy auth middleware',
};

/** 50-key args fixture for truncation:args (one row per key → tight cap clips). */
function buildTruncationArgsFixture(): MessageLike {
  const args: Record<string, unknown> = {
    __tool_use_purpose: 'demo a tool with many args',
  };
  for (let i = 1; i <= 50; i++) {
    args[`key_${String(i).padStart(2, '0')}`] = `value-${i}`;
  }
  return toolFixture({
    id: 'preview-trunc-args',
    name: 'mcp__demo__many_args',
    content: args,
    finishTime: 50,
  });
}

/** 60-line output fixture for truncation:output. Picks a tool from the user's
 *  enabled categories (shell→read→grep→mcp) so the cap demo uses a tool they
 *  actually see; falls back to shell. */
function buildTruncationOutputFixture(
  filters: readonly string[] = ['all']
): MessageLike {
  const lines: string[] = [];
  for (let i = 1; i <= 60; i++) {
    lines.push(
      `line ${String(i).padStart(2, '0')}: lorem ipsum dolor sit amet`
    );
  }
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
  // fs_read uses operations:[{path}]; others take a generic command/query.
  const content: Record<string, unknown> =
    pick.name === 'fs_read'
      ? {
          operations: [{ path: '/tmp/fixture.txt' }],
          __tool_use_purpose: pick.purpose,
        }
      : pick.name.startsWith('mcp__')
        ? { query: 'fixture', __tool_use_purpose: pick.purpose }
        : pick.name === 'grep'
          ? { pattern: 'fixture', path: '.', __tool_use_purpose: pick.purpose }
          : { command: pick.command, __tool_use_purpose: pick.purpose };
  return toolFixture({
    id: 'preview-trunc-output',
    name: pick.name,
    content,
    output: lines.join('\n'),
    finishTime: 80,
  });
}

/** Section-spacing for preview rendering; delegates to needsLeadingBlankByRole. */
function previewNeedsLeadingBlank(
  prev: MessageLike,
  next: MessageLike
): boolean {
  return needsLeadingBlankByRole(prev.role, next.role);
}

/**
 * Render a synthetic scrollback example for the given display/filter draft.
 * Pure (no disk I/O); takes args as-is so the menu can pass an in-progress
 * draft. Tail-clipped to MAX_PREVIEW_ROWS (anchored at top so the diff between
 * two settings doesn't shift) unless `expanded`.
 */
export function renderVerbosityPreview(
  key: VerbosityPreviewKey,
  display: VerboseDisplayConfig,
  filters: readonly string[],
  options: { expanded?: boolean; theme?: RenderTheme } = {}
): string {
  // truncation:output: pick the fixture tool first, then widen filters only
  // if the user's filters don't already cover it.
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
      // Generic mix — one of each kind the user will encounter.
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
      // One tool per category so filter toggles produce visible changes.
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
      // Real tools first, then the synthetic 50-key fixture, so the cap
      // impact is what the eye lands on.
      messages.push(
        PREVIEW_FIXTURE_GREP,
        PREVIEW_FIXTURE_SHELL,
        buildTruncationArgsFixture()
      );
      break;
    case 'truncation:output':
      // Reuse the fixture from above so previewFilters stays aligned.
      if (outputFixture) messages.push(outputFixture);
      break;
  }

  const blocks: string[] = [];

  // Nudge only when we actually widened the user's filters to show the bar.
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
    const blank = prevMsg ? previewNeedsLeadingBlank(prevMsg, msg) : false;
    blocks.push(blank ? `\n${text}` : text);
    prevMsg = msg;
  }
  const joined = blocks.join('\n');
  // Expanded mode skips the clip — the pane viewer paginates itself.
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
 * Add the fixture's category to a copy of the user's filters so the
 * truncation:output bar renders (never mutates saved config).
 */
function widenFiltersForPreview(
  filters: readonly string[],
  needTool: string
): readonly string[] {
  if (filters.includes('all')) return filters;
  if (shouldShowToolOutput(needTool, filters)) return filters;
  // Widen by category so other tools in that category are covered too.
  const cat = categorize(needTool);
  if (cat == null) return [...filters, needTool];
  return [...filters, cat];
}

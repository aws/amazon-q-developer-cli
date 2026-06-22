/**
 * LiteLiveRegion: active tool calls + streaming content + thinking. All running
 * tools shown at once; thinking timer resets per batch; streaming content is
 * not height-bounded (terminal scrolls naturally).
 */
import React, { useState, useEffect, useRef, useMemo } from 'react';
import { Box, Text } from '../../../renderer.js';
import {
  useAppStore,
  MessageRole,
  type MessageType,
} from '../../../stores/app-store.js';
import { usePendingSwap } from './usePendingSwap.js';
import { getVerboseFilters, getVerboseDisplay } from '../../../lite/verbose.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import {
  buildRenderTheme,
  renderAgentMessage,
  renderShellOutputBlock,
  renderThinkingBlock,
  renderLiveStreamingOutputBar,
  renderMessageToText,
  type RenderContext,
} from '../../../lite/render.js';
import { needsLeadingBlankByRole } from '../../../lite/blank-rules.js';
import { getAgentColor } from '../../../utils/agentColors.js';
import {
  useGlyphs,
  useSpinners,
  useAllowAsciiArt,
} from '../../../hooks/useGlyphs.js';
import { useAnimationPaused } from '../../../contexts/AnimationPausedContext.js';
import chalk from 'chalk';

// Lite pacman frames (Unicode only). ASCII mode falls back to quarterSpinner.
const PACMAN_SPINNER_FRAMES: readonly string[] = [
  'ᗢ',
  'ᗣ',
  'ᗤ',
  'ᗥ',
  'ᗦ',
  'ᗧ',
  'ᗨ',
  'ᗩ',
];
const SPINNER_INTERVAL = 150;

// Sentinel for a running tool's spinner slot. Two US-separator control chars,
// picked so they can't collide with tool args, ANSI escapes, or output. The
// row is built ONCE per (id, content) via renderMessageToText with this
// placeholder, then each tick does a single replaceAll to swap the glyph in —
// so the heavy args/diff/reasoning render isn't redone every 150ms.
const SPINNER_PLACEHOLDER = '\x1F\x1F';

export const LiteLiveRegion: React.FC = () => {
  const isProcessing = useAppStore((s) => s.isProcessing);
  const messages = useAppStore((s) => s.messages);
  const retryStatus = useAppStore((s) => s.retryStatus);
  const thinkingContent = useAppStore((s) => s.thinkingContent);
  // Subscribe to the streamingContent primitive (not derived from `messages`)
  // so per-chunk updates re-render only this component, not every
  // [messages]-dep memo in LiteLayout.
  const streamingContent = useAppStore((s) => s.streamingContent);
  // Per-tool streaming output buffers (store flushes ToolCallUpdate chunks
  // here, deleted on finish). The single binding driving the tool-output preview.
  const liveOutputs = useAppStore((s) => s.liveOutputs);
  const pendingApproval = useAppStore((s) => s.pendingApproval);
  const currentAgent = useAppStore((s) => s.currentAgent);
  // Shell-escape (`!command`): the live region collapses to a single
  // brand-purple `! `-gutter row streaming PTY output. Spinner/thinking/tool
  // paths are all suppressed — showing a "thinking" indicator over interactive
  // bash (mwinit/sudo/brew-OTP) is the hung-looking bug this branch fixes.
  const isShellEscape = useAppStore((s) => s.isShellEscape);
  const pendingSwap = usePendingSwap();
  // Accessibility wiring (1:1 with TUI): glyph/spinner Unicode↔ASCII,
  // allowAsciiArt (pacman→quarterSpinner fallback), animation-paused.
  const glyphs = useGlyphs();
  const spinners = useSpinners();
  const { allowAsciiArt } = useAllowAsciiArt();
  const animationPaused = useAnimationPaused();
  // Pacman (Unicode) / quarterSpinner (ASCII). Memoized so the interval closure
  // reads a stable reference.
  const mainSpinnerFrames = useMemo<readonly string[]>(
    () => (allowAsciiArt ? PACMAN_SPINNER_FRAMES : spinners.quarterSpinner),
    [allowAsciiArt, spinners.quarterSpinner]
  );
  // brailleRotate so the tool row matches Spinner.tsx elsewhere.
  const toolSpinnerFrames = spinners.brailleRotate;
  const { getColor, getUserPromptColor, getUserPromptBgHex } = useTheme();
  // Streaming agent-tag color from the agent's palette slot (matches finalized).
  const agentTagFn = useMemo<(s: string) => string>(() => {
    try {
      const name = currentAgent?.name;
      if (!name) {
        const fn = getColor('brand');
        const probe = fn('');
        if (typeof probe !== 'string') return chalk.hex('#C19AFF');
        return (s: string) => fn(s);
      }
      const colorFn = getAgentColor(name, getColor);
      const probe = colorFn('');
      if (typeof probe !== 'string') return chalk.hex('#C19AFF');
      return (s: string) => colorFn(s);
    } catch {
      return chalk.hex('#C19AFF');
    }
  }, [getColor, currentAgent?.name]);
  // Full RenderTheme so /theme swaps reflow the live preview like scrollback.
  const renderTheme = useMemo(
    () => buildRenderTheme(getColor, getUserPromptColor, getUserPromptBgHex),
    [getColor, getUserPromptColor, getUserPromptBgHex]
  );

  const [frame, setFrame] = useState(0);
  const [elapsed, setElapsed] = useState(0);
  const thinkingStartRef = useRef(Date.now());
  const prevHadContentRef = useRef(false);
  // The elapsed counter and spinner only show in certain branches. These refs
  // (read in the interval callback, updated each render) gate setElapsed /
  // setFrame so we don't re-render every 150ms for a value nothing displays.
  const elapsedVisibleRef = useRef(true);
  const spinnerVisibleRef = useRef(true);
  // Was the prior tick idle (no streaming/tools)? Transitioning INTO idle
  // restarts the elapsed counter so it reads per-round, not whole-turn.
  const prevIdleVisibleRef = useRef(false);

  // Reset thinking timer when a new thinking batch starts
  // (thinkingContent goes from empty to non-empty)
  useEffect(() => {
    const hasContent = !!thinkingContent;
    if (!prevHadContentRef.current && hasContent) {
      thinkingStartRef.current = Date.now();
      setElapsed(0);
    }
    prevHadContentRef.current = hasContent;
  }, [thinkingContent]);

  useEffect(() => {
    if (!isProcessing) {
      setElapsed(0);
      return;
    }
    thinkingStartRef.current = Date.now();
    // Animation-paused: hold the current frame (last-frame freeze) and reset
    // elapsed to avoid a stale timestamp.
    if (animationPaused) {
      setElapsed(0);
      return;
    }
    const t = setInterval(() => {
      // Gate both setState calls on the visibility refs — skip when nothing on
      // screen reads them (streaming-only with no spinner/counter).
      if (spinnerVisibleRef.current) {
        setFrame((f) => f + 1);
      }
      // Don't count elapsed while waiting for approval or a mid-flight /agent
      // swap (the model isn't running, a climbing timer would lie).
      if (!pendingApproval && !pendingSwap && elapsedVisibleRef.current) {
        setElapsed(Date.now() - thinkingStartRef.current);
      }
    }, SPINNER_INTERVAL);
    return () => clearInterval(t);
  }, [isProcessing, pendingApproval, pendingSwap, animationPaused]);

  // Reset the thinking start time the moment the swap clears so the timer
  // counts from when the model *actually* starts working, not from when the
  // queued message was submitted.
  useEffect(() => {
    if (!pendingSwap && isProcessing) {
      thinkingStartRef.current = Date.now();
      setElapsed(0);
    }
  }, [pendingSwap, isProcessing]);

  const liveContent = isProcessing ? streamingContent : '';

  // Memoized so 150ms spinner ticks don't re-wrap the accumulated reasoning.
  const termCols = process.stdout.columns ?? 80;
  const thinkingBlockMemo = useMemo(() => {
    if (!thinkingContent) return '';
    return renderThinkingBlock(thinkingContent, renderTheme, termCols, glyphs);
  }, [thinkingContent, renderTheme, termCols, glyphs]);

  // Live shell-escape output. The store seeds an empty `shellOutput: true`
  // Model row and streams PTY chunks into it; find it from the tail and render
  // the brand-purple `! ` gutter via renderShellOutputBlock (same helper used
  // for committed rows, so the live→static flush is a no-op visual transition).
  // Empty content returns '' so the JSX renders an "executing" placeholder.
  const shellOutputBlockMemo = useMemo(() => {
    if (!isShellEscape) return '';
    let row: MessageType | null = null;
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i]!;
      if (m.role !== MessageRole.Model) continue;
      if (!('shellOutput' in m && m.shellOutput)) continue;
      row = m;
      break;
    }
    if (!row) return '';
    return renderShellOutputBlock(row.content, renderTheme, termCols);
  }, [isShellEscape, messages, renderTheme, termCols]);

  // Memoized streaming markdown render, through the same renderAgentMessage
  // that finalized rows use so the live→static flush is a no-op visual
  // transition. Marked treats unclosed inline emphasis as literal text, so a
  // half-arrived `**bold` doesn't bleed (verified by probe).
  const streamingBlockMemo = useMemo(() => {
    if (!liveContent) return '';
    return renderAgentMessage(
      liveContent,
      currentAgent?.name,
      renderTheme,
      termCols,
      // agentTagFn already resolved above; wrap into the (name) => fn shape
      // (one streaming agent per live region, so name is ignored).
      () => agentTagFn,
      glyphs
    );
  }, [
    liveContent,
    currentAgent?.name,
    renderTheme,
    termCols,
    agentTagFn,
    glyphs,
  ]);

  // Active tool batch — trailing run from the first still-unfinished tool.
  // MUST mirror static-flush.ts/computeActiveToolBatchIds or a finished tool
  // could appear in both static and the live region. Inner subagent tools are
  // skipped (they render in the footer strip, not here).
  const activeTools = useMemo(() => {
    if (!isProcessing) return [];
    const mainAgent = currentAgent?.name;
    const isInner = (m: (typeof messages)[number]) =>
      m.role === MessageRole.ToolUse &&
      !!m.agentName &&
      !!mainAgent &&
      m.agentName !== mainAgent;
    const isHidden = (m: (typeof messages)[number]) => isInner(m);
    // Walk from the end: find the trailing run of tool messages, skipping
    // inner subagent tool calls.
    let runStart = messages.length;
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i]!;
      if (m.role !== MessageRole.ToolUse) break;
      if (isHidden(m)) continue;
      runStart = i;
    }
    // Find the first unfinished tool inside the run. Anything before it
    // is in the done prefix and lives in static, not here.
    let firstUnfinished = -1;
    for (let i = runStart; i < messages.length; i++) {
      const m = messages[i]!;
      if (m.role !== MessageRole.ToolUse) continue;
      if (isHidden(m)) continue;
      if (!m.isFinished) {
        firstUnfinished = i;
        break;
      }
    }
    if (firstUnfinished === -1) return [];
    const run = messages.slice(firstUnfinished).filter((m) => !isHidden(m));
    // Project the fields downstream needs: id (lookup key), name (output
    // filter), isFinished (gates the output bar), msg (canonical row render).
    return run.map((m) => ({
      id: m.id,
      name: (m as any).name as string,
      isFinished: !!(m as any).isFinished,
      msg: m,
    }));
  }, [messages, isProcessing, currentAgent]);

  // Leading blank for the live region? Mirrors the static separator rule so
  // the gap is identical before/after flush (no "chat shifted up a row").
  // The "next" role is the live content about to render (tool batch / model).
  const needsLeadingSeparator = useMemo(() => {
    if (!isProcessing) return false;
    if (activeTools.length === 0 && !liveContent) return false;
    // Find the most recent message that would be visible in static.
    let prev: (typeof messages)[number] | undefined;
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i]!;
      // Skip the currently-streaming model message — that lives in the live
      // region itself and isn't a previous static row.
      if (
        m.role === MessageRole.Model &&
        i === messages.length - 1 &&
        !m.standalone
      )
        continue;
      // Skip tools that are part of the active in-flight batch (they're in
      // the live region, not above it).
      if (m.role === MessageRole.ToolUse && !m.isFinished) continue;
      prev = m;
      break;
    }
    if (!prev) return false;
    // Tools take precedence — when a tool batch is in flight, the live region
    // opens with tool rows even if streaming text also arrived.
    const nextRole: 'tool_use' | 'model' =
      activeTools.length > 0 ? 'tool_use' : 'model';
    return needsLeadingBlankByRole(prev.role, nextRole);
  }, [messages, isProcessing, activeTools, liveContent]);

  // Per-tool live output bars: tail-window accumulated `liveOutputs` lines
  // through the shared bar formatter so the preview matches the eventual static
  // rendering. Hoisted above the early returns (hooks run unconditionally).
  const display = getVerboseDisplay();
  // getVerboseFilters() (not getVerboseConfig().filters) so the cli.json
  // CHAT_TOOLS_FILTERS override reaches the live gate. Key the memo on the
  // joined content so the array identity stays stable across spinner ticks.
  const filtersKey = getVerboseFilters().join(',');
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const filtersOverride = useMemo(() => getVerboseFilters(), [filtersKey]);
  const liveBarsByToolId = useMemo(() => {
    const out = new Map<string, string[]>();
    for (const tool of activeTools) {
      // Skip finished tools — LiteLayout renders them to <Static>; a live bar
      // would duplicate the static one about to land above it.
      if (tool.isFinished) continue;
      const sourceChunks = liveOutputs.get(tool.id);
      if (!sourceChunks || sourceChunks.length === 0) continue;
      // liveOutputs is chunks (string[][]) for O(1) append; flatten at the
      // boundary (the formatter's tail-window cap bounds the work).
      const sourceLines = sourceChunks.flat();
      const bar = renderLiveStreamingOutputBar(tool.name, sourceLines, {
        outputMaxLines: display.outputMaxLines,
        outputMaxChars: display.outputMaxChars,
        termCols,
        filtersOverride,
        glyphs,
      });
      if (bar.length > 0) out.set(tool.id, bar);
    }
    return out;
  }, [
    activeTools,
    liveOutputs,
    display.outputMaxLines,
    display.outputMaxChars,
    termCols,
    filtersOverride,
  ]);

  // Canonical render of each in-flight tool's chat-log row (same formatter as
  // the settled row) so a running shell/fs_write/fs_read shows its full body
  // immediately. runningSpinner: SPINNER_PLACEHOLDER so the heavy work only
  // reruns on real dep shifts; per tick we just replaceAll the placeholder.
  const renderedToolBodies = useMemo(() => {
    const out = new Map<string, string>();
    // Per-stage color resolver (same as LiteLayout's static ctx) — without it
    // the subagent pipeline tree flickered from chalk.blue to palette shades
    // on settle.
    const stageColor = (stageName: string) =>
      getAgentColor(stageName, getColor);
    const ctx: RenderContext = {
      pendingApprovalToolCallId: pendingApproval?.toolCall.toolCallId ?? null,
      termCols,
      theme: renderTheme,
      filtersOverride,
      runningSpinner: SPINNER_PLACEHOLDER,
      glyphs,
      getStageInputColor: stageColor,
      getStageOutputColor: stageColor,
      getAgentTagColor: stageColor,
    };
    for (const tool of activeTools) {
      const text = renderMessageToText(
        tool.msg as MessageType,
        currentAgent?.name,
        ctx
      );
      if (text) out.set(tool.id, text);
    }
    return out;
  }, [
    activeTools,
    pendingApproval,
    termCols,
    renderTheme,
    filtersOverride,
    currentAgent?.name,
    getColor,
  ]);

  if (!isProcessing) return null;

  // Shell-escape branch: collapse to a single `! ` gutter row streaming PTY
  // output. Fires BEFORE spinner/retry/tool/thinking — none apply to a `!`
  // command (isProcessing is true only for input chrome + Ctrl+C). Without
  // this the user sees an infinite "thinking" spinner over interactive bash.
  // Empty buffer → dim "executing..." placeholder. wrap="overflow" lets the
  // terminal soft-wrap (baking \n would mangle programs emitting cursor escapes).
  if (isShellEscape) {
    spinnerVisibleRef.current = false;
    elapsedVisibleRef.current = false;
    if (shellOutputBlockMemo) {
      return <Text wrap="overflow">{shellOutputBlockMemo}</Text>;
    }
    const gutter = (renderTheme.brand ?? chalk.hex('#C19AFF'))('! ');
    return (
      <Text wrap="overflow">
        {gutter}
        {chalk.dim('executing...')}
      </Text>
    );
  }

  // Spinner glyphs from the active frame sets (brand color is mode-invariant).
  const spinner = chalk.hex('#C19AFF')(
    mainSpinnerFrames[frame % mainSpinnerFrames.length]
  );
  const toolSpinner = chalk.hex('#C19AFF')(
    toolSpinnerFrames[frame % toolSpinnerFrames.length]
  );

  if (retryStatus) {
    // Retry banner shows the spinner — keep ticking.
    spinnerVisibleRef.current = true;
    return (
      <Text>
        {spinner} {chalk.yellow(retryStatus.message)}
      </Text>
    );
  }

  // Tool call lines — substitute the active spinner glyph into each pre-baked
  // row. Trivial / finished tools have no placeholder, so replaceAll no-ops.
  const toolLines = activeTools.map((tool) => {
    const body = renderedToolBodies.get(tool.id);
    if (!body) return '';
    return body.includes(SPINNER_PLACEHOLDER)
      ? body.replaceAll(SPINNER_PLACEHOLDER, toolSpinner)
      : body;
  });
  // Mirror visibility into the refs the interval reads. Elapsed shows only in
  // the idle/thinking-only branch; the spinner shows idle OR tools OR thinking
  // (streaming-only has none — skip setFrame so long text streams don't
  // re-render every 150ms).
  const idleVisible = !liveContent && toolLines.length === 0;
  elapsedVisibleRef.current = idleVisible;
  spinnerVisibleRef.current =
    idleVisible || toolLines.length > 0 || !!thinkingContent;
  // Round-boundary reset: re-entering idle restarts the counter (else "thinking
  // 47s" persists whole-turn). setElapsed(0) in render is safe — prevIdle flips
  // true next pass, so it doesn't loop.
  if (idleVisible && !prevIdleVisibleRef.current) {
    thinkingStartRef.current = Date.now();
    if (elapsed !== 0) setElapsed(0);
  }
  prevIdleVisibleRef.current = idleVisible;

  // Thinking state: no content streaming, no tools running
  if (!liveContent && toolLines.length === 0) {
    const secs = Math.floor(elapsed / 1000);
    const timeStr = secs > 0 ? chalk.dim(` ${secs}s`) : '';
    // While an /agent swap is mid-flight, the message the user just submitted
    // is held by the dispatcher until the swap RPC returns — the agent isn't
    // actually thinking yet. Render "queued" instead of "thinking" so the
    // spinner doesn't lie about what's happening; the footer's pending chip
    // already names the target agent.
    if (pendingSwap) {
      return (
        <Text>
          {spinner} {chalk.dim('queued')}
        </Text>
      );
    }
    if (thinkingContent) {
      // Gated by /verbose Thinking content: off → spinner + "thinking" label
      // but no preview body (lean/minimal presets).
      const showThinking = getVerboseDisplay().showThinkingContent;
      if (!showThinking) {
        return (
          <Text>
            {spinner} {chalk.dim.italic('thinking')}
            {timeStr}
          </Text>
        );
      }
      // Same purple-bordered block as scrollback (memoized) so the preview
      // matches finalized output 1:1 and earlier reasoning stays visible.
      const block = thinkingBlockMemo;
      return (
        <Box flexDirection="column">
          <Text>
            {spinner} {chalk.dim.italic('thinking')}
            {timeStr}
          </Text>
          {block && <Text>{block}</Text>}
        </Box>
      );
    }
    // No thinking/streaming/tools — plain "thinking" label.
    return (
      <Text>
        {spinner} {chalk.dim('thinking')}
        {timeStr}
      </Text>
    );
  }

  // Leading blanks are BAKED into the row text, never rendered as a sibling
  // `<Text> </Text>`: twinki trimEnd()s wrapped lines and collapses an empty
  // Text in a flex column to zero rows, so the gap wouldn't appear until the
  // following row commits (this glued the prompt to a pending tool). Inlining
  // the '\n' makes the gap part of the row that needs it.
  //
  // Thinking preview gated by /verbose Thinking content; when on it persists
  // across tools + streaming (accumulating every Thought chunk this round).
  const showThinkingContent = getVerboseDisplay().showThinkingContent;
  const showThinkingPreviewBlock = showThinkingContent && !!thinkingContent;
  const thinkingBlockText = showThinkingPreviewBlock ? thinkingBlockMemo : '';
  const hasThinkingBlock = !!thinkingBlockText;
  // Row order: [leading blank] → thinking block → [blank] → tools → [blank] →
  // streaming. Blanks below the block live inside its own Text (baked '\n') so
  // an empty sibling can't collapse out.
  const thinkingBlockWithBreaks = hasThinkingBlock
    ? (needsLeadingSeparator ? '\n' : '') +
      thinkingBlockText +
      // Trailing blank so the bottom rule doesn't glue to the next row.
      (toolLines.length > 0 || liveContent ? '\n' : '')
    : null;
  // When the thinking block took the leading-separator slot, tools + streaming
  // don't bake their own.
  const firstToolWithBreak =
    needsLeadingSeparator && toolLines.length > 0 && !hasThinkingBlock
      ? '\n' + toolLines[0]!
      : (toolLines[0] ?? null);
  const standaloneStreamingWithBreak =
    needsLeadingSeparator &&
    liveContent &&
    toolLines.length === 0 &&
    !hasThinkingBlock
      ? '\n' + streamingBlockMemo
      : null;

  return (
    <Box flexDirection="column">
      {/* Persistent thinking block above tools + streaming so reasoning stays
          visible while the model speaks/runs tools. */}
      {hasThinkingBlock && <Text>{thinkingBlockWithBreaks}</Text>}
      {/* Active tool calls — each line followed by its output bar (if any),
          concatenated via \n so twinki treats them as one block (a separate
          <Text> sibling would race the tool line on each spinner tick). */}
      {toolLines.map((line, i) => {
        const tool = activeTools[i]!;
        const head =
          i === 0 && firstToolWithBreak != null && !hasThinkingBlock
            ? firstToolWithBreak
            : line;
        const bar = liveBarsByToolId.get(tool.id);
        const body =
          bar && bar.length > 0 ? `${head}\n${bar.join('\n')}` : head;
        // wrap="overflow" — body is pre-formatted with its own structural \n;
        // overflow keeps single-line semantics so a long command copies as one
        // line (default wrap would bake extra \n into it).
        return (
          <Text key={tool.id} wrap="overflow">
            {body}
          </Text>
        );
      })}
      {/* Separator between tools and streaming content */}
      {liveContent && toolLines.length > 0 && <Text> </Text>}
      {/* Streaming content through the same renderAgentMessage pipeline as
          finalized rows (live→static flush is a no-op). Carries the leading
          blank itself when it's the only live row. wrap="overflow" as in
          LiteLayout's Static <Text>. */}
      {liveContent && (
        <Text wrap="overflow">
          {standaloneStreamingWithBreak ?? streamingBlockMemo}
        </Text>
      )}
    </Box>
  );
};

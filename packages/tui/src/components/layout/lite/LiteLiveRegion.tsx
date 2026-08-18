/**
 * LiteLiveRegion: active tools + withheld errors + streaming content + thinking.
 * All running tools show at once; thinking resets per round; streaming content
 * is not height-bounded (terminal scrolls naturally).
 */
import React, { useState, useEffect, useRef, useMemo } from 'react';
import { Box, Text } from '../../../renderer.js';
import {
  useAppStore,
  MessageRole,
  type MessageType,
} from '../../../stores/app-store.js';
import { usePendingSwap } from './usePendingSwap.js';
import {
  computeActiveToolBatchIds,
  selectStaticEligible,
} from './static-flush.js';
import {
  getVerboseFilters,
  getVerboseDisplay,
  isMcpMessage,
} from '../../../lite/verbose.js';
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
import { chalk } from '../../../utils/color.js';

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

export function selectLiteLiveHistory(
  messages: MessageType[],
  isProcessing: boolean,
  mainAgentName?: string | null
): { rows: MessageType[]; lastStaticMessage: MessageType | null } {
  if (!isProcessing) return { rows: [], lastStaticMessage: null };

  const activeToolIds = computeActiveToolBatchIds(messages, mainAgentName);
  const staticMessages = selectStaticEligible(
    messages,
    true,
    activeToolIds,
    mainAgentName
  );
  const staticIds = new Set(staticMessages.map((message) => message.id));
  const rows = messages.filter(
    (message) =>
      (message.role === MessageRole.ToolUse && activeToolIds.has(message.id)) ||
      (message.role === MessageRole.User &&
        message.questionToolCallId !== undefined &&
        !staticIds.has(message.id)) ||
      (message.role === MessageRole.System &&
        message.success === false &&
        !staticIds.has(message.id))
  );

  return {
    rows,
    lastStaticMessage: staticMessages.at(-1) ?? null,
  };
}

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
  // here, deleted on finish).
  const liveOutputs = useAppStore((s) => s.liveOutputs);
  const pendingApproval = useAppStore((s) => s.pendingApproval);
  const pendingQuestion = useAppStore((s) => s.pendingQuestion);
  const currentAgent = useAppStore((s) => s.currentAgent);
  // Shell-escape (`!command`) suppresses spinner/thinking/tool paths — a
  // "thinking" indicator over interactive bash (mwinit/sudo/brew-OTP) is the
  // hung-looking bug this branch fixes.
  const isShellEscape = useAppStore((s) => s.isShellEscape);
  const pendingSwap = usePendingSwap();
  const glyphs = useGlyphs();
  const spinners = useSpinners();
  const { allowAsciiArt } = useAllowAsciiArt();
  const animationPaused = useAnimationPaused();
  // Memoized so the interval closure reads a stable reference.
  const mainSpinnerFrames = useMemo<readonly string[]>(
    () => (allowAsciiArt ? PACMAN_SPINNER_FRAMES : spinners.quarterSpinner),
    [allowAsciiArt, spinners.quarterSpinner]
  );
  // brailleRotate so the tool row matches Spinner.tsx elsewhere.
  const toolSpinnerFrames = spinners.brailleRotate;
  const { getColor, getUserPromptColor, getUserPromptBgHex } = useTheme();
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
  // The elapsed counter and spinner only show in certain branches. These refs
  // (read in the interval callback, updated each render) gate setElapsed /
  // setFrame so we don't re-render every 150ms for a value nothing displays.
  const elapsedVisibleRef = useRef(true);
  const spinnerVisibleRef = useRef(true);
  // Was the prior tick idle (no streaming/tools)? Transitioning INTO idle
  // restarts the elapsed counter so it reads per-round, not whole-turn.
  const prevIdleVisibleRef = useRef(false);

  // Timer resets per ROUND (idle re-entry, below), NOT per thinkingContent
  // toggle — the store churns that empty↔non-empty within one round, so
  // resetting on it restarted the counter mid-round.

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
      if (
        !pendingApproval &&
        !pendingQuestion &&
        !pendingSwap &&
        elapsedVisibleRef.current
      ) {
        setElapsed(Date.now() - thinkingStartRef.current);
      }
    }, SPINNER_INTERVAL);
    return () => clearInterval(t);
  }, [
    isProcessing,
    pendingApproval,
    pendingQuestion,
    pendingSwap,
    animationPaused,
  ]);

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

  const liveHistory = useMemo(
    () =>
      selectLiteLiveHistory(messages, isProcessing, currentAgent?.name ?? null),
    [messages, isProcessing, currentAgent?.name]
  );
  const liveRows = liveHistory.rows;
  const activeTools = useMemo(
    () =>
      liveRows.flatMap((message) =>
        message.role === MessageRole.ToolUse
          ? [
              {
                id: message.id,
                name: message.name,
                isFinished: !!message.isFinished,
                msg: message,
              },
            ]
          : []
      ),
    [liveRows]
  );

  // Leading blank for the live region? Mirrors the static separator rule so
  // the gap is identical before/after flush (no "chat shifted up a row").
  const needsLeadingSeparator = useMemo(() => {
    if (!isProcessing) return false;
    const nextRole =
      liveRows[0]?.role ?? (liveContent ? MessageRole.Model : null);
    if (!nextRole) return false;
    const prev = liveHistory.lastStaticMessage;
    if (!prev) return false;
    return needsLeadingBlankByRole(prev.role, nextRole);
  }, [isProcessing, liveRows, liveContent, liveHistory.lastStaticMessage]);

  // Per-tool live output bars through the shared bar formatter so the preview
  // matches the eventual static rendering. Hooks run unconditionally, so
  // hoisted above the early returns.
  const display = getVerboseDisplay();
  // Key the saved filters by content so the array identity stays stable across
  // spinner ticks.
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
      // Keep immutable chunks intact so the formatter can reuse prior wraps.
      const bar = renderLiveStreamingOutputBar(tool.name, sourceChunks, {
        outputMaxLines: display.outputMaxLines,
        outputMaxChars: display.outputMaxChars,
        termCols,
        filtersOverride,
        glyphs,
        kind: tool.msg.kind,
        origin: tool.msg.origin ?? (isMcpMessage(tool.msg) ? 'mcp' : undefined),
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
      pendingApprovalToolCallId:
        pendingQuestion?.toolCallId ??
        pendingApproval?.toolCall.toolCallId ??
        null,
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
    pendingQuestion,
    termCols,
    renderTheme,
    filtersOverride,
    currentAgent?.name,
    getColor,
  ]);
  const renderedNonToolRows = useMemo(() => {
    const out = new Map<string, string>();
    const ctx: RenderContext = { termCols, theme: renderTheme, glyphs };
    for (const row of liveRows) {
      if (row.role !== MessageRole.ToolUse) {
        out.set(row.id, renderMessageToText(row, currentAgent?.name, ctx));
      }
    }
    return out;
  }, [liveRows, termCols, renderTheme, glyphs, currentAgent?.name]);

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
    spinnerVisibleRef.current = true; // banner shows the spinner — keep ticking
    return (
      <Text>
        {spinner} {chalk.yellow(retryStatus.message)}
      </Text>
    );
  }

  // Substitute the spinner glyph into each pre-baked row (no placeholder on
  // trivial/finished tools, so replaceAll no-ops).
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
  const idleVisible = !liveContent && liveRows.length === 0;
  elapsedVisibleRef.current = idleVisible;
  spinnerVisibleRef.current =
    idleVisible || activeTools.length > 0 || !!thinkingContent;
  // Round-boundary reset: re-entering idle restarts the counter (else "thinking
  // 47s" persists whole-turn). setElapsed(0) in render is safe — prevIdle flips
  // true next pass, so it doesn't loop.
  if (idleVisible && !prevIdleVisibleRef.current) {
    thinkingStartRef.current = Date.now();
    if (elapsed !== 0) setElapsed(0);
  }
  prevIdleVisibleRef.current = idleVisible;

  if (!liveContent && liveRows.length === 0) {
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
      // but no preview body (for example, the lean preset).
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
  // Blanks below the thinking block live inside its own Text so an empty
  // sibling cannot collapse out.
  const thinkingBlockWithBreaks = hasThinkingBlock
    ? (needsLeadingSeparator ? '\n' : '') +
      thinkingBlockText +
      (liveRows.length > 0 || liveContent ? '\n' : '')
    : null;
  const toolIndexById = new Map(
    activeTools.map((tool, index) => [tool.id, index])
  );
  const liveHistoryLines = liveRows.map((row, index) => {
    let body: string;
    if (row.role === MessageRole.ToolUse) {
      const toolIndex = toolIndexById.get(row.id);
      const head = toolIndex === undefined ? '' : toolLines[toolIndex]!;
      const bar = liveBarsByToolId.get(row.id);
      body = bar?.length ? `${head}\n${bar.join('\n')}` : head;
    } else {
      body = renderedNonToolRows.get(row.id) ?? '';
    }
    const needsBreak =
      index === 0
        ? needsLeadingSeparator && !hasThinkingBlock
        : needsLeadingBlankByRole(liveRows[index - 1]!.role, row.role);
    return { id: row.id, text: (needsBreak ? '\n' : '') + body };
  });
  const previousLiveRole = liveRows.at(-1)?.role;
  const streamingNeedsBreak = previousLiveRole
    ? needsLeadingBlankByRole(previousLiveRole, MessageRole.Model)
    : needsLeadingSeparator && !hasThinkingBlock;
  const streamingWithBreak =
    streamingNeedsBreak && liveContent ? '\n' + streamingBlockMemo : null;

  return (
    <Box flexDirection="column">
      {hasThinkingBlock && <Text>{thinkingBlockWithBreaks}</Text>}
      {liveHistoryLines.map((row) => {
        return (
          <Text key={row.id} wrap="overflow">
            {row.text}
          </Text>
        );
      })}
      {/* Streaming content through the same renderAgentMessage pipeline as
          finalized rows (live→static flush is a no-op). Carries the leading
          blank itself when it's the only live row. wrap="overflow" as in
          LiteLayout's Static <Text>. */}
      {liveContent && (
        <Text wrap="overflow">{streamingWithBreak ?? streamingBlockMemo}</Text>
      )}
    </Box>
  );
};

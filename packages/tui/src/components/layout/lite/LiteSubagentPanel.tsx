/**
 * LiteSubagentPanel — inline trace viewer for one running subagent.
 *
 * The lite layout's append-only chat log (rendered into <Static>) is hard
 * read-only — re-rendering it would scramble the user's scrollback. So
 * instead of a full-screen "agent monitor" like classic mode, this panel
 * lives ABOVE the input box (which is allowed to re-render freely) and
 * shows the focused subagent's full trace there.
 *
 * The trace is reconstructed from `sessionConversationsStore` keyed by the
 * subagent's sessionId — same source classic uses for its monitor view.
 * Lines are rendered with the same `renderMessageToText` helper as the
 * main chat log so the subagent's reasoning, tool calls, and outputs
 * format identically. The viewport renders a fixed line count, with one
 * horizontal column of padding inside the box border.
 *
 * Auto-follow: while the caller's `scrollOffset` is "at the bottom" the
 * panel stays pinned to the latest trace lines as new content arrives —
 * mirroring how a tail -f reader feels. Once the user scrolls up, follow
 * disengages and we keep their offset stable; scrolling back to bottom
 * re-arms follow.
 *
 * Pure presentational. All keypress handling lives in LiteLayout (Ctrl+O
 * toggle, ←→ cycle, ↑↓ scroll, Esc close) so it can coordinate with
 * PromptInput.suppressArrows.
 */
import React, { useEffect, useMemo } from 'react';
import { Box, Text } from '../../../renderer.js';
import { useStore } from 'zustand';
import { sessionConversationsStore } from '../../../stores/session-conversations.js';
import { renderMessageToText } from '../../../lite/render.js';
import { MessageRole, type MessageType } from '../../../stores/app-store.js';
import { getAgentColor } from '../../../utils/agentColors.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { needsLeadingBlank } from './static-flush.js';
import chalk from 'chalk';

interface LiteSubagentPanelProps {
  /** sessionId of the focused subagent — drives the conversation lookup. */
  sessionId: string;
  /** Display name shown in the panel header. */
  name: string;
  /** Index of this subagent within the open list (1-based for display). */
  position: number;
  /** Total number of subagents the user can cycle through. */
  total: number;
  /** Visible line count (caller computes from terminal height). */
  visibleLines: number;
  /** How many lines to skip from the top of the trace. */
  scrollOffset: number;
  /**
   * Whether the panel should stay pinned to the bottom as new lines arrive.
   * The caller flips this off the first time the user scrolls up and back on
   * when they scroll back down to the floor.
   */
  followBottom: boolean;
  /**
   * Reports the trace's current total line count to the caller so it can
   * clamp scroll offsets and re-arm followBottom when the user pages back to
   * the floor. Called whenever totalLines changes.
   */
  onLinesChange?: (totalLines: number) => void;
  /** Optional phase label shown after the name (e.g. "running", "complete"). */
  phaseLabel?: string;
  /**
   * When true, the user has pressed Ctrl+X once and is in the 2s window
   * before a second press kills the focused subagent. The header surfaces
   * a yellow `armed: ctrl+x to KILL` chip and the hint row swaps from its
   * normal dim "ctrl+o close" line to a yellow "ctrl+x KILL · esc cancel"
   * call-to-action. Visual is yellow throughout to match the rest of
   * lite's "user attention required" signals (approval prompt's tool
   * name, footer subagent row in 'requesting-permission' phase).
   *
   * Pure presentational — the timer and second-press detection live in
   * LiteLayout so the kill action can coordinate with terminateSession,
   * the in-flight tool-call cleanup in `sessionConversationsStore`, and
   * the pending-approval dismissal. The panel just reflects the state.
   */
  armedToKill?: boolean;
}

const EMPTY: any[] = [];

export const LiteSubagentPanel: React.FC<LiteSubagentPanelProps> = ({
  sessionId,
  name,
  position,
  total,
  visibleLines,
  scrollOffset,
  followBottom,
  onLinesChange,
  phaseLabel,
  armedToKill,
}) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  // Subscribe to just this subagent's conversation. The shape is
  // MessageType[] — model text, tool calls, etc.
  const messages = useStore(
    sessionConversationsStore,
    (s) => s.conversations.get(sessionId) ?? EMPTY
  );

  // Render every message and split into lines. We keep the message-level
  // rendering identical to the main chat log so users get the same visual
  // mental model — the panel is just a scoped viewport into the same
  // formatter. `agentName` filtering doesn't apply here: the entire
  // conversation IS this subagent's trace.
  const allLines = useMemo(() => {
    const out: string[] = [];
    let prev: MessageType | null = null;
    for (const msg of messages) {
      // Skip empty model messages (still streaming, no text yet).
      if (msg.role === MessageRole.Model && !msg.content) continue;
      const text = renderMessageToText(msg, name, {
        // Don't suppress diff for any tool — the subagent panel never
        // hosts an approval prompt, so there's no "above the fold" copy
        // we'd be duplicating.
        pendingApprovalToolCallId: null,
        termCols: process.stdout.columns ?? 80,
      });
      if (!text) continue;
      // Match the chat log's section-spacing rules: blank around user
      // messages, blank between tools and model text, compact between
      // adjacent tool calls. Without this the trace reads as a wall of
      // back-to-back tool lines and the eye loses turn boundaries.
      if (prev && needsLeadingBlank(prev, msg)) out.push('');
      out.push(...text.split('\n'));
      prev = msg;
    }
    return out;
  }, [messages, name]);

  const totalLines = allLines.length;
  // Bubble totalLines up so the parent can clamp scroll offsets and decide
  // when ↓ has reached the floor (and follow can re-engage). Effect runs only
  // when totalLines changes — no per-render pressure on the parent.
  useEffect(() => {
    onLinesChange?.(totalLines);
  }, [totalLines, onLinesChange]);
  const maxOffset = Math.max(0, totalLines - visibleLines);
  // Auto-follow: when the caller says "stay pinned to bottom" we ignore the
  // raw scrollOffset and snap to the last `visibleLines` rows. This keeps the
  // panel feeling like `tail -f` while the subagent emits new tool calls /
  // reasoning. Manual scrolling unsets followBottom in the caller, at which
  // point we honor scrollOffset directly.
  const offset = followBottom
    ? maxOffset
    : Math.min(maxOffset, Math.max(0, scrollOffset));
  const visible = allLines.slice(offset, offset + visibleLines);

  // Pad the viewport so the panel doesn't visually shrink when the trace
  // is shorter than `visibleLines`. Without this the input box would
  // jump up and down as the trace grows. The "(no trace yet)" placeholder
  // counts as one line so we subtract it from the padding budget when
  // there's no trace yet.
  const visibleCount = visible.length === 0 ? 1 : visible.length;
  const padding = Math.max(0, visibleLines - visibleCount);

  // One column of horizontal padding inside the box border so trace text
  // doesn't run flush against the chrome — matches the breathing room the
  // chat log itself has via the leading "> " prefix.
  const INDENT = '  ';

  const agentColor = getAgentColor(name, getColor);
  const header = (() => {
    const tag = agentColor(`[${name}]`);
    const counter = total > 1 ? chalk.dim(` ${position}/${total}`) : '';
    // Match the footer's terminal-phase convention (SubagentFooter.tsx) so
    // the panel and the strip read consistently — red ✗ killed when the
    // user terminated the stage, green ✓ complete when it finished cleanly.
    // Other phases (running, summarizing, requesting-permission) stay as
    // plain dim text since they're already advertised elsewhere.
    const phase =
      phaseLabel === 'killed'
        ? chalk.red(` · ${glyphs.cross} killed`)
        : phaseLabel === 'complete'
          ? chalk.green(` · ${glyphs.checkmark} complete`)
          : phaseLabel
            ? chalk.dim(` · ${phaseLabel}`)
            : '';
    const scroll =
      totalLines > visibleLines
        ? chalk.dim(
            ` · ${offset + 1}-${offset + visible.length}/${totalLines}${
              followBottom ? ' · live' : ''
            }`
          )
        : '';
    // Armed-kill chip — yellow so it stands out against the otherwise-dim
    // header. Sits at the tail so the existing position/phase/scroll bits
    // stay where the eye expects them; the chip is the new thing demanding
    // attention. Dropped entirely when not armed.
    const armed = armedToKill
      ? chalk.yellow(' · armed: press ctrl+x again to KILL')
      : '';
    return `${chalk.dim(`${glyphs.cornerTopLeft}${glyphs.lineHorizontal} `)}${tag}${counter}${phase}${scroll}${armed}`;
  })();

  const hint = (() => {
    // Armed mode owns the hint row entirely — replace the normal scroll /
    // cycle / close hints with a yellow call-to-action so the user has one
    // clear instruction during the 2s window. Esc cancels the arm without
    // closing the panel (the panel-keypress handler in LiteLayout disarms
    // on Esc when something is armed; without something armed, Esc closes
    // the panel like normal).
    if (armedToKill) {
      return chalk.yellow(
        `${glyphs.cornerBottomLeft}${glyphs.lineHorizontal} ctrl+x KILL · esc cancel`
      );
    }
    // Show the full shortcut set from the moment a subagent panel opens —
    // discoverability beats minimalism here. The previous gates hid most
    // shortcuts at first stage spawn (empty trace → no scroll hint, single
    // stage → no cycle hint, ctrl+x never advertised) so the user only saw
    // ctrl+o close. ↑↓ + ctrl+a/z are no-ops when the trace fits the
    // viewport, so unconditionally advertising them costs nothing.
    // shift+←→ stays gated on multi-stage because cycling with one stage
    // is a true no-op. ctrl+x kill is gated on phaseLabel !== 'complete'
    // and !== 'killed' to match the kill handler's bail condition (LiteLayout)
    // — we don't advertise a kill that won't fire on a terminal stage.
    const parts: string[] = [];
    parts.push('↑↓ scroll · ctrl+a/z top/bot');
    if (total > 1) parts.push('shift+←→ cycle');
    if (phaseLabel && phaseLabel !== 'complete' && phaseLabel !== 'killed')
      parts.push('ctrl+x kill');
    parts.push('ctrl+o close');
    return chalk.dim(
      `${glyphs.cornerBottomLeft}${glyphs.lineHorizontal} ${parts.join(' · ')}`
    );
  })();

  // Bake the inner pad rows + bottom padding into the body Text elements as
  // embedded newlines instead of rendering single-space `<Text> </Text>`
  // sibling rows. Each wrapped line is trimEnd()-ed before flush, so a
  // sibling whose content becomes empty collapses to zero rows on a later
  // render — leaving prior-frame glyphs (visible most often when a markdown
  // table was the last body content at kill time) painted on screen and
  // creating a visual "duplicate panel". Embedding the blanks inside the
  // body text element keeps them as part of a non-empty Text whose row
  // count survives every re-render. Same trick LiteLiveRegion uses for its
  // leading separator. Visual height is preserved exactly: 1 (header) + 1
  // (top blank) + visibleLines (trace + bottom padding) + 1 (bottom blank)
  // + 1 (hint) = 4 + visibleLines, same as before.
  const trailingPad = padding + 1;
  return (
    <Box flexDirection="column">
      <Text>{header}</Text>
      {visible.length === 0 ? (
        <Text>
          {'\n' +
            chalk.dim(`${INDENT}(no trace yet)`) +
            '\n'.repeat(trailingPad)}
        </Text>
      ) : (
        visible.map((line, i) => {
          const isFirst = i === 0;
          const isLast = i === visible.length - 1;
          const prefix = isFirst ? '\n' : '';
          const suffix = isLast ? '\n'.repeat(trailingPad) : '';
          return (
            <Text key={`s-${offset + i}`}>
              {prefix}
              {INDENT}
              {line}
              {suffix}
            </Text>
          );
        })
      )}
      <Text>{hint}</Text>
    </Box>
  );
};

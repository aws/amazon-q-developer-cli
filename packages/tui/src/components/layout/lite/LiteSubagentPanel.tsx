/**
 * LiteSubagentPanel — inline trace viewer for one running subagent.
 *
 * The lite chat log (in <Static>) is hard read-only — re-rendering it would
 * scramble scrollback. So instead of a full-screen monitor, this panel lives
 * ABOVE the input box (free to re-render) and shows the focused subagent's
 * trace, reconstructed from `sessionConversationsStore` via the same
 * `renderMessageToText` helper as the main log.
 *
 * Pure presentational. All keypress handling lives in LiteLayout so it can
 * coordinate with PromptInput.suppressArrows.
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
  sessionId: string;
  name: string;
  /** 1-based for display. */
  position: number;
  total: number;
  visibleLines: number;
  /** How many lines to skip from the top of the trace. */
  scrollOffset: number;
  /**
   * Whether the panel should stay pinned to the bottom as new lines arrive.
   * The caller flips this off the first time the user scrolls up and back on
   * when they scroll back down to the floor.
   */
  followBottom: boolean;
  /** Reports total line count to the caller (clamps scroll, re-arms follow). */
  onLinesChange?: (totalLines: number) => void;
  /** Optional phase label shown after the name (e.g. "running", "complete"). */
  phaseLabel?: string;
  /**
   * True during the 2s window after the first Ctrl+X, before a second press
   * kills the subagent. The panel only reflects this; the timer, second-press
   * detection, and kill (terminateSession + cleanup) live in LiteLayout.
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
  const messages = useStore(
    sessionConversationsStore,
    (s) => s.conversations.get(sessionId) ?? EMPTY
  );

  // The entire conversation IS this subagent's trace, so no agentName filter.
  const allLines = useMemo(() => {
    const out: string[] = [];
    let prev: MessageType | null = null;
    for (const msg of messages) {
      if (msg.role === MessageRole.Model && !msg.content) continue;
      const text = renderMessageToText(msg, name, {
        // The panel never hosts an approval prompt, so no diff to suppress.
        pendingApprovalToolCallId: null,
        termCols: process.stdout.columns ?? 80,
      });
      if (!text) continue;
      // Match the chat log's section-spacing so the trace doesn't read as a
      // wall of back-to-back tool lines with no turn boundaries.
      if (prev && needsLeadingBlank(prev, msg)) out.push('');
      out.push(...text.split('\n'));
      prev = msg;
    }
    return out;
  }, [messages, name]);

  const totalLines = allLines.length;
  useEffect(() => {
    onLinesChange?.(totalLines);
  }, [totalLines, onLinesChange]);
  const maxOffset = Math.max(0, totalLines - visibleLines);
  // followBottom (set by the caller) snaps to the last `visibleLines` rows for
  // a tail -f feel; manual scrolling unsets it and we honor scrollOffset.
  const offset = followBottom
    ? maxOffset
    : Math.min(maxOffset, Math.max(0, scrollOffset));
  const visible = allLines.slice(offset, offset + visibleLines);

  // Pad so the panel doesn't shrink (and bounce the input box) when the trace
  // is shorter than visibleLines. The "(no trace yet)" placeholder counts as
  // one line, so subtract it from the budget when there's no trace.
  const visibleCount = visible.length === 0 ? 1 : visible.length;
  const padding = Math.max(0, visibleLines - visibleCount);

  const INDENT = '  ';

  const agentColor = getAgentColor(name, getColor);
  const header = (() => {
    const tag = agentColor(`[${name}]`);
    const counter = total > 1 ? chalk.dim(` ${position}/${total}`) : '';
    // Match the footer's terminal-phase convention (SubagentFooter.tsx) so the
    // panel and strip read consistently (red ✗ killed, green ✓ complete).
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
    // Armed-kill chip at the tail (yellow, attention-demanding) so the
    // existing position/phase/scroll bits stay where the eye expects them.
    const armed = armedToKill
      ? chalk.yellow(' · armed: press ctrl+x again to KILL')
      : '';
    return `${chalk.dim(`${glyphs.cornerTopLeft}${glyphs.lineHorizontal} `)}${tag}${counter}${phase}${scroll}${armed}`;
  })();

  const hint = (() => {
    // Armed mode owns the hint row: one yellow call-to-action during the 2s
    // window (Esc cancels the arm without closing the panel).
    if (armedToKill) {
      return chalk.yellow(
        `${glyphs.cornerBottomLeft}${glyphs.lineHorizontal} ctrl+x KILL · esc cancel`
      );
    }
    // Advertise the full shortcut set unconditionally (discoverability) — the
    // scroll keys are harmless no-ops when the trace fits. shift+←→ stays
    // gated on multi-stage (true no-op otherwise); ctrl+x kill is gated to
    // match the kill handler's bail on terminal stages (LiteLayout).
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

  // WORKAROUND: bake pad rows into the body Text as embedded newlines rather
  // than single-space `<Text> </Text>` siblings. trimEnd() before flush
  // collapses an empty sibling to zero rows on a later render, leaving
  // prior-frame glyphs painted (a visual "duplicate panel" — most visible
  // when a markdown table was the last body at kill time). Embedded blanks
  // stay part of a non-empty Text whose row count survives re-renders. Same
  // trick LiteLiveRegion uses. Visual height stays 4 + visibleLines.
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

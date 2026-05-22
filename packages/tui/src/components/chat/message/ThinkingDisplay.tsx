import React, { useMemo } from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';

/** Number of trailing lines shown in the collapsed view. */
const PREVIEW_LINES = 4;

export interface ThinkingDisplayProps {
  /** Reasoning/thinking text emitted by the agent. */
  text: string;
  /**
   * True when this turn has been flushed to scrollback. The expansion state
   * is then frozen via the hook's snapshot logic, so reopening history
   * shows whatever the user had at the time of flush. The "(ctrl+o to
   * toggle)" suffix is also dropped from the hint in this state, since
   * pressing ctrl+o no longer affects this block.
   */
  isStatic?: boolean;
  /** Optional bar color override (defaults to the active agent's color). */
  barColor?: string;
}

/**
 * Renders the agent's streaming reasoning ("thinking") text.
 *
 * Visual shape (collapsed, after streaming):
 *   ● Thinking
 *     ...+6 lines above (ctrl+o to toggle)
 *     Third paragraph still visible.
 *
 *     Final paragraph.
 *
 * Notes:
 *   - We tail-truncate (show the most-recent lines), so the elision hint
 *     sits *above* the body — that's the direction where the hidden
 *     content actually lives. This is intentionally different from
 *     head-truncating tools like Read/Grep, which place their hint below.
 *   - Paragraph breaks (`\n\n`) are preserved as blank lines within the
 *     body. Leading empty lines from a paragraph-break landing at the slice
 *     boundary are trimmed so the body never opens with a blank row.
 *   - Expansion shares the global `toolOutputsExpanded` flag with tool
 *     outputs (ctrl+o toggles all collapsible content at once). The flag
 *     applies during streaming too — pressing ctrl+o while reasoning is
 *     still arriving expands the block immediately, the same way it does
 *     for streaming tool outputs.
 *   - In scrollback (`isStatic=true`), expansion state is frozen at flush
 *     and the hint drops the "(ctrl+o to toggle)" suffix.
 */
export const ThinkingDisplay = React.memo(function ThinkingDisplay({
  text,
  isStatic = false,
  barColor,
}: ThinkingDisplayProps) {
  const { getColor } = useTheme();
  const dim = getColor('secondary');

  // Trim outer whitespace and split. Keep internal empty strings —
  // they represent paragraph boundaries the model emitted on purpose
  // and we want to render them as blank rows in both views.
  const lines = useMemo(() => {
    const trimmed = text.trim();
    return trimmed === '' ? [] : trimmed.split('\n');
  }, [text]);

  // We use the hook for two things only:
  //   1. `expanded` — subscription to the global toolOutputsExpanded flag
  //   2. side effects: registering hasExpandableToolOutputs (so ctrl+o is
  //      bound app-wide) and requestRemeasure() on expand/collapse.
  // The hook's own `expandHint`/`hiddenCount` ignore our leading-empty
  // trim, so we compute those locally below.
  const { expanded } = useExpandableOutput({
    totalItems: lines.length,
    previewCount: PREVIEW_LINES,
    isStatic,
    unit: 'lines',
  });

  // Streaming no longer forces a collapsed view: ctrl+o needs to take
  // effect immediately on the thinking block, the same way it does on
  // streaming tool outputs (e.g. Shell). If the user expands mid-stream,
  // subsequent flushes simply append more lines below.
  const visibleLines = useMemo(() => {
    if (expanded) return lines;
    let tail = lines.slice(-PREVIEW_LINES);
    while (tail.length > 0 && tail[0] === '') {
      tail = tail.slice(1);
    }
    return tail;
  }, [expanded, lines]);

  // Honest hidden count: total minus what we actually render. If the slice
  // started on paragraph-break empties, those are reflected here as hidden
  // — the user can ctrl+o to see them in context.
  const hiddenAbove = expanded ? 0 : lines.length - visibleLines.length;

  // Hint sits *above* the body since we tail-truncate. "above" makes the
  // direction explicit. In active state, surface the keybinding; in static
  // (scrollback) state, the expansion ref is frozen — pressing ctrl+o
  // won't change this block — so we omit the suffix to avoid suggesting
  // an action that no longer works.
  //
  // The hint shows during streaming too: as the model emits more lines
  // beyond the visible tail, the count ticks up (".+5 above" → "+6 above"
  // → ...) which gives the user a clear signal that earlier reasoning has
  // scrolled out of view, even while the tail is still moving.
  const topHint =
    hiddenAbove > 0
      ? isStatic
        ? `...+${hiddenAbove} lines above`
        : `...+${hiddenAbove} lines above (ctrl+o to toggle)`
      : '';

  if (lines.length === 0) return null;

  // Status `success` -> dot + bar both render via theme's `success` entry,
  // which has a `truecolor` field (no `named` fallback), so dot and bar
  // are guaranteed to render as the same colour regardless of terminal
  // palette. Same green as finished tools — visually consistent with the
  // rest of the chat surface.
  return (
    <StatusBar status="success" barColor={barColor}>
      <Box flexDirection="column">
        <StatusInfo title="Thinking" />
        {topHint !== '' && <Text>{dim(topHint)}</Text>}
        <Text>{dim(visibleLines.join('\n'))}</Text>
      </Box>
    </StatusBar>
  );
});

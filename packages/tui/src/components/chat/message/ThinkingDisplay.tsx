import React, { useMemo } from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useKeybindings } from '../../../hooks/useKeybindings.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import type { ThinkingMode } from '../../../hooks/useGlyphs.js';

export interface ThinkingDisplayProps {
  /** Reasoning/thinking text emitted by the agent. */
  text: string;
  /** Display mode. `off` is handled by the caller (this never renders then). */
  mode?: ThinkingMode;
  /**
   * Duration spent reasoning, in ms. When set, the block is "done": it shows
   * "Thought for Ns" instead of the live "Thinking..." header.
   */
  thinkingMs?: number;
  /**
   * True when this turn has been flushed to scrollback. Expansion state is
   * frozen via the hook's snapshot logic, and the ctrl+o hint is dropped.
   */
  isStatic?: boolean;
  /** Optional bar color override (defaults to the active agent's color). */
  barColor?: string;
}

/**
 * Renders the agent's reasoning ("thinking") as a single collapsible block.
 *
 *   collapsed (default):  ⋮ Thinking... (esc to cancel · ctrl+o to view)
 *   expanded (ctrl+o):    ⋮ Thinking... (esc to cancel · ctrl+o to collapse details)
 *                           <full reasoning stream>
 *   done:                 ● Thought for 3s... (ctrl+o to view)
 *
 * Expansion shares the global `toolOutputsExpanded` flag with tool outputs, so
 * one ctrl+o toggles both. In `expanded` mode the stream is always shown and
 * ctrl+o is a noop on thinking (handled via `forceExpanded`).
 */
export const ThinkingDisplay = React.memo(function ThinkingDisplay({
  text,
  mode = 'collapsed',
  thinkingMs,
  isStatic = false,
  barColor,
}: ThinkingDisplayProps) {
  const { getColor } = useTheme();
  const dim = getColor('secondary');
  const keybindings = useKeybindings();
  const glyphs = useGlyphs();

  const lines = useMemo(() => {
    const trimmed = text.trim();
    return trimmed === '' ? [] : trimmed.split('\n');
  }, [text]);

  // previewCount 0: any line makes the block expandable (registers ctrl+o).
  // forceExpanded in `expanded` mode → always open, never registered (noop).
  const { expanded } = useExpandableOutput({
    totalItems: lines.length,
    previewCount: 0,
    isStatic,
    forceExpanded: mode === 'expanded',
  });

  if (lines.length === 0) return null;

  const done = thinkingMs != null;
  const title = done
    ? `Thought for ${Math.max(1, Math.ceil(thinkingMs / 1000))}s...`
    : 'Thinking...';

  // Hint parts: "esc to cancel" only while actively reasoning; the ctrl+o
  // toggle only when it actually does something (collapsed mode, non-static).
  const hintParts: string[] = [];
  if (!done && !isStatic) {
    hintParts.push(`${keybindings.label('cancelStream')} to cancel`);
  }
  if (mode === 'collapsed' && !isStatic) {
    hintParts.push(expanded ? 'ctrl+o to collapse details' : 'ctrl+o to view');
  }
  const hint = hintParts.length > 0 ? ` (${hintParts.join(' · ')})` : '';

  return (
    <StatusBar status={done ? 'success' : 'thinking'} barColor={barColor}>
      <Box flexDirection="column">
        <Text>
          {dim(title)}
          {dim(hint)}
        </Text>
        {expanded && (
          <Box flexDirection="row" marginLeft={2}>
            <Text>{dim(`${glyphs.cornerBottomLeftRound} `)}</Text>
            <Box flexGrow={1} flexShrink={1}>
              <Text>{dim(lines.join('\n'))}</Text>
            </Box>
          </Box>
        )}
      </Box>
    </StatusBar>
  );
});

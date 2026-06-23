/**
 * Full-height scrollable /verbosity preview, shown in place of the menu's
 * input area when the user presses `p`. Same fixtures as the inline preview
 * but un-clipped, with its own scroll offset and keypress handling.
 */
import React, { useEffect, useMemo, useState } from 'react';
import { Box } from '../../../renderer.js';
import { Text } from '../text/Text.js';
import { Divider } from '../divider/Divider.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { useVerbosityPreviewText } from './VerbosityPreview.js';
import { type VerbosityPreviewKey } from '../../../lite/render.js';
import { type VerboseDisplayConfig } from '../../../lite/verbose.js';
import chalk from 'chalk';

interface VerbosityPreviewPaneProps {
  which: VerbosityPreviewKey;
  displayOverride?: VerboseDisplayConfig;
  filtersOverride?: readonly string[];
  /** `p` collapses the pane back to the inline (mini) preview. */
  onCollapse: () => void;
  /** Ctrl+P closes the preview entirely (back to hidden). */
  onHide: () => void;
}

const PANE_VISIBLE_LINES = 18;
const PAGE_STEP = 10;

export const VerbosityPreviewPane: React.FC<VerbosityPreviewPaneProps> = ({
  which,
  displayOverride,
  filtersOverride,
  onCollapse,
  onHide,
}) => {
  const { text, dim } = useVerbosityPreviewText(
    which,
    displayOverride,
    filtersOverride,
    true
  );
  const lines = useMemo(() => text.split('\n'), [text]);

  const totalLines = lines.length;
  const maxOffset = Math.max(0, totalLines - PANE_VISIBLE_LINES);
  const [offset, setOffset] = useState(0);

  // Re-clamp when the line count shrinks (e.g. a section toggle collapsed).
  useEffect(() => {
    if (offset > maxOffset) setOffset(maxOffset);
  }, [maxOffset, offset]);

  useKeypress((input, key) => {
    const ch = input.toLowerCase();
    if (key.ctrl && ch === 'p') return onHide();
    // Esc is the safe-back action: returns to the menu (mini), NOT to hidden.
    if (ch === 'p' || key.escape) return onCollapse();
    if (key.upArrow) return setOffset((o) => Math.max(0, o - 1));
    if (key.downArrow) return setOffset((o) => Math.min(maxOffset, o + 1));
    // Ctrl+B/F page, Ctrl+A/Z jump to top/bottom (`less` vocab).
    if (key.ctrl && ch === 'b')
      return setOffset((o) => Math.max(0, o - PAGE_STEP));
    if (key.ctrl && ch === 'f')
      return setOffset((o) => Math.min(maxOffset, o + PAGE_STEP));
    if (key.ctrl && ch === 'a') return setOffset(0);
    if (key.ctrl && ch === 'z') return setOffset(maxOffset);
  });

  const visible = lines.slice(offset, offset + PANE_VISIBLE_LINES);
  const padding = Math.max(0, PANE_VISIBLE_LINES - visible.length);

  const scrollable = totalLines > PANE_VISIBLE_LINES;
  const counter = scrollable
    ? chalk.dim(` · ${offset + 1}-${offset + visible.length}/${totalLines}`)
    : '';
  const hint = chalk.dim(
    `${scrollable ? '↑↓ scroll · ctrl+b/f page · ctrl+a/z top/bot · ' : ''}p shrink · ctrl+p hide · esc back`
  );

  return (
    <Box flexDirection="column">
      <Divider />
      <Box paddingX={1} flexDirection="column">
        <Text>
          {dim('Preview')}
          {counter}
        </Text>
        <Box height={1} />
        {visible.map((line, i) => (
          <Text key={`pl-${offset + i}`}>{line || ' '}</Text>
        ))}
        {Array.from({ length: padding }).map((_, i) => (
          <Text key={`pad-${i}`}> </Text>
        ))}
        <Box height={1} />
        <Text>{hint}</Text>
      </Box>
    </Box>
  );
};

/**
 * Full-height scrollable /verbosity preview, shown in place of the menu's
 * input area when the user presses `p`. Same fixtures as the inline preview
 * but un-clipped, with its own scroll offset and keypress handling.
 */
import React, { useEffect, useMemo, useState } from 'react';
import { Box } from '../../../renderer.js';
import { Text } from '../text/Text.js';
import { Divider } from '../divider/Divider.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import {
  renderVerbosityPreview,
  buildRenderTheme,
  type VerbosityPreviewKey,
} from '../../../lite/render.js';
import {
  getVerboseConfig,
  getVerboseDisplay,
  type VerboseDisplayConfig,
} from '../../../lite/verbose.js';
import chalk from 'chalk';

interface VerbosityPreviewPaneProps {
  /** Which fixture set to render. Picked from the active /verbosity submenu. */
  which: VerbosityPreviewKey;
  /** Draft overrides for an in-progress truncation cap / highlighted density
   *  preset; default to the saved config. */
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
  const { getColor, getUserPromptColor, getUserPromptBgHex } = useTheme();
  const dim = useMemo(() => getColor('secondary'), [getColor]);
  const theme = useMemo(
    () => buildRenderTheme(getColor, getUserPromptColor, getUserPromptBgHex),
    [getColor, getUserPromptColor, getUserPromptBgHex]
  );

  const display = displayOverride ?? getVerboseDisplay();
  const filters = useMemo(
    () => filtersOverride ?? getVerboseConfig().filters,
    [filtersOverride]
  );

  // Build full (un-clipped) preview as a line array. Memoized on the
  // (key, display, filters) tuple so toggling preview state mid-edit
  // doesn't recompute on every render.
  const lines = useMemo(() => {
    const text = renderVerbosityPreview(which, display, filters, {
      expanded: true,
      theme,
    });
    return text.split('\n');
  }, [which, display, filters, theme]);

  const totalLines = lines.length;
  const maxOffset = Math.max(0, totalLines - PANE_VISIBLE_LINES);
  const [offset, setOffset] = useState(0);

  // Re-clamp offset when the underlying line count shrinks (e.g. user
  // collapsed a section toggle while in the pane).
  useEffect(() => {
    if (offset > maxOffset) setOffset(maxOffset);
  }, [maxOffset, offset]);

  useKeypress((input, key) => {
    if (key.ctrl && (input === 'p' || input === 'P')) {
      onHide();
      return;
    }
    if (input === 'p' || input === 'P') {
      onCollapse();
      return;
    }
    // Esc is the safe-back action: returns to the menu with the mini preview,
    // NOT the hidden state.
    if (key.escape) {
      onCollapse();
      return;
    }
    if (key.upArrow) {
      setOffset((o) => Math.max(0, o - 1));
      return;
    }
    if (key.downArrow) {
      setOffset((o) => Math.min(maxOffset, o + 1));
      return;
    }
    // Ctrl+B / Ctrl+F: page back / forward (same vocab as `less`). The
    // subagent panel uses ↑↓ + ctrl+a/z for top/bottom; we add page
    // jumps because the verbosity preview can be much longer than a
    // typical subagent trace.
    if (key.ctrl && (input === 'b' || input === 'B')) {
      setOffset((o) => Math.max(0, o - PAGE_STEP));
      return;
    }
    if (key.ctrl && (input === 'f' || input === 'F')) {
      setOffset((o) => Math.min(maxOffset, o + PAGE_STEP));
      return;
    }
    if (key.ctrl && (input === 'a' || input === 'A')) {
      setOffset(0);
      return;
    }
    if (key.ctrl && (input === 'z' || input === 'Z')) {
      setOffset(maxOffset);
      return;
    }
  });

  const visible = lines.slice(offset, offset + PANE_VISIBLE_LINES);
  // Pad to fixed height so the surrounding layout doesn't jump as the
  // user scrolls past short fixtures.
  const padding = Math.max(0, PANE_VISIBLE_LINES - visible.length);

  const counter =
    totalLines > PANE_VISIBLE_LINES
      ? chalk.dim(` · ${offset + 1}-${offset + visible.length}/${totalLines}`)
      : '';

  const hint = (() => {
    const parts: string[] = [];
    if (totalLines > PANE_VISIBLE_LINES) {
      parts.push('↑↓ scroll', 'ctrl+b/f page', 'ctrl+a/z top/bot');
    }
    parts.push('p shrink', 'ctrl+p hide', 'esc back');
    return chalk.dim(parts.join(' · '));
  })();

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

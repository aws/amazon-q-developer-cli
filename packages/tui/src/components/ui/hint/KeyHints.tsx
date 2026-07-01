import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import type { Glyphs } from '../../../utils/glyphs.js';

/**
 * A single key-hint entry: a (pre-resolved) key chord and its description.
 *
 * `keys` should already contain whatever display glyphs the caller wants,
 * e.g. `` `${glyphs.arrowUp}${glyphs.arrowDown}` `` or a literal like `"esc"`.
 * Resolving glyphs at the call site (instead of by name here) keeps this API
 * tiny and lets callers compose chords freely.
 */
export interface KeyHint {
  /** Pre-resolved key chord to display, e.g. "↑↓", "esc", "ctrl+o". */
  keys: string;
  /** Human-readable action, e.g. "to navigate". */
  label: string;
}

export interface KeyHintsProps {
  /** Ordered hints to render, joined by the active small-dot separator. */
  hints: KeyHint[];
  /** Hide entirely when false (mirrors ActionHint). Defaults to true. */
  visible?: boolean;
  /** Footer alignment (mirrors ActionHint). Defaults to 'right'. */
  align?: 'left' | 'right';
}

/**
 * Shared footer key-hint strip, e.g. `↑↓ to navigate · ↵ to select`.
 *
 * Consolidates the hand-rolled `↑↓ · ↵` footers duplicated across Menu,
 * ApprovalRequest, ThemePanel, SettingsPanel, etc. Keys render in the primary
 * color and labels in muted, matching ActionHint's conventions. The separator
 * is sourced from useGlyphs, so it degrades to `.` in ASCII mode.
 */
export const KeyHints: React.FC<KeyHintsProps> = ({
  hints,
  visible = true,
  align = 'right',
}) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();

  if (!visible || hints.length === 0) return null;

  const key = getColor('primary');
  const dim = getColor('muted');
  const separator = dim(` ${glyphs.smallDot} `);
  const content = hints
    .map(({ keys, label }) => `${key(keys)} ${dim(label)}`)
    .join(separator);

  return (
    <Box
      paddingX={1}
      marginBottom={1}
      justifyContent={align === 'left' ? 'flex-start' : 'flex-end'}
    >
      <Text>{content}</Text>
    </Box>
  );
};

/**
 * Pure-string variant of {@link KeyHints} for non-React / chalk surfaces such
 * as the lite renderer. Joins each `"<keys> <label>"` pair with the active
 * small-dot separator (`·` in Unicode mode, `.` in ASCII mode). Callers apply
 * their own coloring (e.g. chalk) around the returned string.
 */
export function formatKeyHints(glyphs: Glyphs, parts: KeyHint[]): string {
  return parts
    .map(({ keys, label }) => `${keys} ${label}`)
    .join(` ${glyphs.smallDot} `);
}

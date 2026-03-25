import React, { useMemo } from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { visibleWidth } from '../../../utils/text-width.js';

/** Margin used by the StatusBar dot + padding before file list content */
const LEFT_MARGIN = 2;
/** Separator between inline items */
const SEP = ', ';

export interface FileListProps {
  /** Items to display (filenames, entry names, etc.) */
  items: string[];
  /** Max items to show when collapsed (0 = show all) */
  previewCount: number;
  /** Whether the list is expanded (ctrl+o) */
  expanded: boolean;
  /** Hint text for expand toggle (active turn only) */
  expandHint?: string;
  /** Number of hidden items (for static "+N more" suffix) */
  hiddenCount?: number;
}

/**
 * Renders a list of file/entry names.
 *
 * Layout strategy:
 * - Fits as many items as possible on each line (inline, separated by two spaces)
 * - Wraps to next line when the next item wouldn't fit
 * - When collapsed, shows up to `previewCount` items + expand hint
 * - When expanded, shows all items
 */
export const FileList = React.memo(function FileList({
  items,
  previewCount,
  expanded,
  expandHint,
  hiddenCount = 0,
}: FileListProps) {
  const { getColor } = useTheme();
  const color = getColor('primary');

  const termWidth = process.stdout.columns || 80;
  const availableWidth = termWidth - LEFT_MARGIN - 4; // some right margin

  const visibleItems = expanded ? items : items.slice(0, previewCount);
  const moreCount = expanded ? 0 : hiddenCount;

  // Group items into lines that fit within terminal width
  const lines = useMemo(() => {
    const result: string[][] = [];
    let currentLine: string[] = [];
    let currentWidth = 0;

    for (const item of visibleItems) {
      const itemWidth = visibleWidth(item);

      if (currentLine.length === 0) {
        // First item always goes on current line
        currentLine.push(item);
        currentWidth = itemWidth;
      } else {
        const widthWithSep = currentWidth + SEP.length + itemWidth;
        if (widthWithSep <= availableWidth) {
          currentLine.push(item);
          currentWidth = widthWithSep;
        } else {
          result.push(currentLine);
          currentLine = [item];
          currentWidth = itemWidth;
        }
      }
    }
    if (currentLine.length > 0) {
      result.push(currentLine);
    }
    return result;
  }, [visibleItems, availableWidth]);

  if (items.length === 0) return null;

  // "+N more" suffix only in static (history) — active turn uses expandHint instead
  const moreSuffix = moreCount > 0 && !expandHint ? ` +${moreCount} more` : '';

  return (
    <>
      {lines.map((lineItems, i) => {
        const isLastLine = i === lines.length - 1;
        const text = lineItems.join(SEP) + (isLastLine ? moreSuffix : '');
        return (
          <Box key={i} marginLeft={LEFT_MARGIN}>
            <Text>{color(text)}</Text>
          </Box>
        );
      })}
      {moreCount > 0 && expandHint && (
        <Box marginLeft={LEFT_MARGIN}>
          <Text>{getColor('secondary')(expandHint)}</Text>
        </Box>
      )}
    </>
  );
});

import React, { useEffect, useMemo, useState } from 'react';
import { Box, useInput } from './../../renderer.js';
import { Text } from './text/Text.js';
import { Panel } from './panel/Panel.js';
import { Divider } from './divider/Divider.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import {
  padToWidth,
  padToWidthRight,
  truncateToWidth,
  visibleWidth,
} from '../../utils/text-width.js';
import { chalk } from '../../utils/color.js';

/**
 * Clamp `lines` to at most `max` rows. When they overflow, keep the head and
 * tail and replace the middle with a "⋯ N more ⋯" marker, so the start and end
 * of a sequence stay visible (e.g. a rewind turn's first tools + final reply).
 * The marker inherits the indent of the first hidden line for visual alignment.
 */
function elideLines(lines: string[], max: number, mid: string): string[] {
  if (max <= 0) return [];
  if (lines.length <= max) return lines;
  if (max === 1) return [`${mid} ${lines.length} more ${mid}`];
  const visible = max - 1; // one row reserved for the marker
  const head = Math.ceil(visible / 2);
  const tail = visible - head;
  const hidden = lines.length - head - tail;
  const firstHidden = lines[head] || '';
  const indent = firstHidden.match(/^(\s*)/)?.[1] || '';
  return [
    ...lines.slice(0, head),
    `${indent}${mid} ${hidden} more ${mid}`,
    ...(tail > 0 ? lines.slice(lines.length - tail) : []),
  ];
}

/**
 * A selectable overlay with a list of rows, optional column headers, and an
 * optional contextual preview pane that updates on highlight.
 *
 * Use for flows like `/rewind`, `/settings > theme`, `/settings > keybindings`:
 * anywhere the user needs to navigate a list and see per-item details before
 * committing a choice.
 *
 * Sits between `Menu` (flat, read-only-after-pick) and `Panel` (display-only):
 * Explorer is a Panel with an interactive list inside.
 */

export interface ExplorerColumn {
  /** Field key on each row to read the cell value from. */
  key: string;
  /** Header label shown at the top of the list. Empty string to hide. */
  label: string;
  /** Alignment within the column. Default 'left'. */
  align?: 'left' | 'right';
}

export interface ExplorerRow {
  /** Stable id (used as React key and passed to onSelect). */
  id: string;
  /** Column key -> cell value. Missing keys render empty. */
  values: Record<string, string>;
  /** Optional right-side tag shown after the last column, e.g. "← compaction". */
  tag?: string;
  /** Optional preview shown in the preview pane when this row is hovered. */
  preview?: {
    heading?: string;
    body: string;
  };
}

export interface ExplorerProps {
  /** Shown in the Panel title bar. */
  title: string;
  /** Description line above the list. Optional. */
  description?: string;
  columns: ExplorerColumn[];
  rows: ExplorerRow[];
  /** Max visible rows at once. Defaults to 10. */
  visibleRows?: number;
  /** Section heading above the preview pane. Ignored if no rows have a preview. */
  previewHeading?: string;
  /** Keybinding hints shown in Panel footer. Default: ↑↓ / Enter / Esc. */
  keyHints?: Array<{ key: string; label: string }>;
  /** When true, shows a search input for type-to-filter. Defaults to true. */
  searchable?: boolean;
  searchPlaceholder?: string;
  /**
   * Verb shown next to the close shortcut in the footer (default: 'to close').
   * Forwarded to {@link Panel}. Use to disambiguate when ESC means "go back"
   * rather than "close" — e.g. multi-step flows like /settings → theme
   * wizard where ESC walks the user back one screen at a time.
   */
  closeHintLabel?: string;
  onSelect: (row: ExplorerRow) => void;
  onClose: () => void;
}

export const Explorer: React.FC<ExplorerProps> = ({
  title,
  description,
  columns,
  rows,
  visibleRows = 10,
  previewHeading,
  keyHints,
  searchable = true,
  searchPlaceholder = 'type to filter',
  closeHintLabel,
  onSelect,
  onClose,
}) => {
  const { getColor, colors } = useTheme();
  const glyphs = useGlyphs();
  const { width: termWidth, height: termHeight } = useTerminalSize();
  const secondaryHex =
    (colors as { secondary?: { truecolor?: string } }).secondary?.truecolor ??
    '#808080';
  const brandHex = useMemo(() => {
    const fromGetColor = (getColor('brand') as { hex?: string })?.hex;
    if (
      fromGetColor &&
      fromGetColor !== '#000000' &&
      fromGetColor !== 'inherit'
    )
      return fromGetColor;
    return (
      (colors as { brand?: { truecolor?: string } }).brand?.truecolor ??
      '#C19AFF'
    );
  }, [colors, getColor]);
  const accentHex =
    (colors as { accent?: { truecolor?: string } }).accent?.truecolor ??
    '#ff00ff';

  const [selectedIndex, setSelectedIndex] = useState(0);
  const [search, setSearch] = useState('');

  // Filter rows by search: simple case-insensitive substring on any cell value.
  const filteredRows = useMemo(() => {
    if (!search) return rows;
    const q = search.toLowerCase();
    return rows.filter((r) =>
      columns.some((c) => (r.values[c.key] ?? '').toLowerCase().includes(q))
    );
  }, [rows, search, columns]);

  // Clamp selection when filter changes.
  useEffect(() => {
    if (selectedIndex >= filteredRows.length) {
      setSelectedIndex(Math.max(0, filteredRows.length - 1));
    }
  }, [filteredRows.length, selectedIndex]);

  const selected = filteredRows[selectedIndex];

  useInput((input, key) => {
    if (key.escape) {
      if (search) {
        setSearch('');
        return;
      }
      onClose();
      return;
    }
    if (key.return) {
      if (selected) onSelect(selected);
      return;
    }
    if (key.upArrow) {
      setSelectedIndex((i) => Math.max(0, i - 1));
      return;
    }
    if (key.downArrow) {
      setSelectedIndex((i) => Math.min(filteredRows.length - 1, i + 1));
      return;
    }
    // Search input
    if (!searchable) return;
    if (key.backspace || key.delete) {
      setSearch((s) => s.slice(0, -1));
      return;
    }
    if (input && input.length === 1 && input >= ' ' && !key.ctrl && !key.meta) {
      setSearch((s) => s + input);
    }
  });

  // Compute column widths from data + header labels.
  const columnWidths = useMemo(() => {
    return columns.map((c) => {
      const headerW = visibleWidth(c.label);
      const dataW = filteredRows.reduce(
        (acc, r) => Math.max(acc, visibleWidth(r.values[c.key] ?? '')),
        0
      );
      return Math.max(headerW, dataW);
    });
  }, [columns, filteredRows]);

  // Responsive: adjust list rows and preview to fit terminal height.
  // All chrome (panel border, title, description, search, header, footer) always renders.
  // Measured from actual JSX output:
  //   panel border+title: 2, blank+description+blank: 3, search+margin: 2,
  //   column header: 1, "(+N more)": 1, footer+border: 2 = 11
  const CHROME_LINES = 11;

  // Reserve a fixed budget for preview so layout doesn't jump as selection changes.
  const hasAnyPreview = filteredRows.some((r) => r.preview);
  const PREVIEW_CHROME = 3; // divider + heading + margin
  const PREVIEW_BODY_MAX = 8;
  const previewBudget = hasAnyPreview ? PREVIEW_BODY_MAX + PREVIEW_CHROME : 0;

  const spaceForContent = termHeight - CHROME_LINES;
  // List gets priority (minimum 3), preview gets the remainder.
  const effectiveVisibleRows = Math.max(
    Math.min(visibleRows, spaceForContent - previewBudget),
    3
  );
  const maxPreviewLines =
    spaceForContent - effectiveVisibleRows > PREVIEW_CHROME + 2
      ? Math.min(
          PREVIEW_BODY_MAX,
          spaceForContent - effectiveVisibleRows - PREVIEW_CHROME
        )
      : 0;

  const previewLines = selected?.preview
    ? elideLines(
        selected.preview.body.split('\n'),
        maxPreviewLines,
        glyphs.midEllipsis
      )
    : [];

  // Responsive width: max width for the first column so rows never wrap.
  const chevronW = 2;
  const gapW = 4;
  const panelPad = 4;
  const secondColW = columnWidths[1] ?? 0;
  const maxFirstColWidth = Math.max(
    termWidth - chevronW - gapW - secondColW - panelPad,
    20
  );

  // Scroll window.
  const startIndex = Math.max(
    0,
    Math.min(
      selectedIndex - Math.floor(effectiveVisibleRows / 2),
      filteredRows.length - effectiveVisibleRows
    )
  );
  const endIndex = Math.min(
    startIndex + effectiveVisibleRows,
    filteredRows.length
  );
  const windowRows = filteredRows.slice(startIndex, endIndex);

  const renderHint = (hint: { key: string; label: string }) => (
    <>
      {hint.key} {chalk.hex(secondaryHex)(hint.label)}
    </>
  );

  const defaultHints: Array<{ key: string; label: string }> = [
    { key: `${glyphs.arrowUp}${glyphs.arrowDown}`, label: 'to navigate' },
    { key: glyphs.enter, label: 'to select' },
  ];
  const effectiveHints = keyHints ?? defaultHints;

  return (
    <Panel
      title={title}
      onClose={onClose}
      closeHintLabel={closeHintLabel}
      footerLeft={
        <Text>
          {effectiveHints.map((h, i) => (
            <React.Fragment key={h.key}>
              {i > 0 && chalk.hex(secondaryHex)(` ${glyphs.smallDot} `)}
              {renderHint(h)}
            </React.Fragment>
          ))}
        </Text>
      }
    >
      <Box flexDirection="column">
        {description && (
          <>
            <Box height={1} />
            <Text>{chalk.hex(secondaryHex)(description)}</Text>
            <Box height={1} />
          </>
        )}

        {searchable && (
          <Box marginBottom={1}>
            <Text>{chalk.hex(secondaryHex)('search: ')}</Text>
            {search ? <Text>{chalk.hex(brandHex)(search)}</Text> : null}
            <Text inverse> </Text>
            {!search && (
              <Text>{chalk.hex(secondaryHex)(' ' + searchPlaceholder)}</Text>
            )}
          </Box>
        )}

        {/* Column headers — flush-left; data rows indent 2 for the chevron slot. */}
        {columns.some((c) => c.label) && (
          <Box flexDirection="row">
            {columns.map((c, i) => {
              // First column absorbs the chevron-slot width so its data rows
              // stay right-aligned with subsequent columns below.
              const w = (columnWidths[i] ?? 0) + (i === 0 ? 2 : 0);
              const label = c.label;
              const padded =
                c.align === 'right'
                  ? padToWidthRight(label, w)
                  : padToWidth(label, w);
              return (
                <React.Fragment key={c.key}>
                  <Text>{padded}</Text>
                  {i < columns.length - 1 && <Box width={4} />}
                </React.Fragment>
              );
            })}
          </Box>
        )}

        {/* Rows */}
        {windowRows.map((row, windowIdx) => {
          const idx = startIndex + windowIdx;
          const isSel = idx === selectedIndex;
          return (
            <Box key={row.id} flexDirection="row">
              <Text>
                {isSel ? chalk.hex(accentHex).bold(`${glyphs.chevron} `) : '  '}
              </Text>
              {columns.map((c, ci) => {
                const w = columnWidths[ci] ?? 0;
                const raw = row.values[c.key] ?? '';
                // Truncate first column to available terminal width to prevent wrapping.
                const cellWidth = ci === 0 ? Math.min(w, maxFirstColWidth) : w;
                const truncated = truncateToWidth(raw, cellWidth);
                const value =
                  c.align === 'right'
                    ? padToWidthRight(truncated, cellWidth)
                    : padToWidth(truncated, cellWidth);
                const styled = isSel
                  ? chalk.hex(accentHex).bold(value)
                  : chalk.hex(secondaryHex)(value);
                return (
                  <React.Fragment key={c.key}>
                    <Text>{styled}</Text>
                    {ci < columns.length - 1 && <Box width={4} />}
                  </React.Fragment>
                );
              })}
              {row.tag && (
                <>
                  <Box width={4} />
                  <Text>{chalk.hex(secondaryHex)(row.tag)}</Text>
                </>
              )}
            </Box>
          );
        })}

        {endIndex < filteredRows.length && (
          <Box>
            <Text>
              {chalk.hex(secondaryHex)(
                `  (+${filteredRows.length - endIndex} more)`
              )}
            </Text>
          </Box>
        )}

        {filteredRows.length === 0 && (
          <Box>
            <Text>{chalk.hex(secondaryHex)('  No matching entries.')}</Text>
          </Box>
        )}

        {/* Preview pane */}
        {selected?.preview && maxPreviewLines > 0 && (
          <>
            <Divider />
            {previewHeading && (
              <Box>
                <Text>{chalk.hex(brandHex)(previewHeading)}</Text>
              </Box>
            )}
            {selected.preview.heading && (
              <Box>
                <Text>{chalk.bold(selected.preview.heading)}</Text>
              </Box>
            )}
            <Box flexDirection="row" marginTop={0}>
              <Box flexDirection="column" width={1} backgroundColor={brandHex}>
                {previewLines.map((_line, i) => (
                  <Text key={i}> </Text>
                ))}
              </Box>
              <Box flexDirection="column" marginLeft={1}>
                {previewLines.map((line, i) => {
                  const truncated =
                    visibleWidth(line) > 160
                      ? truncateToWidth(line, 160, '…')
                      : line;
                  const isTool = line.trimStart().startsWith('↳');
                  return (
                    <Text key={i}>
                      {isTool ? chalk.hex(secondaryHex)(truncated) : truncated}
                    </Text>
                  );
                })}
              </Box>
            </Box>
          </>
        )}
      </Box>
    </Panel>
  );
};

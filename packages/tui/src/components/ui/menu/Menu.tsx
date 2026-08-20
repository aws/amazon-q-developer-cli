import React, {
  useState,
  useEffect,
  useCallback,
  useMemo,
  useRef,
} from 'react';
import { Box, useMouse, CURSOR_MARKER } from './../../../renderer.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTextStyle } from '../../../hooks/useTextStyle.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { Text } from '../text/Text.js';
import { Icon, IconType } from '../icon/Icon.js';
import { Divider } from '../divider/Divider.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { useKeybindings } from '../../../hooks/useKeybindings.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import {
  visibleWidth,
  truncateToWidth,
  padToWidth,
} from '../../../utils/text-width.js';
import { chalk } from '../../../utils/color.js';

export interface MenuItem {
  label: string;
  description: string;
  group?: string;
  /** Cell for the opt-in annotation column; ignored unless the menu sets
   *  `columnHeaders`. Rendered dim, aligned, between label and group. */
  annotation?: string;
}

export interface MenuProps {
  items: MenuItem[];
  prefix?: string; // "/" for slash commands, "@" for mentions, "" for generic
  onSelect: (item: MenuItem) => void;
  onHighlight?: (item: MenuItem) => void;
  onEscape?: () => void;
  onTabComplete?: () => void;
  /** Called when → is pressed on the highlighted item (e.g. drill-in to details). */
  onRightArrow?: (item: MenuItem) => void;
  visibleItems?: number; // defaults to 8
  showSelectedIndicator?: boolean; // show chevron indicator for selected item
  /** When true, renders a search input line above the list for type-to-filter. */
  searchable?: boolean;
  /** Label shown before the search input (e.g. "Select model"). */
  searchLabel?: string;
  /** Placeholder shown when search input is empty. */
  searchPlaceholder?: string;
  /** When true, shows ESC/↑↓ footer hints. Defaults to same as searchable. */
  showFooterHints?: boolean;
  /** When true, selected item uses bold instead of accent color, preserving embedded ANSI colors in labels. */
  preserveLabelColors?: boolean;
  /** Wording after the close-menu key in the footer; the prop owns its leading
   *  separator (`← back` none, `to close` a space). Default `to cancel`. */
  closeMenuActionLabel?: string;
  /** Extra footer segment appended after the standard hints, on the SAME row
   *  with the SAME dim styling (e.g. the /verbosity `ctrl+p to show preview`).
   *  `key` is brand-colored, `label` dim — matching the built-in hints. */
  footerHint?: { key: string; label: string };
  /** Initial cursor row; clamped to range, applied on mount only (re-key to
   *  re-apply). Defaults to 0. */
  initialIndex?: number;
  /** Static title shown above the menu items (rendered regardless of searchable). */
  title?: string;
  /** Lite-only: symmetric arrow shortcuts (right→Enter, left→Esc). */
  liteOnly?: boolean;
  /**
   * Opt-in column mode: renders a dim header row above the items and a dim,
   * aligned annotation cell per item between the label and group columns.
   * `group`/`description` headers render only when provided AND the
   * corresponding column exists (some item has a group / a description).
   * Without this prop, per-item `annotation` values are ignored and the menu
   * renders exactly as before.
   */
  columnHeaders?: {
    label: string;
    annotation: string;
    group?: string;
    description?: string;
  };
}

import { rankMenuItems } from './menu-search.js';

// LINT-DEBT(complexity): pre-existing at gate adoption; Function 'Menu' has a complexity of 38. Maximum allowed is 30.; refactor before extending
// eslint-disable-next-line complexity
export const Menu = React.memo(function Menu({
  items,
  prefix = '',
  onSelect,
  onHighlight,
  onEscape,
  onTabComplete,
  onRightArrow,
  visibleItems = 8,
  showSelectedIndicator = false,
  searchable = false,
  searchLabel = 'search',
  searchPlaceholder = 'type to search',
  showFooterHints,
  preserveLabelColors = false,
  closeMenuActionLabel = 'to cancel',
  initialIndex,
  title,
  liteOnly = false,
  footerHint,
  columnHeaders,
}: MenuProps) {
  const [selectedIndex, setSelectedIndex] = useState(() => {
    if (initialIndex == null) return 0;
    const max = Math.max(0, items.length - 1);
    return Math.min(Math.max(0, initialIndex), max);
  });
  const [searchText, setSearchText] = useState('');
  const { getColor } = useTheme();
  const { width: terminalWidth } = useTerminalSize();
  const keybindings = useKeybindings();
  const glyphs = useGlyphs();

  // Get chalk functions for styling and coloring
  const label = useTextStyle('label');
  const selectedLabel = useTextStyle('selectedLabel');
  const description = getColor('secondary');
  const dimText = getColor('secondary');
  const brandText = getColor('primary');

  // Filter items when searchable: label matches rank above description-only
  // matches, fuzzy score breaks ties within each tier.
  const displayItems = useMemo(() => {
    if (!searchable || !searchText) return items;
    return rankMenuItems(items, searchText);
  }, [items, searchText, searchable]);

  // Reset selection when filter changes. Skip the first run so initialIndex
  // (applied by useState above) survives mount — otherwise the cursor always
  // snaps to row 0 on mount, defeating ESC-back navigation that wants to
  // restore the parent's row.
  const didMountSearchResetRef = useRef(false);
  useEffect(() => {
    if (!didMountSearchResetRef.current) {
      didMountSearchResetRef.current = true;
      return;
    }
    setSelectedIndex(0);
  }, [searchText]);

  // Calculate max group column width (0 if no items have groups). A provided
  // group header participates so it never overflows its column.
  const hasGroups = displayItems.some((item) => item.group);
  const maxGroupLength = hasGroups
    ? Math.max(
        ...displayItems.map((item) => visibleWidth(item.group ?? '')),
        visibleWidth(columnHeaders?.group ?? ''),
        0
      )
    : 0;

  const indicatorWidth = showSelectedIndicator ? 3 : 0; // chevron + 2 spaces
  const spacerWidth = 4; // Box width={4}
  const groupWidth = hasGroups ? maxGroupLength + spacerWidth : 0;

  // Annotation column exists only when headers are provided; the headers
  // participate in the width so a header never overflows its column.
  const hasAnnotations = columnHeaders != null;
  const maxAnnotationLength = hasAnnotations
    ? Math.max(
        ...displayItems.map((item) => visibleWidth(item.annotation ?? '')),
        visibleWidth(columnHeaders.annotation)
      )
    : 0;
  const annotationWidth = hasAnnotations
    ? maxAnnotationLength + spacerWidth
    : 0;

  // Width of the longest label and description, used to size the columns.
  const rawMaxLabelLength = Math.max(
    Math.max(...displayItems.map((item) => visibleWidth(item.label)), 0) +
      visibleWidth(prefix),
    hasAnnotations ? visibleWidth(columnHeaders.label) : 0
  );
  const maxDescLength = Math.max(
    ...displayItems.map((item) => visibleWidth(item.description)),
    0
  );

  // Reserve a small sliver so at least a little of the description (e.g. a
  // relative timestamp like "3 days ago") survives when a long label would
  // otherwise consume the whole row. Kept intentionally minimal so the
  // label gets as much room as possible - descriptions get more than this
  // whenever the label is short, since the cap below only binds for labels
  // too long to fit the row.
  const minDescReserve = 9;
  const descReserve =
    maxDescLength > 0 ? Math.min(maxDescLength, minDescReserve) : 0;

  // Cap the label column to what fits after the indicator, spacer, group
  // and the reserved description width. Labels wider than the cap wrap
  // within their column (rendered in a fixed-width box below) instead of
  // stealing the description's space via flex shrink. Short labels are
  // padded up to the column width; the box width matches the pad target,
  // so the padding never wraps into phantom blank rows on selection.
  const columnMargin = 1;
  const maxLabelColWidth = Math.max(
    1,
    terminalWidth -
      indicatorWidth -
      spacerWidth -
      annotationWidth -
      groupWidth -
      descReserve -
      columnMargin
  );
  const maxLabelLength = Math.min(rawMaxLabelLength, maxLabelColWidth);

  // Calculate available width for description
  const availableDescWidth = Math.max(
    0,
    terminalWidth -
      indicatorWidth -
      maxLabelLength -
      spacerWidth -
      annotationWidth -
      groupWidth -
      columnMargin
  );

  // Call onHighlight when selectedIndex changes
  useEffect(() => {
    const selectedItem = displayItems[selectedIndex];
    if (onHighlight && selectedIndex >= 0 && selectedItem) {
      onHighlight(selectedItem);
    }
  }, [selectedIndex, onHighlight, displayItems]);

  // LINT-DEBT(complexity): pre-existing at gate adoption; Arrow function has a complexity of 40. Maximum allowed is 30.; refactor before extending
  // LINT-DEBT(sonarjs/cognitive-complexity): pre-existing at gate adoption; Refactor this function to reduce its Cognitive Complexity from 34 to the 30 allowed.; refactor before extending
  // eslint-disable-next-line complexity, sonarjs/cognitive-complexity
  useKeypress((input, key) => {
    // ctrl+p = up, ctrl+n = down (standard readline/emacs navigation).
    // Yield Ctrl+P to lite preview controls (CommandMenu claims it); plain ↑
    // still navigates.
    if (key.upArrow || (!liteOnly && key.ctrl && input === 'p')) {
      setSelectedIndex((prev) => Math.max(0, prev - 1));
    } else if (key.downArrow || (key.ctrl && input === 'n')) {
      setSelectedIndex((prev) => Math.min(displayItems.length - 1, prev + 1));
    } else if (key.return && selectedIndex >= 0) {
      const selectedItem = displayItems[selectedIndex];
      if (selectedItem) {
        onSelect(selectedItem);
      }
    } else if (keybindings.matches('closeMenu', input, key) && onEscape) {
      onEscape();
    } else if (key.tab && onTabComplete) {
      onTabComplete();
    } else if (key.rightArrow && onRightArrow) {
      const selectedItem = displayItems[selectedIndex];
      if (selectedItem) onRightArrow(selectedItem);
    } else if (
      liteOnly &&
      key.rightArrow &&
      !onRightArrow &&
      selectedIndex >= 0
    ) {
      // Right=Enter for lite arrow-cluster select. Lite-only: ungating it in
      // TUI would auto-respond to ApprovalRequest dropdowns (no liteOnly).
      const selectedItem = displayItems[selectedIndex];
      if (selectedItem) onSelect(selectedItem);
    } else if (liteOnly && key.leftArrow && onEscape && !searchable) {
      // Left=Esc, symmetric with right. Suppressed in searchable menus where
      // left/right drive the search-input cursor.
      onEscape();
    } else if (!searchable) {
      // Non-searchable: swallow remaining keys.
    } else if (searchable && key.ctrl && input) {
      if (input === 'u') {
        // Ctrl+U - clear line
        setSearchText('');
      } else if (input === 'w') {
        // Ctrl+W - delete word backward
        setSearchText((prev) => prev.replace(/\S+\s*$/, ''));
      }
    } else if (!key.ctrl && !key.meta) {
      // Searchable: capture text input
      if (key.backspace || key.delete) {
        setSearchText((prev) => prev.slice(0, -1));
      } else if (input && input.length === 1 && input >= ' ') {
        setSearchText((prev) => prev + input);
      }
    }
  });

  // Calculate scroll window
  const startIndex = Math.max(
    0,
    Math.min(
      selectedIndex - Math.floor(visibleItems / 2),
      displayItems.length - visibleItems
    )
  );
  const endIndex = Math.min(startIndex + visibleItems, displayItems.length);
  const visibleItemsSlice = displayItems.slice(startIndex, endIndex);

  useMouse(
    useCallback(
      (event: { type: string }) => {
        if (event.type === 'scrollup') {
          setSelectedIndex((prev) => Math.max(0, prev - 1));
        } else if (event.type === 'scrolldown') {
          setSelectedIndex((prev) =>
            Math.min(displayItems.length - 1, prev + 1)
          );
        }
      },
      [displayItems.length]
    )
  );

  return (
    <Box flexDirection="column">
      {title && !searchable && (
        <>
          <Text>{dimText(title)}</Text>
          <Box height={1} />
        </>
      )}
      {searchable && (
        <Box>
          <Text>{dimText(`${searchLabel}: `)}</Text>
          {searchText ? <Text>{brandText(searchText)}</Text> : null}
          <Text inverse> </Text>
          {!searchText && <Text>{dimText(` ${searchPlaceholder}`)}</Text>}
        </Box>
      )}
      {searchable && <Box height={1} />}
      {hasAnnotations && (
        // Mirrors the item-row structure cell for cell (indicator indent,
        // label box, spacers, annotation, group, description) so every
        // header sits exactly over its column.
        <Box flexDirection="row">
          {showSelectedIndicator && (
            <Box flexDirection="row" flexShrink={0}>
              <Text> </Text>
              <Text> </Text>
            </Box>
          )}
          <Box width={maxLabelLength} flexShrink={0}>
            <Text>
              {dimText(padToWidth(columnHeaders.label, maxLabelLength))}
            </Text>
          </Box>
          <Box width={4} />
          <Text>
            {dimText(padToWidth(columnHeaders.annotation, maxAnnotationLength))}
          </Text>
          <Box width={4} />
          {hasGroups && (
            <>
              <Text>
                {dimText(padToWidth(columnHeaders.group ?? '', maxGroupLength))}
              </Text>
              <Box width={4} />
            </>
          )}
          {columnHeaders.description != null && maxDescLength > 0 && (
            // Bounded like item descriptions: on narrow terminals the
            // description column can shrink below the header text.
            <Text>
              {dimText(
                visibleWidth(columnHeaders.description) > availableDescWidth
                  ? truncateToWidth(
                      columnHeaders.description,
                      availableDescWidth,
                      '...'
                    )
                  : columnHeaders.description
              )}
            </Text>
          )}
        </Box>
      )}
      {visibleItemsSlice.map((item, visibleIndex) => {
        const actualIndex = startIndex + visibleIndex;
        const itemText = `${prefix}${item.label}`;
        const paddedItem = padToWidth(itemText, maxLabelLength);
        const isSelected = actualIndex === selectedIndex;

        // Truncate description if too long
        const truncatedDesc =
          visibleWidth(item.description) > availableDescWidth
            ? truncateToWidth(item.description, availableDescWidth, '...')
            : item.description;

        return (
          <Box key={item.label} flexDirection="row">
            {showSelectedIndicator && (
              <Box flexDirection="row" flexShrink={0}>
                {isSelected ? (
                  <Icon
                    type={IconType.CHEVRON_RIGHT}
                    color={preserveLabelColors ? brandText : selectedLabel}
                  />
                ) : (
                  <Text> </Text>
                )}
                <Text> </Text>
              </Box>
            )}
            <Box width={maxLabelLength} flexShrink={0}>
              <Text>
                {isSelected ? CURSOR_MARKER : ''}
                {isSelected
                  ? preserveLabelColors
                    ? chalk.bold(paddedItem)
                    : selectedLabel(paddedItem)
                  : preserveLabelColors
                    ? paddedItem
                    : label(paddedItem)}
              </Text>
            </Box>
            <Box width={4} />
            {hasAnnotations && (
              <>
                <Text>
                  {dimText(
                    padToWidth(item.annotation ?? '', maxAnnotationLength)
                  )}
                </Text>
                <Box width={4} />
              </>
            )}
            {hasGroups && (
              <>
                <Text>
                  {dimText(padToWidth(item.group ?? '', maxGroupLength))}
                </Text>
                <Box width={4} />
              </>
            )}
            <Text>{description(truncatedDesc)}</Text>
          </Box>
        );
      })}
      {endIndex < displayItems.length && (
        <Box>
          <Text>{dimText(`(+${displayItems.length - endIndex} more)`)}</Text>
        </Box>
      )}
      {(showFooterHints ?? searchable) && (
        <>
          <Divider />
          <Box paddingX={1}>
            <Text>
              {brandText(keybindings.label('closeMenu'))}{' '}
              {dimText(closeMenuActionLabel)}
              {displayItems.length > 1 && (
                <>
                  {dimText(` ${glyphs.smallDot} `)}
                  {brandText(`${glyphs.arrowUp}${glyphs.arrowDown}`)}{' '}
                  {dimText('to navigate')}
                </>
              )}
              {onRightArrow ? (
                <>
                  {dimText(` ${glyphs.smallDot} `)}
                  {brandText(glyphs.arrow)} {dimText('to view details')}
                  {dimText(` ${glyphs.smallDot} `)}
                  {brandText(glyphs.enter)} {dimText('to run')}
                </>
              ) : (
                <>
                  {dimText(` ${glyphs.smallDot} `)}
                  {brandText(glyphs.enter)} {dimText('to select')}
                </>
              )}
              {footerHint && (
                <>
                  {dimText(' · ')}
                  {brandText(footerHint.key)} {dimText(footerHint.label)}
                </>
              )}
            </Text>
          </Box>
        </>
      )}
    </Box>
  );
});

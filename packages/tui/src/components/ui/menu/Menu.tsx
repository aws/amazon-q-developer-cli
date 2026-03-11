import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { Box, useMouse } from './../../../renderer.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTextStyle } from '../../../hooks/useTextStyle.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { Text } from '../text/Text.js';
import { Icon, IconType } from '../icon/Icon.js';
import { useKeypress } from '../../../hooks/useKeypress.js';

export interface MenuItem {
  label: string;
  description: string;
  group?: string;
}

export interface MenuProps {
  items: MenuItem[];
  prefix?: string; // "/" for slash commands, "@" for mentions, "" for generic
  onSelect: (item: MenuItem) => void;
  onHighlight?: (item: MenuItem) => void;
  onEscape?: () => void;
  onTabComplete?: () => void;
  visibleItems?: number; // defaults to 8
  showSelectedIndicator?: boolean; // show chevron indicator for selected item
  /** When true, renders a search input line above the list for type-to-filter. */
  searchable?: boolean;
  /** Label shown before the search input (e.g. "Select model"). */
  searchLabel?: string;
  /** Placeholder shown when search input is empty. */
  searchPlaceholder?: string;
}

import { fuzzyScore } from '../../../utils/fuzzyScore.js';

export const Menu = React.memo(function Menu({
  items,
  prefix = '',
  onSelect,
  onHighlight,
  onEscape,
  onTabComplete,
  visibleItems = 8,
  showSelectedIndicator = false,
  searchable = false,
  searchLabel = 'search',
  searchPlaceholder = 'type to search',
}: MenuProps) {
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [searchText, setSearchText] = useState('');
  const { getColor } = useTheme();
  const { width: terminalWidth } = useTerminalSize();

  // Get chalk functions for styling and coloring
  const label = useTextStyle('label');
  const selectedLabel = useTextStyle('selectedLabel');
  const description = getColor('secondary');
  const dimText = getColor('secondary');
  const brandText = getColor('primary');

  // Filter items when searchable using fuzzy subsequence matching + scoring
  const displayItems = useMemo(() => {
    if (!searchable || !searchText) return items;
    const query = searchText.toLowerCase();
    const scored: { item: MenuItem; score: number }[] = [];
    for (const item of items) {
      const labelScore = fuzzyScore(query, item.label.toLowerCase());
      const descScore = fuzzyScore(query, item.description.toLowerCase());
      const best = Math.max(labelScore, descScore);
      if (best > 0) scored.push({ item, score: best });
    }
    scored.sort((a, b) => b.score - a.score);
    return scored.map((s) => s.item);
  }, [items, searchText, searchable]);

  // Reset selection when filter changes
  useEffect(() => {
    setSelectedIndex(0);
  }, [searchText]);

  // Calculate the maximum item label length for consistent column alignment
  const maxLabelLength =
    Math.max(...displayItems.map((item) => item.label.length), 0) +
    prefix.length;

  // Calculate max group column width (0 if no items have groups)
  const hasGroups = displayItems.some((item) => item.group);
  const maxGroupLength = hasGroups
    ? Math.max(...displayItems.map((item) => item.group?.length ?? 0), 0)
    : 0;

  // Calculate available width for description
  const indicatorWidth = showSelectedIndicator ? 3 : 0; // chevron + 2 spaces
  const spacerWidth = 4; // Box width={4}
  const groupWidth = hasGroups ? maxGroupLength + spacerWidth : 0;
  const availableDescWidth =
    terminalWidth -
    indicatorWidth -
    maxLabelLength -
    spacerWidth -
    groupWidth -
    5; // -5 for margin

  // Call onHighlight when selectedIndex changes
  useEffect(() => {
    const selectedItem = displayItems[selectedIndex];
    if (onHighlight && selectedIndex >= 0 && selectedItem) {
      onHighlight(selectedItem);
    }
  }, [selectedIndex, onHighlight, displayItems]);

  useKeypress((input, key) => {
    if (key.upArrow) {
      setSelectedIndex((prev) => Math.max(0, prev - 1));
    } else if (key.downArrow) {
      setSelectedIndex((prev) => Math.min(displayItems.length - 1, prev + 1));
    } else if (key.return && selectedIndex >= 0) {
      const selectedItem = displayItems[selectedIndex];
      if (selectedItem) {
        onSelect(selectedItem);
      }
    } else if (key.escape && onEscape) {
      onEscape();
    } else if (key.tab && onTabComplete) {
      onTabComplete();
    } else if (!searchable) {
      // Non-searchable: rightArrow selects
      if (key.rightArrow && selectedIndex >= 0) {
        const selectedItem = displayItems[selectedIndex];
        if (selectedItem) {
          onSelect(selectedItem);
        }
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

  useMouse({
    onScrollUp: useCallback(() => {
      setSelectedIndex((prev) => Math.max(0, prev - 1));
    }, []),
    onScrollDown: useCallback(() => {
      setSelectedIndex((prev) => Math.min(displayItems.length - 1, prev + 1));
    }, [displayItems.length]),
  });

  return (
    <Box flexDirection="column">
      {searchable && (
        <Box>
          <Text>{dimText(`${searchLabel}: `)}</Text>
          {searchText ? <Text>{brandText(searchText)}</Text> : null}
          <Text inverse> </Text>
          {!searchText && <Text>{dimText(` ${searchPlaceholder}`)}</Text>}
        </Box>
      )}
      {visibleItemsSlice.map((item, visibleIndex) => {
        const actualIndex = startIndex + visibleIndex;
        const itemText = `${prefix}${item.label}`;
        const paddedItem = itemText.padEnd(maxLabelLength);
        const isSelected = actualIndex === selectedIndex;

        // Truncate description if too long
        const truncatedDesc =
          item.description.length > availableDescWidth
            ? item.description.slice(0, availableDescWidth - 3) + '...'
            : item.description;

        return (
          <Box key={item.label} flexDirection="row">
            {showSelectedIndicator && (
              <>
                {isSelected ? (
                  <Icon type={IconType.CHEVRON_RIGHT} color={selectedLabel} />
                ) : (
                  <Text> </Text>
                )}
                <Text> </Text>
              </>
            )}
            <Text>
              {isSelected ? selectedLabel(paddedItem) : label(paddedItem)}
            </Text>
            <Box width={4} />
            {hasGroups && (
              <>
                <Text>
                  {dimText((item.group ?? '').padEnd(maxGroupLength))}
                </Text>
                <Box width={4} />
              </>
            )}
            <Text>{description(truncatedDesc)}</Text>
          </Box>
        );
      })}
    </Box>
  );
});

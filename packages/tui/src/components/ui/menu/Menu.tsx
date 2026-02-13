import React, { useState } from 'react';
import { Box } from 'ink';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTextStyle } from '../../../hooks/useTextStyle.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { Text } from '../text/Text.js';
import { Icon, IconType } from '../icon/Icon.js';
import { useKeypress } from '../../../hooks/useKeypress.js';

export interface MenuItem {
  label: string;
  description: string;
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
}

export const Menu = React.memo(function Menu({
  items,
  prefix = '',
  onSelect,
  onHighlight,
  onEscape,
  onTabComplete,
  visibleItems = 8,
  showSelectedIndicator = false,
}: MenuProps) {
  const [selectedIndex, setSelectedIndex] = useState(0);
  const { getColor } = useTheme();
  const { width: terminalWidth } = useTerminalSize();

  // Get chalk functions for styling and coloring
  const label = useTextStyle('label');
  const selectedLabel = useTextStyle('selectedLabel');
  const description = getColor('secondary');

  // Calculate the maximum item label length for consistent column alignment
  const maxLabelLength =
    Math.max(...items.map((item) => item.label.length)) + prefix.length;

  // Calculate available width for description
  const indicatorWidth = showSelectedIndicator ? 3 : 0; // chevron + 2 spaces
  const spacerWidth = 4; // Box width={4}
  const availableDescWidth =
    terminalWidth - indicatorWidth - maxLabelLength - spacerWidth - 5; // -5 for margin

  // Call onHighlight when selectedIndex changes
  React.useEffect(() => {
    const selectedItem = items[selectedIndex];
    if (onHighlight && selectedIndex >= 0 && selectedItem) {
      onHighlight(selectedItem);
    }
  }, [selectedIndex, onHighlight, items]);

  useKeypress((_input, key) => {
    if (key.upArrow) {
      setSelectedIndex((prev) => Math.max(0, prev - 1));
    } else if (key.downArrow) {
      setSelectedIndex((prev) => Math.min(items.length - 1, prev + 1));
    } else if ((key.return || key.rightArrow) && selectedIndex >= 0) {
      const selectedItem = items[selectedIndex];
      if (selectedItem) {
        onSelect(selectedItem);
      }
    } else if (key.escape && onEscape) {
      onEscape();
    } else if (key.tab && onTabComplete) {
      onTabComplete();
    }
  });

  // Calculate scroll window
  const startIndex = Math.max(
    0,
    Math.min(
      selectedIndex - Math.floor(visibleItems / 2),
      items.length - visibleItems
    )
  );
  const endIndex = Math.min(startIndex + visibleItems, items.length);
  const visibleItemsSlice = items.slice(startIndex, endIndex);

  return (
    <Box flexDirection="column">
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
            <Text>{description(truncatedDesc)}</Text>
          </Box>
        );
      })}
    </Box>
  );
});

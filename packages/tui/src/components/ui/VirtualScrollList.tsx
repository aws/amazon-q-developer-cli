import React from 'react';
import { Box, Text } from 'ink';

interface VirtualScrollListProps {
  items: any[];
  renderItem: (item: any, index: number) => React.ReactNode;
  height?: number;
}

// Placeholder implementation - future enhancement for large conversations
export const VirtualScrollList: React.FC<VirtualScrollListProps> = ({
  items,
  renderItem,
  height = 20,
}) => {
  // For now, render all items (no virtualization)
  // Future implementation would only render visible items for performance

  return (
    <Box flexDirection="column" height={height}>
      {items.length === 0 ? (
        <Text dimColor>No items to display</Text>
      ) : (
        items.map((item, index) => (
          <Box key={index}>{renderItem(item, index)}</Box>
        ))
      )}
    </Box>
  );
};

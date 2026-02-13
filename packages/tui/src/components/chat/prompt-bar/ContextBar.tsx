import React from 'react';
import { Box } from 'ink';
import { Icon, IconType } from '../../ui/icon/Icon.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

interface ContextBarProps {
  children: React.ReactNode;
}

export function ContextBar({ children }: ContextBarProps) {
  const { getColor } = useTheme();

  // Convert children to array and filter out falsy values
  const items = React.Children.toArray(children).filter(Boolean);
  const [firstItem, ...restItems] = items;

  return (
    <Box flexDirection="row" gap={0} width="100%" flexWrap="wrap">
      {/* First item on the left - grows to push others right, never wraps */}
      <Box flexGrow={1} flexShrink={0}>
        {firstItem}
      </Box>

      {/* Remaining items with separators - each chip+separator wraps as a unit */}
      {restItems.map((item, index) => (
        <Box key={index + 1} flexDirection="row" gap={1} flexShrink={0}>
          <Icon type={IconType.SMALL_DOT} color={getColor('secondary')} />
          {item}
        </Box>
      ))}
    </Box>
  );
}

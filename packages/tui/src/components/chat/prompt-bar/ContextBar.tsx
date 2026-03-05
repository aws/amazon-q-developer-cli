import React from 'react';
import { Box } from 'ink';
import { Icon, IconType } from '../../ui/icon/Icon.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

interface ContextBarProps {
  primaryItems?: React.ReactNode[];
  secondaryItems?: React.ReactNode[];
}

export function ContextBar({
  primaryItems = [],
  secondaryItems = [],
}: ContextBarProps) {
  const { getColor } = useTheme();

  // Filter out falsy items
  const filteredPrimary = primaryItems.filter(Boolean);
  const filteredSecondary = secondaryItems.filter(Boolean);

  return (
    <Box flexDirection="row" gap={0} width="100%" flexWrap="wrap">
      {/* Primary items - left aligned, grows to push secondary right */}
      <Box flexDirection="row" gap={0} flexGrow={1}>
        {filteredPrimary.map((item, index) => (
          <Box key={index} flexDirection="row" flexShrink={0}>
            {item}
            {/* Show dot after item only if there's another primary item */}
            {index < filteredPrimary.length - 1 && (
              <Box paddingX={1}>
                <Icon type={IconType.SMALL_DOT} color={getColor('secondary')} />
              </Box>
            )}
          </Box>
        ))}
      </Box>

      {/* Secondary items - right aligned */}
      {filteredSecondary.map((item, index) => (
        <Box
          key={filteredPrimary.length + index}
          flexDirection="row"
          flexShrink={0}
        >
          {item}
          {/* Show dot after item if there's a next item in secondary */}
          {index < filteredSecondary.length - 1 && (
            <Box paddingX={1}>
              <Icon type={IconType.SMALL_DOT} color={getColor('secondary')} />
            </Box>
          )}
        </Box>
      ))}
    </Box>
  );
}

import React from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { Divider } from './divider/Divider.js';
import { MarkdownRenderer } from './MarkdownRenderer.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useAppStore } from '../../stores/app-store.js';

export interface WelcomeMessageBarProps {
  /** When true, show all lines regardless of store expanded state (for Static rendering) */
  forceExpanded?: boolean;
}

export const WelcomeMessageBar = React.memo(function WelcomeMessageBar({
  forceExpanded = false,
}: WelcomeMessageBarProps) {
  const announcement = useAppStore((s) => s.announcement);
  const expanded = useAppStore((s) => s.announcementExpanded);
  const { getColor, getUserResponseColor } = useTheme();

  if (!announcement) return null;

  const lines = announcement.content.split('\n');
  const showAll = forceExpanded || expanded;
  const isTruncated = lines.length > announcement.maxLines;
  const visibleContent = showAll
    ? announcement.content
    : lines.slice(0, announcement.maxLines).join('\n');

  return (
    <Box flexDirection="column" marginTop={1}>
      <Divider />
      <Box paddingX={1} flexDirection="column" marginTop={1}>
        <MarkdownRenderer
          content={visibleContent}
          color={getUserResponseColor()}
        />
        {isTruncated && !showAll && (
          <Text>{getColor('muted')('ctrl+o to expand')}</Text>
        )}
      </Box>
    </Box>
  );
});

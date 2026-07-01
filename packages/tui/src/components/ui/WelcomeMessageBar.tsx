import React from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { Divider } from './divider/Divider.js';
import { MarkdownRenderer } from './MarkdownRenderer.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useAppStore } from '../../stores/app-store.js';
import { useAllowAsciiArt, useGlyphs } from '../../hooks/useGlyphs.js';
import {
  getAnnouncementContent,
  UNICODE_ICONS,
  ASCII_ICONS,
} from '../../constants/feed.js';

/** Extract the header + only the "Added" section from grouped markdown content. */
function extractAddedSection(content: string): string {
  const lines = content.split('\n');
  const result: string[] = [];
  let inAdded = false;

  for (const line of lines) {
    if (line.startsWith('**✨')) {
      result.push(line);
      continue;
    }
    if (/^\*\*\w+\*\*$/.test(line.trim())) {
      inAdded = line.trim() === '**Added**';
      if (inAdded) result.push(line);
      continue;
    }
    if (inAdded) result.push(line);
  }

  return result.join('\n');
}

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
  const { allowAsciiArt } = useAllowAsciiArt();
  const glyphs = useGlyphs();

  if (!announcement) return null;

  const icons = allowAsciiArt ? UNICODE_ICONS : ASCII_ICONS;
  const content = getAnnouncementContent({ icons });
  if (!content) return null;

  const showAll = forceExpanded || expanded;
  const addedOnly = extractAddedSection(content);
  const hasMore = addedOnly.length < content.length;
  const visibleContent = (showAll ? content : addedOnly).replace(
    /✨/g,
    glyphs.sparkle
  );

  return (
    <Box flexDirection="column" marginTop={1}>
      <Divider />
      <Box paddingX={1} flexDirection="column" marginTop={1}>
        <MarkdownRenderer
          content={visibleContent}
          color={getUserResponseColor()}
        />
        {hasMore && !showAll && (
          <Text>{getColor('muted')('ctrl+o to expand')}</Text>
        )}
      </Box>
    </Box>
  );
});

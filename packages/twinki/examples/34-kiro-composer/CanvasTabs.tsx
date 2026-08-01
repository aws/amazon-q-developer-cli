import React from 'react';
import { Box, Text, type ComponentMouseEvent } from 'twinki';
import type { ShowcaseTheme } from '../32-acp-showcase/themes.js';

export interface CanvasTabItem {
  id: string;
  title: string;
  kind: 'session' | 'file' | 'diff';
  statusColor?: string;
}

export function CanvasTabs({
  tabs,
  activeId,
  width,
  theme,
  onActivate,
  onClose,
  onContext,
}: {
  tabs: CanvasTabItem[];
  activeId: string;
  width: number;
  theme: ShowcaseTheme;
  onActivate: (id: string) => void;
  onClose: (id: string) => void;
  onContext: (tab: CanvasTabItem, event: ComponentMouseEvent) => void;
}): React.ReactElement {
  const minimumTabWidth = 10;
  const capacity = Math.max(1, Math.floor(width / minimumTabWidth));
  const activeIndex = Math.max(
    0,
    tabs.findIndex((tab) => tab.id === activeId)
  );
  const start = Math.max(0, Math.min(activeIndex - Math.floor(capacity / 2), tabs.length - capacity));
  const visibleTabs = tabs.slice(start, start + capacity);
  const hiddenBefore = start;
  const hiddenAfter = Math.max(0, tabs.length - start - visibleTabs.length);
  const markerWidth =
    (hiddenBefore ? String(hiddenBefore).length + 2 : 0) + (hiddenAfter ? String(hiddenAfter).length + 2 : 0);
  const separatorWidth = Math.max(0, visibleTabs.length - 1);
  const tabWidth =
    visibleTabs.length > 0
      ? Math.max(6, Math.floor((width - markerWidth - separatorWidth) / visibleTabs.length))
      : width;

  return (
    <Box width={width} height={1} backgroundColor={theme.raised}>
      {hiddenBefore > 0 && (
        <Text color={theme.muted} backgroundColor={theme.raised}>
          {`‹${hiddenBefore} `}
        </Text>
      )}
      {visibleTabs.map((tab, index) => {
        const active = tab.id === activeId;
        const background = active ? theme.panel : theme.raised;
        const icon = tab.kind === 'session' ? 'C' : tab.kind === 'diff' ? 'D' : 'F';
        return (
          <React.Fragment key={tab.id}>
            {index > 0 && (
              <Text color={theme.border} backgroundColor={theme.raised}>
                |
              </Text>
            )}
            <Box
              width={tabWidth}
              backgroundColor={background}
              onClick={() => onActivate(tab.id)}
              onMouseDown={(event) => {
                if (event.button === 'right') onContext(tab, event);
              }}
            >
              <Text color={tab.statusColor ?? (active ? theme.accent : theme.muted)} backgroundColor={background} bold>
                {` ${icon}`}
              </Text>
              <Box width={Math.max(1, tabWidth - 4)} backgroundColor={background}>
                <Text
                  color={active ? theme.fg : theme.muted}
                  backgroundColor={background}
                  bold={active}
                  wrap="truncate"
                >
                  {` ${tab.title}`}
                </Text>
              </Box>
              <Text
                color={active ? theme.muted : theme.border}
                backgroundColor={background}
                onClick={() => onClose(tab.id)}
              >
                {' x'}
              </Text>
            </Box>
          </React.Fragment>
        );
      })}
      {hiddenAfter > 0 && (
        <Text color={theme.muted} backgroundColor={theme.raised}>
          {` ${hiddenAfter}›`}
        </Text>
      )}
      {tabs.length === 0 && (
        <Text color={theme.muted} backgroundColor={theme.raised}>
          {' Empty canvas '}
        </Text>
      )}
    </Box>
  );
}

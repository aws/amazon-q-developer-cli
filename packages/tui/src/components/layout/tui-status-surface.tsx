import React from 'react';
import { useRenderMetrics, isDevMode } from '../../hooks/useRenderMetrics.js';
import {
  useAllowIcons,
  useGlyphs,
  useSpinners,
} from '../../hooks/useGlyphs.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { ContextBar } from '../chat/prompt-bar/ContextBar.js';
import { Chip, ChipColor } from '../ui/chip/index.js';
import type { StatusSurfaceProps } from './status-surface.js';
import {
  STATUS_SEGMENTS,
  statusSegmentIdsFor,
} from './status-line/registry.js';
import { Region } from '../../renderer.js';
import { useStatusSegments } from './status-line/useStatusSegments.js';
import { useStatusClock } from './status-line/useStatusClock.js';
import { statusSegmentsNeedClock } from './status-line/config.js';
import type { SegmentRenderContext } from './status-line/segments.js';

const RenderMetricsChip: React.FC<{
  color?: ChipColor | ((text: string) => string);
}> = ({ color }) => {
  const metrics = useRenderMetrics();
  const glyphs = useGlyphs();
  if (!metrics) return null;
  return (
    <Chip
      value={`${metrics.lastRenderMs.toFixed(1)}ms ${glyphs.smallDot} ${metrics.yogaNodeCount}n ${glyphs.smallDot} ${metrics.heapUsedMB}MB ${glyphs.smallDot} #${metrics.renderCount} ${glyphs.smallDot} r${metrics.fullRedrawCount}`}
      color={color ?? ChipColor.PRIMARY}
    />
  );
};

export const TuiStatusSurface: React.FC<StatusSurfaceProps> = (props) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const spinners = useSpinners();
  const { allowIcons } = useAllowIcons();
  const visible = useStatusSegments('tui');
  const clock = useStatusClock(statusSegmentsNeedClock(visible));

  const ctx: SegmentRenderContext = {
    props,
    getColor,
    glyphs,
    allowIcons,
    spinners,
    muted: props.dimmed ? getColor('muted') : null,
    now: props.now ?? clock,
  };

  // Partitioning the one fixed order by side is what keeps location and branch
  // right-aligned regardless of where they sit in the id list.
  const nodesOn = (side: 'left' | 'right') =>
    statusSegmentIdsFor('tui')
      .filter((id) => visible[id] && STATUS_SEGMENTS[id].side === side)
      .map((id) => STATUS_SEGMENTS[id].tui(ctx));

  // The metrics chip leads the right group, which is where it sat before the bar
  // became configurable.
  const secondaryItems = [
    isDevMode() ? (
      <Region id="metrics">
        <RenderMetricsChip {...(ctx.muted ? { color: ctx.muted } : {})} />
      </Region>
    ) : null,
    ...nodesOn('right'),
  ];

  return (
    <ContextBar
      primaryItems={nodesOn('left')}
      secondaryItems={secondaryItems}
    />
  );
};

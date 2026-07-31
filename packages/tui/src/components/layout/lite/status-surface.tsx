import React from 'react';
import { chalk } from '../../../utils/color.js';
import { Box, Text } from '../../../renderer.js';
import {
  useAllowIcons,
  useGlyphs,
  useSpinners,
} from '../../../hooks/useGlyphs.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { packStatusSegments } from './status-segments.js';
import type { StatusSurfaceProps } from '../status-surface.js';
import {
  STATUS_SEGMENTS,
  statusSegmentIdsFor,
} from '../status-line/registry.js';
import { useStatusSegments } from '../status-line/useStatusSegments.js';
import { useStatusClock } from '../status-line/useStatusClock.js';
import { statusSegmentsNeedClock } from '../status-line/config.js';
import type { SegmentRenderContext } from '../status-line/segments.js';

export const LiteStatusSurface: React.FC<StatusSurfaceProps> = (props) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const spinners = useSpinners();
  const { allowIcons } = useAllowIcons();
  const visible = useStatusSegments('lite');
  const clock = useStatusClock(statusSegmentsNeedClock(visible));

  const ctx: SegmentRenderContext = {
    props,
    getColor,
    glyphs,
    allowIcons,
    spinners,
    // Lite has no dimmed variant; the modern bar owns that state.
    muted: null,
    now: props.now ?? clock,
  };

  // Lite paints one flat run, so a segment's `side` is not consulted: the fixed
  // id order is the paint order and wrapping is handled by width.
  const segments = statusSegmentIdsFor('lite')
    .filter((id) => visible[id])
    .map((id) => STATUS_SEGMENTS[id].lite!(ctx));
  const lines = packStatusSegments(
    segments,
    Math.max(20, process.stdout.columns ?? 80),
    chalk.dim(` ${glyphs.smallDot} `)
  );

  return (
    <Box flexDirection="column">
      {lines.map((line, index) => (
        <Text key={index}>{line}</Text>
      ))}
    </Box>
  );
};

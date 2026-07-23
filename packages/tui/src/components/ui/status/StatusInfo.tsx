import React from 'react';
import { Box } from '../../../renderer.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { Text } from '../text/Text.js';
import { getStatusColor } from '../../../utils/colorUtils.js';
import { useStatusBar } from '../../chat/status-bar/StatusBar.js';
import type { StatusType } from '../../../types/componentTypes.js';
import { ShimmerText } from '../shimmer/ShimmerText.js';
import {
  useToolArgsExpanded,
  useVerbosityToolContext,
} from '../VerbosityToolContext.js';
import { clipChars, formatElapsed } from '../../../lite/render.js';
import { normalizeLineEndings } from '../../../utils/string.js';

export interface StatusInfoProps {
  /** The main heading/identifier (e.g., tool name, alert message) */
  title: string;

  /** Optional target/context shown in parentheses (e.g., file path, environment) */
  target?: string;

  /** Optional status type for color theming - if provided, overrides StatusBar context */
  status?: StatusType;

  /** Whether to color the title based on status. Defaults to false. */
  useStatusColor?: boolean;

  /** Whether to apply shimmer effect to title. Defaults to false. */
  shimmer?: boolean;

  /** Whether to bold the title. Defaults to false. */
  bold?: boolean;

  /** Whether to underline the title. Defaults to false. */
  underline?: boolean;
}

export const StatusInfo = React.memo(function StatusInfo({
  title,
  target,
  status: statusProp,
  useStatusColor = false,
  shimmer = false,
  bold = false,
  underline = false,
}: StatusInfoProps) {
  const { getColor } = useTheme();

  // Try to get status from StatusBar context
  let contextStatus: StatusType | undefined;
  try {
    const statusBar = useStatusBar();
    contextStatus = statusBar.status;
  } catch {
    // Not inside a StatusBar, that's okay
  }

  // Use prop if provided, otherwise use context, otherwise undefined
  const status = statusProp ?? contextStatus;

  const titleColor =
    useStatusColor && status
      ? getStatusColor(status, getColor)
      : getColor('primary');
  const styledTitle =
    bold && underline
      ? titleColor.bold.underline
      : bold
        ? titleColor.bold
        : underline
          ? titleColor.underline
          : titleColor;
  const targetColor = getColor('highlight');

  const {
    reasoning,
    elapsedMs,
    argsMode,
    argsMaxLines,
    argsMaxChars,
    isStatic,
  } = useVerbosityToolContext();
  const fullTarget = target == null ? undefined : normalizeLineEndings(target);
  const fullTargetLines = fullTarget?.split('\n') ?? [];
  const targetLines = fullTargetLines.map((line) =>
    clipChars(line, argsMaxChars ?? null)
  );
  const visibleTargetLines =
    argsMaxLines != null && argsMaxLines > 0
      ? targetLines.slice(0, argsMaxLines)
      : targetLines;
  const hiddenTargetLines = Math.max(
    0,
    targetLines.length - visibleTargetLines.length
  );
  const charsClipped = targetLines.some(
    (line, index) => line !== fullTargetLines[index]
  );
  const argsExpanded = useToolArgsExpanded(
    hiddenTargetLines > 0 || charsClipped
  );
  const truncationMarker =
    hiddenTargetLines > 0
      ? `... (+${hiddenTargetLines} more lines)${isStatic ? '' : ' (ctrl+o to toggle)'}`
      : charsClipped && !isStatic
        ? '... (ctrl+o to toggle)'
        : undefined;
  const displayTarget = argsExpanded
    ? fullTarget
    : [
        ...visibleTargetLines,
        ...(truncationMarker ? [truncationMarker] : []),
      ].join('\n');

  return (
    <>
      <Text>
        {shimmer ? (
          <ShimmerText text={title} color={titleColor.hex} />
        ) : (
          styledTitle(title)
        )}
        {displayTarget &&
          argsMode !== 'off' &&
          targetColor(` ${displayTarget}`)}
        {elapsedMs != null && getColor('muted')(` ${formatElapsed(elapsedMs)}`)}
      </Text>
      {reasoning && (
        <Box marginLeft={2}>
          <Text wrap="wrap">{getColor('brand')(reasoning)}</Text>
        </Box>
      )}
    </>
  );
});

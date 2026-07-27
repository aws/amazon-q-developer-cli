import React from 'react';
import { chalk } from '../../../utils/color.js';
import { Box, Text } from '../../../renderer.js';
import {
  useAllowIcons,
  useGlyphs,
  useSpinners,
} from '../../../hooks/useGlyphs.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import {
  getAgentColor,
  getAgentDisplayName,
} from '../../../utils/agentColors.js';
import { formatCloudFooter } from '../../../utils/cloud-status.js';
import { formatEffort, shortenPath } from '../../../utils/string.js';
import type { Glyphs } from '../../../utils/glyphs.js';
import type { AppState } from '../../../stores/app-store.js';
import { renderPendingAgent } from './ConnectingPanel.js';
import { packStatusSegments } from './status-segments.js';
import { goalElapsed, type StatusSurfaceProps } from '../status-surface.js';

export const LiteStatusSurface: React.FC<StatusSurfaceProps> = ({
  agentName,
  autonomousModeActive = false,
  modelName,
  effort,
  contextUsagePercent,
  workspacePath,
  gitBranch,
  goalStatus,
  cloudSessionActive = false,
  cloudRepo = null,
  cloudBranch = null,
  cloudExtraRepos = 0,
  pendingAgentName = null,
  animationFrame = 0,
}) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const spinners = useSpinners();
  const { allowIcons } = useAllowIcons();
  const secondary = getColor('secondary');
  const ctxPct = contextUsagePercent ?? 0;
  const agentSegment = pendingAgentName
    ? renderPendingAgent(
        pendingAgentName,
        animationFrame,
        getColor,
        spinners.brailleRotate
      )
    : colorAgentName(agentName, getColor);
  const segments = [
    agentSegment,
    autonomousModeActive ? getColor('warning')('Autonomous') : '',
    modelName ? getColor('primary')(modelName) : '',
    effort ? secondary(formatEffort(effort)) : '',
    `${gradientCtxColor(ctxPct)(`${ctxPct}%`)} ${chalk.dim('ctx')}`,
    getColor('brand')(
      cloudSessionActive
        ? formatCloudFooter(
            cloudRepo,
            cloudBranch,
            allowIcons ? glyphs.cloud : undefined,
            cloudExtraRepos,
            glyphs
          )
        : shortenPath(workspacePath)
    ),
    !cloudSessionActive && gitBranch
      ? `${secondary('(')}${getColor('primary')(gitBranch)}${secondary(')')}`
      : '',
    formatGoal(goalStatus, glyphs, allowIcons),
  ];
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

function formatGoal(
  goalStatus: AppState['goalStatus'],
  glyphs: Glyphs,
  allowIcons: boolean
): string {
  if (!goalStatus) return '';
  const iter = `[${goalStatus.iteration + 1}/${goalStatus.maxIterations}]`;
  const elapsed = goalElapsed(goalStatus);
  const withElapsed = (label: string) =>
    elapsed ? `${label} ${glyphs.smallDot} ${elapsed}` : label;
  const withIcon = (icon: string, label: string) =>
    `${allowIcons ? `${icon} ` : ''}${label}`;
  switch (goalStatus.state) {
    case 'completed':
      return chalk.green(withElapsed(withIcon(glyphs.checkmark, 'goal done')));
    case 'exhausted':
      return chalk.red(withElapsed(withIcon(glyphs.cross, 'goal exhausted')));
    case 'paused':
      return chalk.yellow(
        withElapsed(withIcon(glyphs.pause, `goal paused ${iter}`))
      );
    default:
      return chalk.dim(withElapsed(withIcon(glyphs.refresh, `goal ${iter}`)));
  }
}

function colorAgentName(
  agentName: string | null,
  getColor: (path: string) => any
): string {
  const raw = agentName || 'kiro';
  return getAgentColor(raw, getColor)(getAgentDisplayName(raw));
}

function gradientCtxColor(pct: number): (text: string) => string {
  const p = Math.max(0, Math.min(100, pct));
  const lerp = (a: number, b: number, t: number) => Math.round(a + (b - a) * t);
  if (p <= 20) return chalk.rgb(80, 200, 80);
  if (p <= 30) {
    const t = (p - 20) / 10;
    return chalk.rgb(lerp(80, 220, t), lerp(200, 220, t), lerp(80, 0, t));
  }
  const t = (p - 30) / 70;
  return chalk.rgb(220, lerp(220, 60, t), lerp(0, 60, t));
}

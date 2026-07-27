import React from 'react';
import { Region, Text } from '../../renderer.js';
import { useRenderMetrics, isDevMode } from '../../hooks/useRenderMetrics.js';
import { useAllowIcons, useGlyphs } from '../../hooks/useGlyphs.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { formatCloudFooter } from '../../utils/cloud-status.js';
import { getAgentColor, getAgentDisplayName } from '../../utils/agentColors.js';
import { formatEffort, shortenPath } from '../../utils/string.js';
import type { Glyphs } from '../../utils/glyphs.js';
import type { AppState } from '../../stores/app-store.js';
import { ContextBar } from '../chat/prompt-bar/ContextBar.js';
import { Chip, ChipColor, ProgressChip } from '../ui/chip/index.js';
import { goalElapsed, type StatusSurfaceProps } from './status-surface.js';

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

// Region confines the dev-only metrics tick to this status-bar subtree.
export const TuiStatusSurface: React.FC<StatusSurfaceProps> = ({
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
  codeIntelligenceActive = false,
  dimmed = false,
}) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();

  if (dimmed) {
    const mutedColor = getColor('muted');
    const primaryItems = [
      agentName !== null && (
        <Chip value={getAgentDisplayName(agentName)} color={mutedColor} />
      ),
      autonomousModeActive && <Chip value="Autonomous" color={mutedColor} />,
      modelName !== null && <Chip value={modelName} color={mutedColor} />,
      effort && <Chip value={formatEffort(effort)} color={mutedColor} />,
      contextUsagePercent != null && (
        <ProgressChip
          value={contextUsagePercent}
          warningThreshold={60}
          colorOverride={mutedColor}
        />
      ),
      codeIntelligenceActive && allowIcons && (
        <Text>{mutedColor(glyphs.codeIntelligence)}</Text>
      ),
    ];
    const secondaryItems = [
      isDevMode() ? (
        <Region id="metrics">
          <RenderMetricsChip color={mutedColor} />
        </Region>
      ) : null,
      cloudSessionActive ? (
        <Chip
          value={formatCloudFooter(
            cloudRepo,
            cloudBranch,
            allowIcons ? glyphs.cloud : undefined,
            cloudExtraRepos,
            glyphs
          )}
          color={mutedColor}
        />
      ) : (
        <Chip value={shortenPath(workspacePath)} color={mutedColor} />
      ),
      !cloudSessionActive && gitBranch && (
        <Chip value={gitBranch} color={mutedColor} wrap={true} />
      ),
    ];
    return (
      <ContextBar primaryItems={primaryItems} secondaryItems={secondaryItems} />
    );
  }

  const primaryItems = [
    agentName !== null && (
      <Chip
        value={getAgentDisplayName(agentName)}
        color={getAgentColor(agentName, getColor)}
      />
    ),
    autonomousModeActive && (
      <Chip value="Autonomous" color={ChipColor.WARNING} />
    ),
    modelName !== null && <Chip value={modelName} color={ChipColor.PRIMARY} />,
    effort && <Chip value={formatEffort(effort)} color={ChipColor.SECONDARY} />,
    contextUsagePercent != null && (
      <ProgressChip value={contextUsagePercent} warningThreshold={60} />
    ),
    codeIntelligenceActive && allowIcons && (
      <Text>{getColor('primary')(glyphs.codeIntelligence)}</Text>
    ),
    goalStatus && (
      <Chip
        value={formatGoal(goalStatus, glyphs, allowIcons)}
        color={
          goalStatus.state === 'completed'
            ? ChipColor.SUCCESS
            : goalStatus.state === 'exhausted'
              ? ChipColor.ERROR
              : ChipColor.SECONDARY
        }
      />
    ),
  ];
  const secondaryItems = [
    isDevMode() ? (
      <Region id="metrics">
        <RenderMetricsChip />
      </Region>
    ) : null,
    cloudSessionActive ? (
      <Chip
        value={formatCloudFooter(
          cloudRepo,
          cloudBranch,
          allowIcons ? glyphs.cloud : undefined,
          cloudExtraRepos,
          glyphs
        )}
        color={ChipColor.BRAND}
      />
    ) : (
      <Chip value={shortenPath(workspacePath)} color={ChipColor.BRAND} />
    ),
    !cloudSessionActive && gitBranch && (
      <Chip value={gitBranch} color={ChipColor.PRIMARY} wrap={true} />
    ),
  ];
  return (
    <ContextBar primaryItems={primaryItems} secondaryItems={secondaryItems} />
  );
};

function formatGoal(
  goalStatus: NonNullable<AppState['goalStatus']>,
  glyphs: Glyphs,
  allowIcons: boolean
): string {
  const icon = allowIcons
    ? goalStatus.state === 'paused'
      ? glyphs.pause
      : goalStatus.state === 'completed'
        ? glyphs.checkmark
        : goalStatus.state === 'exhausted'
          ? glyphs.cross
          : glyphs.refresh
    : '';
  const label =
    goalStatus.state === 'paused'
      ? 'Paused'
      : goalStatus.state === 'completed'
        ? 'Done'
        : goalStatus.state === 'exhausted'
          ? 'Exhausted'
          : `Active [${goalStatus.iteration + 1}/${goalStatus.maxIterations}]`;
  const elapsed = goalElapsed(goalStatus);
  return `${icon ? `${icon} ` : ''}Goal ${label}${elapsed ? ` ${glyphs.smallDot} ${elapsed}` : ''}`;
}

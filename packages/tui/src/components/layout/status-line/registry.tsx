/**
 * The one place a status-line segment is defined for both surfaces.
 *
 * `Record<StatusSegmentId, StatusSegmentDef>` is total, so adding an id fails to
 * compile until both renderers exist. That is the point: the two surfaces are
 * separate implementations, and a segment reaching only one of them is the bug
 * this shape rules out.
 *
 * The renderers stay separate because the surfaces differ in how they colour and
 * pack their output, not because the data differs.
 */
import React from 'react';
import { Text } from '../../../renderer.js';
import type { UiMode } from '../../../types/ui-mode.js';
import { Chip, ChipColor, ProgressChip } from '../../ui/chip/index.js';
import { chalk } from '../../../utils/color.js';
import {
  getAgentColor,
  getAgentDisplayName,
  renderPendingAgent,
} from '../../../utils/agentColors.js';
import { formatCloudFooter } from '../../../utils/cloud-status.js';
import { formatEffort, shortenPath } from '../../../utils/string.js';
import type { Glyphs } from '../../../utils/glyphs.js';
import type { AppState } from '../../../stores/app-store.js';
import { goalElapsed } from '../status-surface.js';
import { STATUS_SEGMENT_IDS } from './segments.js';
import type {
  SegmentRenderContext,
  StatusSegmentDef,
  StatusSegmentId,
} from './segments.js';

/** Zero-padded local wall clock, e.g. `22:36`. */
function formatClockTime(now: Date): string {
  const hh = String(now.getHours()).padStart(2, '0');
  const mm = String(now.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

/** ISO calendar date, e.g. `2026-07-27`. */
function formatClockDate(now: Date): string {
  const y = now.getFullYear();
  const m = String(now.getMonth() + 1).padStart(2, '0');
  const d = String(now.getDate()).padStart(2, '0');
  return `${y}-${m}-${d}`;
}

/** Where the location segment resolves to: a cloud repo footer, or the cwd. */
function locationLabel(ctx: SegmentRenderContext): string {
  const { props, glyphs, allowIcons } = ctx;
  return props.cloudSessionActive
    ? formatCloudFooter(
        props.cloudRepo ?? null,
        props.cloudBranch ?? null,
        allowIcons ? glyphs.cloud : undefined,
        props.cloudExtraRepos ?? 0,
        glyphs
      )
    : shortenPath(props.workspacePath);
}

function goalLabel(
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
  const suffix = elapsed ? ` ${glyphs.smallDot} ${elapsed}` : '';
  return `${icon ? `${icon} ` : ''}Goal ${label}${suffix}`;
}

/** Lite's goal wording and colouring, which differ from modern's on purpose. */
function liteGoalLabel(
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

/** Green below 20%, ramping through amber to red as the window fills. */
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

export const STATUS_SEGMENTS: Record<StatusSegmentId, StatusSegmentDef> = {
  agent: {
    side: 'left',
    tui: ({ props, getColor, muted }) =>
      props.agentName === null ? null : (
        <Chip
          value={getAgentDisplayName(props.agentName)}
          color={muted ?? getAgentColor(props.agentName, getColor)}
        />
      ),
    lite: ({ props, getColor, spinners }) => {
      if (props.pendingAgentName) {
        return renderPendingAgent(
          props.pendingAgentName,
          props.animationFrame ?? 0,
          getColor,
          spinners.brailleRotate
        );
      }
      const raw = props.agentName || 'kiro';
      return getAgentColor(raw, getColor)(getAgentDisplayName(raw));
    },
  },

  autonomous: {
    side: 'left',
    tui: ({ props, muted }) =>
      props.autonomousModeActive ? (
        <Chip value="Autonomous" color={muted ?? ChipColor.WARNING} />
      ) : null,
    lite: ({ props, getColor }) =>
      props.autonomousModeActive ? getColor('warning')('Autonomous') : '',
  },

  model: {
    side: 'left',
    tui: ({ props, muted }) =>
      props.modelName === null ? null : (
        <Chip value={props.modelName} color={muted ?? ChipColor.PRIMARY} />
      ),
    lite: ({ props, getColor }) =>
      props.modelName ? getColor('primary')(props.modelName) : '',
  },

  effort: {
    side: 'left',
    tui: ({ props, muted }) =>
      props.effort ? (
        <Chip
          value={formatEffort(props.effort)}
          color={muted ?? ChipColor.SECONDARY}
        />
      ) : null,
    lite: ({ props, getColor }) =>
      props.effort ? getColor('secondary')(formatEffort(props.effort)) : '',
  },

  context: {
    side: 'left',
    tui: ({ props, muted }) =>
      props.contextUsagePercent == null ? null : (
        <ProgressChip
          value={props.contextUsagePercent}
          warningThreshold={60}
          {...(muted ? { colorOverride: muted } : {})}
        />
      ),
    // Lite has always shown a bar even with no reading yet, treating absent as 0.
    lite: ({ props }) => {
      const pct = props.contextUsagePercent ?? 0;
      return `${gradientCtxColor(pct)(`${pct}%`)} ${chalk.dim('ctx')}`;
    },
  },

  tangent: {
    side: 'left',
    tui: ({ props, muted }) =>
      props.tangentName ? (
        <Text>{(muted ?? chalk.yellow)(`↯ ${props.tangentName}`)}</Text>
      ) : null,
    lite: ({ props }) =>
      props.tangentName ? chalk.yellow(`↯ ${props.tangentName}`) : '',
  },

  codeIntel: {
    side: 'left',
    tui: ({ props, getColor, glyphs, allowIcons, muted }) =>
      props.codeIntelligenceActive && allowIcons ? (
        <Text>{(muted ?? getColor('primary'))(glyphs.codeIntelligence)}</Text>
      ) : null,
    // Lite has never painted the indicator, so it is not offered there.
    lite: null,
  },

  goal: {
    side: 'left',
    tui: ({ props, glyphs, allowIcons, muted }) => {
      // The dimmed bar has always dropped goal rather than greying it.
      if (!props.goalStatus || muted) return null;
      return (
        <Chip
          value={goalLabel(props.goalStatus, glyphs, allowIcons)}
          color={
            props.goalStatus.state === 'completed'
              ? ChipColor.SUCCESS
              : props.goalStatus.state === 'exhausted'
                ? ChipColor.ERROR
                : ChipColor.SECONDARY
          }
        />
      );
    },
    lite: ({ props, glyphs, allowIcons }) =>
      liteGoalLabel(props.goalStatus, glyphs, allowIcons),
  },

  location: {
    side: 'right',
    tui: (ctx) => (
      <Chip value={locationLabel(ctx)} color={ctx.muted ?? ChipColor.BRAND} />
    ),
    lite: (ctx) => ctx.getColor('brand')(locationLabel(ctx)),
  },

  branch: {
    side: 'right',
    tui: ({ props, muted }) =>
      !props.cloudSessionActive && props.gitBranch ? (
        <Chip
          value={props.gitBranch}
          color={muted ?? ChipColor.PRIMARY}
          wrap={true}
        />
      ) : null,
    lite: ({ props, getColor }) => {
      if (props.cloudSessionActive || !props.gitBranch) return '';
      const secondary = getColor('secondary');
      return `${secondary('(')}${getColor('primary')(props.gitBranch)}${secondary(')')}`;
    },
  },

  date: {
    side: 'left',
    tui: ({ now, muted }) =>
      now ? (
        <Chip
          value={formatClockDate(now)}
          color={muted ?? ChipColor.SECONDARY}
        />
      ) : null,
    lite: ({ now, getColor }) =>
      now ? getColor('secondary')(formatClockDate(now)) : '',
  },

  time: {
    side: 'left',
    tui: ({ now, muted }) =>
      now ? (
        <Chip
          value={formatClockTime(now)}
          color={muted ?? ChipColor.SECONDARY}
        />
      ) : null,
    lite: ({ now, getColor }) =>
      now ? getColor('secondary')(formatClockTime(now)) : '',
  },

  usage: {
    side: 'left',
    tui: ({ props, muted }) =>
      props.usagePercent == null ? null : (
        <Chip
          value={`usage ${props.usagePercent}%`}
          color={
            muted ??
            (props.usagePercent >= 90 ? ChipColor.WARNING : ChipColor.SECONDARY)
          }
        />
      ),
    lite: ({ props, getColor }) => {
      if (props.usagePercent == null) return '';
      const value = `${props.usagePercent}%`;
      const painted =
        props.usagePercent >= 90
          ? getColor('warning')(value)
          : getColor('secondary')(value);
      return `${painted} ${chalk.dim('usage')}`;
    },
  },

  credits: {
    side: 'left',
    tui: ({ props, muted }) =>
      props.creditsRemaining == null ? null : (
        <Chip
          value={`${Math.round(props.creditsRemaining).toLocaleString()} credits`}
          color={muted ?? ChipColor.BRAND}
        />
      ),
    lite: ({ props, getColor }) =>
      props.creditsRemaining == null
        ? ''
        : `${getColor('brand')(Math.round(props.creditsRemaining).toLocaleString())} ${chalk.dim('credits')}`,
  },
};

/**
 * Ids the given surface can actually paint, in paint order.
 *
 * Derived from the renderers rather than a second list, so a segment cannot be
 * offered somewhere it would render nothing.
 */
export function statusSegmentIdsFor(
  surface: UiMode
): readonly StatusSegmentId[] {
  if (surface === 'tui') return STATUS_SEGMENT_IDS;
  return STATUS_SEGMENT_IDS.filter((id) => STATUS_SEGMENTS[id].lite !== null);
}

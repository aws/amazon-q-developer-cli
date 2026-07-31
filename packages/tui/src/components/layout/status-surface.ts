import type { ComponentType } from 'react';
import type { AppState } from '../../stores/app-store.js';

export interface StatusSurfaceProps {
  agentName: string | null;
  /** Whether the session runs the bundled autonomous mode (Autonomous chip). */
  autonomousModeActive?: boolean;
  modelName: string | null;
  effort: string | null;
  contextUsagePercent: number | null;
  workspacePath: string;
  gitBranch: string | null;
  goalStatus: AppState['goalStatus'];
  tangentName?: string | null;
  cloudSessionActive?: boolean;
  cloudRepo?: string | null;
  cloudBranch?: string | null;
  cloudExtraRepos?: number;
  codeIntelligenceActive?: boolean;
  dimmed?: boolean;
  pendingAgentName?: string | null;
  animationFrame?: number;
  /** Pins the clock for the date/time segments; the surface derives one otherwise. */
  now?: Date | null;
  /** Percentage of the billing period's included credits already consumed. */
  usagePercent?: number | null;
  /** Included credits left in the billing period. */
  creditsRemaining?: number | null;
}

export type StatusSurface = ComponentType<StatusSurfaceProps>;

export function goalElapsed(
  goalStatus: NonNullable<AppState['goalStatus']>
): string {
  const secs = goalStatus.startedAt
    ? Math.floor((Date.now() - goalStatus.startedAt) / 1000)
    : (goalStatus.elapsedSecs ?? 0);
  if (secs <= 0) return '';
  if (secs >= 3600) {
    return `${Math.floor(secs / 3600)}h${Math.floor((secs % 3600) / 60)}m`;
  }
  return secs >= 60 ? `${Math.floor(secs / 60)}m` : `${secs}s`;
}

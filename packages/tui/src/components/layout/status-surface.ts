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
  cloudSessionActive?: boolean;
  cloudRepo?: string | null;
  cloudBranch?: string | null;
  cloudExtraRepos?: number;
  codeIntelligenceActive?: boolean;
  dimmed?: boolean;
  pendingAgentName?: string | null;
  animationFrame?: number;
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

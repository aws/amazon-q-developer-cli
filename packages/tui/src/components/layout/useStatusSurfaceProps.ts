import { useShallow } from 'zustand/react/shallow';
import { useAppStore } from '../../stores/app-store.js';
import type { StatusSurfaceProps } from './status-surface.js';

// Taking the required complement forces every status prop to be classified as
// shared (returned below) or layout-specific (named here) — a new prop cannot
// silently reach only one surface.
type DivergentKeys =
  | 'agentName'
  | 'autonomousModeActive'
  | 'modelName'
  | 'contextUsagePercent'
  | 'gitBranch'
  | 'dimmed'
  | 'pendingAgentName'
  | 'animationFrame'
  // Each surface's wrapper computes its own clock (useStatusClock); billing is
  // read per-surface via useStatusBilling(surface). None reach the shared bag.
  | 'now'
  | 'usagePercent'
  | 'creditsRemaining';

type SharedStatusSurfaceProps = Required<
  Omit<StatusSurfaceProps, DivergentKeys>
>;

// useShallow preserves object identity when no selected field changes, keeping
// the InlineLayout status-line useMemos that depend on it effective.
export function useStatusSurfaceProps(): SharedStatusSurfaceProps {
  return useAppStore(
    useShallow(
      (s): SharedStatusSurfaceProps => ({
        effort: s.currentEffort,
        goalStatus: s.goalStatus,
        tangentName: s.tangentName,
        cloudSessionActive: s.cloudSessionActive,
        cloudRepo: s.cloudRepo,
        cloudBranch: s.cloudBranch,
        cloudExtraRepos: s.cloudExtraRepos,
        codeIntelligenceActive: s.codeIntelligenceActive,
        // Read live, not once at module load: resuming a session from another
        // workspace calls process.chdir(), and the chip must follow. The
        // accompanying store update (the switch's system message) re-runs this
        // selector; useShallow keeps identity stable while the cwd is unchanged.
        workspacePath: process.cwd(),
      })
    )
  );
}

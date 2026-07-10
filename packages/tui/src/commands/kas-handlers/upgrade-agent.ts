import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';
import type { CommandOption } from '../../types/commands';
import type { UpgradeAnalysisRow } from '../../stores/app-store.js';
import {
  scanAgents,
  upgradeAgentFile,
  type AgentClassification,
  type AgentScope,
  type AgentUpgradeOutcome,
  type ScanResult,
} from '../../utils/agent-migration/index.js';

/** `/upgrade-agent`: routes to the bucket picker, diagnostics, or — on a picker selection — the bucket upgrade. */
export async function handleUpgradeAgent(
  cmd: KasCommand,
  args: string,
  ctx: CommandContext,
  options?: DispatchOptions
): Promise<void> {
  const trimmed = args.trim();
  if (!trimmed || trimmed === 'run') {
    return showBucketPicker(ctx, cmd);
  }
  if (trimmed === 'diagnostics') {
    return showDiagnostics(ctx, cmd);
  }
  if (options?.argIsSynthetic) {
    return upgradeBucket(ctx, trimmed);
  }
  ctx.showAlert(`Unknown /upgrade-agent argument: ${trimmed}`, 'error', 3000);
}

interface ActionableBucket {
  classification: AgentClassification;
  label: string;
  actionDescription: string;
}

const ACTIONABLE_BUCKETS: readonly ActionableBucket[] = [
  {
    classification: 'v2-only',
    label: 'V2',
    actionDescription: 'Upgrade to universal (V2 + V3) config',
  },
  {
    classification: 'universal-out-of-sync',
    label: 'Universal · Out of Sync',
    // Re-upgrade re-derives V3 purely from V2, so any hand-authored V3 edits
    // are discarded (the original is backed up to .bak — see the post-run
    // alert). Say "overwrites" plainly rather than the softer "align".
    actionDescription: 'Rebuild V3 from V2 (overwrites manual V3 edits)',
  },
];

// ── Pure presentation logic (no I/O; unit-tested directly) ──────────────────

/** Picker options (one per non-empty classification × scope) + per-bucket agent names for the preview panel. */
export function buildUpgradeBuckets(scan: ScanResult): {
  options: CommandOption[];
  preview: Record<string, string[]>;
} {
  const options: CommandOption[] = [];
  const preview: Record<string, string[]> = {};

  for (const bucket of ACTIONABLE_BUCKETS) {
    const counts = scan.counts[bucket.classification];
    for (const scope of ['local', 'global'] as AgentScope[]) {
      const count = counts[scope];
      if (count === 0) continue;
      const value = `${bucket.classification}:${scope}`;
      preview[value] = scan.agents
        .filter(
          (a) => a.classification === bucket.classification && a.scope === scope
        )
        .map((a) => a.name)
        .sort();
      const scopeLabel = scope === 'local' ? 'Workspace' : 'Global';
      options.push({
        value,
        // 3 columns (like /agent): label · scope group · action description.
        label: `${bucket.label} [${count} agent${count === 1 ? '' : 's'}]`,
        group: scopeLabel,
        description: bucket.actionDescription,
      });
    }
  }

  return { options, preview };
}

/** Diagnostics rows (in-sync agents + their warnings) and the header, or `null` when there are none to show. */
export function buildDiagnosticsRows(
  scan: ScanResult
): { rows: UpgradeAnalysisRow[]; desc: string } | null {
  // Only in-sync agents are listed: their warnings describe the live V3 config.
  const rows: UpgradeAnalysisRow[] = scan.agents
    .filter((agent) => agent.classification === 'universal-in-sync')
    .map((agent) => ({
      name: agent.name,
      scope: agent.scope,
      warnings: agent.warnings,
    }));

  if (rows.length === 0) {
    return null;
  }

  // The list shows only in-sync universal agents, so the header is just that
  // count (the picker owns the v2-only / out-of-sync numbers).
  const desc = `${rows.length} Universal agent${rows.length === 1 ? '' : 's'}`;
  return { rows, desc };
}

/** Parse a picker `classification:scope` value; `null` if malformed (no colon). */
export function parseBucketValue(
  value: string
): { classification: AgentClassification; scope: AgentScope } | null {
  const colonIndex = value.indexOf(':');
  if (colonIndex < 0) {
    return null;
  }
  return {
    classification: value.slice(0, colonIndex) as AgentClassification,
    scope: value.slice(colonIndex + 1) as AgentScope,
  };
}

/** Format the post-upgrade summary alert from the per-agent outcomes. */
export function summarizeBucketUpgrade(outcomes: AgentUpgradeOutcome[]): {
  message: string;
  level: 'success' | 'error';
} {
  let upgraded = 0;
  let needsReview = 0;
  let errors = 0;
  for (const outcome of outcomes) {
    if (outcome.status === 'upgraded') {
      upgraded += 1;
      if (outcome.warnings.length > 0) needsReview += 1;
    } else if (outcome.status === 'error') {
      errors += 1;
    }
  }

  const parts: string[] = [];
  if (upgraded > 0) {
    parts.push(`Upgraded ${upgraded} agent${upgraded === 1 ? '' : 's'}`);
  }
  if (needsReview > 0) {
    parts.push(
      `${needsReview} with warnings — run /upgrade-agent diagnostics to review`
    );
  }
  if (errors > 0) {
    parts.push(`${errors} failed`);
  }
  const summary = parts.join(' · ') || 'No changes written';
  return {
    message: `${summary} (backed up to .json.bak)`,
    level: errors > 0 ? 'error' : 'success',
  };
}

// ── Effectful handlers (thin: scan/upgrade I/O + ctx wiring) ─────────────────

/** Opens the read-only diagnostics menu: in-sync agents and their conversion warnings. */
function showDiagnostics(ctx: CommandContext, cmd: KasCommand): void {
  const scan = scanAgents();

  if (scan.total === 0) {
    ctx.showAlert('No agent configs found in .kiro/agents/', 'warning', 3000);
    return;
  }

  const diagnostics = buildDiagnosticsRows(scan);
  if (!diagnostics) {
    // Diagnostics reviews already-upgraded (in-sync) agents; there are none to
    // show yet. Point at the upgrade flow regardless of what's pending.
    ctx.showAlert(
      'No upgraded agents to review — run /upgrade-agent first',
      'warning',
      4000
    );
    return;
  }

  // Stash the rows for the menu to read, then open it through the
  // activeCommand path (previewKey routes CommandMenu → UpgradeDiagnosticsMenu).
  ctx.setUpgradeDiagnostics(diagnostics.rows, diagnostics.desc);
  ctx.setActiveCommand({
    command: cmd,
    options: diagnostics.rows.map((r) => ({ value: r.name, label: r.name })),
    previewKey: 'upgrade-diagnostics',
  });
}

/** Opens the picker of actionable (classification × scope) buckets to upgrade. */
function showBucketPicker(ctx: CommandContext, cmd: KasCommand): void {
  const { options, preview } = buildUpgradeBuckets(scanAgents());

  if (options.length === 0) {
    ctx.showAlert(
      'No agents to upgrade — every agent is already in sync or V3-only',
      'success',
      3000
    );
    return;
  }

  // Stash the per-bucket agent names, then open the picker; CommandMenu renders
  // the highlighted bucket's agents in a panel below the menu (previewKey).
  ctx.setUpgradeRunPreview(preview);
  ctx.setActiveCommand({ command: cmd, options, previewKey: 'upgrade-run' });
}

/** Upgrades every agent in the picked `classification:scope` bucket (each backed up to .bak). */
function upgradeBucket(ctx: CommandContext, value: string): void {
  const parsed = parseBucketValue(value);
  if (!parsed) {
    ctx.showAlert(`Invalid /upgrade-agent option: ${value}`, 'error', 3000);
    return;
  }

  // Re-scan so we operate on the freshest disk state, not on the snapshot
  // used to build the picker.
  const scan = scanAgents();
  const targets = scan.agents.filter(
    (a) =>
      a.classification === parsed.classification && a.scope === parsed.scope
  );

  if (targets.length === 0) {
    ctx.showAlert(
      'Nothing to upgrade in that bucket (state changed since picker)',
      'warning',
      3000
    );
    return;
  }

  const outcomes = targets.map((a) => upgradeAgentFile(a.sourcePath));
  const { message, level } = summarizeBucketUpgrade(outcomes);
  ctx.showAlert(message, level, 5000);
}

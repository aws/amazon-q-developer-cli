/**
 * Read-only classification scan for the `/upgrade-agent` panels. Walks the
 * workspace-local and user-global agent dirs and classifies every agent
 * (content-based, re-derived from disk each call). Mirrors `upgradeAgentConfig`.
 */

import {
  type AgentClassification,
  ACTIONABLE_CLASSIFICATIONS,
  isActionable,
  type MigrationWarning,
  upgradeAgentConfig,
} from './migrate.js';
import {
  agentNameFromPath,
  defaultAgentDirs,
  isExternallyManaged,
  listAgentFiles,
  looksLikeAgent,
  parseAgentFile,
} from './io.js';

export type AgentScope = 'local' | 'global';

/** Re-export the engine's classification type so panel code only imports here. */
export type { AgentClassification, MigrationWarning } from './migrate.js';
export { ACTIONABLE_CLASSIFICATIONS, isActionable };

export interface ScannedAgent {
  name: string;
  scope: AgentScope;
  classification: AgentClassification;
  /** Absolute path of the source `.json` file. */
  sourcePath: string;
  /** Conversion warnings from the V3 derivation (empty for v3-only / no-trust agents). */
  warnings: MigrationWarning[];
}

/** Per-scope counts for one classification bucket. */
export interface BucketCount {
  local: number;
  global: number;
  total: number;
}

export interface ScanResult {
  agents: ScannedAgent[];
  counts: Record<AgentClassification, BucketCount>;
  /** Total number of distinct agents found. */
  total: number;
}

interface ScanDir {
  dir: string;
  scope: AgentScope;
}

/** Default scan dirs: workspace-local first, then user-global. */
export function defaultScanDirs(cwd: string = process.cwd()): ScanDir[] {
  const [local, global] = defaultAgentDirs(cwd);
  return [
    { dir: local!, scope: 'local' },
    { dir: global!, scope: 'global' },
  ];
}

function emptyCounts(): Record<AgentClassification, BucketCount> {
  const zero = (): BucketCount => ({ local: 0, global: 0, total: 0 });
  return {
    'v2-only': zero(),
    'universal-out-of-sync': zero(),
    'universal-in-sync': zero(),
    'v3-only': zero(),
  };
}

/**
 * Scan agent directories and classify every agent. Top-level files only;
 * backup files are filtered out by `listAgentFiles`.
 */
export function scanAgents(dirs: ScanDir[] = defaultScanDirs()): ScanResult {
  const agents: ScannedAgent[] = [];

  for (const { dir, scope } of dirs) {
    for (const path of listAgentFiles(dir)) {
      const config = parseAgentFile(path);
      if (!config || !looksLikeAgent(config)) continue;
      if (isExternallyManaged(config)) continue;

      const result = upgradeAgentConfig(config);
      agents.push({
        name: agentNameFromPath(path),
        scope,
        classification: result.classification,
        sourcePath: path,
        warnings: result.warnings,
      });
    }
  }

  const counts = emptyCounts();
  for (const a of agents) {
    const bucket = counts[a.classification];
    bucket[a.scope] += 1;
    bucket.total += 1;
  }

  return { agents, counts, total: agents.length };
}

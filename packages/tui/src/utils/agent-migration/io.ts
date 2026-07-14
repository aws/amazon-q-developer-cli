/**
 * File I/O for the universal-config upgrade. Enriches V2-style configs with
 * derived V3 fields, rewritten in place after backing the original up to
 * `<name>.json.bak` (numbered on collision). The result is a universal config
 * both engines load: V2 (Rust) reads its trust fields, KAS reads `permissions`.
 */

import {
  copyFileSync,
  existsSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from 'fs';
import { join } from 'path';

import { kiroHomePath } from '../kiro-home.js';
import { logger } from '../logger.js';
import {
  type AgentClassification,
  type MigrationWarning,
  upgradeAgentConfig,
} from './migrate.js';

/** Suffix used for the safety-net backup of the pre-upgrade source file. */
export const BACKUP_SUFFIX = '.bak';

/** Per-agent upgrade outcome. */
export interface AgentUpgradeOutcome {
  /** Agent name (filename without extension). */
  name: string;
  sourcePath: string;
  /** Backup written before the in-place rewrite, if any. */
  backupPath?: string;
  classification: AgentClassification | 'not-agent';
  status:
    | 'upgraded'
    | 'skipped-in-sync'
    | 'skipped-v3-only'
    | 'skipped-not-agent'
    | 'skipped-managed'
    | 'error';
  warnings: MigrationWarning[];
  error?: string;
}

/** Workspace-level (`<cwd>/.kiro/agents`) and user-level (`~/.kiro/agents`) dirs. */
export function defaultAgentDirs(cwd: string = process.cwd()): string[] {
  return [join(cwd, '.kiro', 'agents'), kiroHomePath('agents')];
}

/**
 * Next available backup path (GNU `cp --backup=numbered`): `foo.json.bak`, then
 * `.bak.1`, `.bak.2`, … Always returns a path that doesn't already exist.
 */
export function backupPath(sourcePath: string): string {
  const base = `${sourcePath}${BACKUP_SUFFIX}`;
  if (!existsSync(base)) {
    return base;
  }
  for (let i = 1; i < 10000; i++) {
    const candidate = `${sourcePath}${BACKUP_SUFFIX}.${i}`;
    if (!existsSync(candidate)) {
      return candidate;
    }
  }
  // Bail out hard rather than silently overwrite a backup.
  throw new Error(
    `Refusing to upgrade ${sourcePath}: too many existing backups`
  );
}

/** Agent display name from its path: filename without `.json`/`.md`. */
export function agentNameFromPath(filePath: string): string {
  const base = filePath.split(/[\\/]/).pop() ?? filePath;
  return base.replace(/\.(json|md)$/, '');
}

/** Lenient parse of an agent JSON file. Returns `undefined` on any failure. */
export function parseAgentFile(
  filePath: string
): Record<string, unknown> | undefined {
  try {
    const raw = JSON.parse(readFileSync(filePath, 'utf-8'));
    if (raw && typeof raw === 'object' && !Array.isArray(raw)) {
      return raw as Record<string, unknown>;
    }
  } catch (err) {
    logger.debug(`[agent-upgrade] failed to parse ${filePath}: ${String(err)}`);
  }
  return undefined;
}

/**
 * List candidate agent files (absolute `.json` paths). Backup files (`.bak*`)
 * are excluded. Returns `[]` for missing/unreadable directories.
 */
export function listAgentFiles(dir: string): string[] {
  if (!existsSync(dir)) {
    return [];
  }
  try {
    return readdirSync(dir)
      .sort()
      .filter((entry) => entry.endsWith('.json'))
      .map((entry) => join(dir, entry));
  } catch (err) {
    logger.warn(`[agent-upgrade] Failed to read dir ${dir}: ${String(err)}`);
    return [];
  }
}

/** True if a parsed object looks like an agent config (has a meaningful field). */
export function looksLikeAgent(config: Record<string, unknown>): boolean {
  return (
    'prompt' in config ||
    'tools' in config ||
    'allowedTools' in config ||
    'permissions' in config ||
    'toolsSettings' in config ||
    'hooks' in config
  );
}

/**
 * Heuristic: an AIM-managed agent (marker phrase in `description`) must never
 * be rewritten. Centralized so the scan and the writer share one exclusion.
 */
export function isExternallyManaged(config: Record<string, unknown>): boolean {
  const desc = config.description;
  return typeof desc === 'string' && desc.includes('managed by AIM');
}

/**
 * Upgrade a single agent file in place: back up to `<source>.bak` (numbered on
 * collision), then write the enriched config. Non-actionable classifications
 * leave the file untouched.
 */
export function upgradeAgentFile(sourcePath: string): AgentUpgradeOutcome {
  const name = agentNameFromPath(sourcePath);

  let raw: string;
  try {
    raw = readFileSync(sourcePath, 'utf-8');
  } catch (err) {
    return {
      name,
      sourcePath,
      classification: 'not-agent',
      status: 'error',
      warnings: [],
      error: `Failed to read: ${String(err)}`,
    };
  }

  let config: Record<string, unknown>;
  try {
    config = JSON.parse(raw);
  } catch (err) {
    return {
      name,
      sourcePath,
      classification: 'not-agent',
      status: 'error',
      warnings: [],
      error: `Invalid JSON: ${String(err)}`,
    };
  }

  if (!looksLikeAgent(config)) {
    return {
      name,
      sourcePath,
      classification: 'not-agent',
      status: 'skipped-not-agent',
      warnings: [],
    };
  }

  const result = upgradeAgentConfig(config);

  // Defense-in-depth: never rewrite a managed agent even if handed its path
  // directly (the scan excludes these too).
  if (isExternallyManaged(config)) {
    return {
      name,
      sourcePath,
      classification: result.classification,
      status: 'skipped-managed',
      warnings: [],
    };
  }

  // Non-actionable classifications stay untouched.
  if (result.classification === 'v3-only') {
    return {
      name,
      sourcePath,
      classification: result.classification,
      status: 'skipped-v3-only',
      warnings: [],
    };
  }
  if (result.classification === 'universal-in-sync') {
    return {
      name,
      sourcePath,
      classification: result.classification,
      status: 'skipped-in-sync',
      warnings: [],
    };
  }

  let backup: string;
  try {
    backup = backupPath(sourcePath);
    copyFileSync(sourcePath, backup);
  } catch (err) {
    return {
      name,
      sourcePath,
      classification: result.classification,
      status: 'error',
      warnings: result.warnings,
      error: `Failed to back up: ${String(err)}`,
    };
  }

  try {
    writeFileSync(sourcePath, JSON.stringify(result.config, null, 2) + '\n');
  } catch (err) {
    return {
      name,
      sourcePath,
      backupPath: backup,
      classification: result.classification,
      status: 'error',
      warnings: result.warnings,
      error: `Failed to write: ${String(err)}`,
    };
  }

  return {
    name,
    sourcePath,
    backupPath: backup,
    classification: result.classification,
    status: 'upgraded',
    warnings: result.warnings,
  };
}

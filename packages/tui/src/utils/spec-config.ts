/**
 * Loader for `.kiro/specs/<feature>/.config.kiro`.
 *
 * The file is plain JSON with two known fields today:
 *
 *   {
 *     "workflowType": "requirements-first" | "design-first",
 *     "specType":     "feature" | "bugfix"
 *   }
 *
 * Both are optional. Unknown values, parse failures, missing files, and
 * permission errors all fall back to defaults silently — this read is
 * UX-affecting (it drives the stage-bar order) but never user-blocking.
 */

import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { specsRoot } from './spec-workspace.js';

export type WorkflowType = 'requirements-first' | 'design-first';
export type SpecType = 'feature' | 'bugfix';

export interface SpecConfig {
  workflowType: WorkflowType;
  specType: SpecType;
}

export const DEFAULT_SPEC_CONFIG: SpecConfig = {
  workflowType: 'requirements-first',
  specType: 'feature',
};

const KNOWN_WORKFLOW_TYPES: readonly WorkflowType[] = [
  'requirements-first',
  'design-first',
] as const;

const KNOWN_SPEC_TYPES: readonly SpecType[] = ['feature', 'bugfix'] as const;

/** Absolute path to a feature's `.config.kiro`. */
export function specConfigPath(
  workspaceRoot: string,
  featureName: string
): string {
  return join(specsRoot(workspaceRoot), featureName, '.config.kiro');
}

/**
 * Load the spec config for `<feature>` or return defaults.
 *
 * Synchronous on purpose: callers (the artifact-view open path) want
 * the config to land in the same render frame as the artifact summary.
 * The file is tiny (well under a kB in practice) so the cost is
 * negligible. We also bound the read to 64 kB defensively.
 */
export function loadSpecConfig(
  workspaceRoot: string,
  featureName: string
): SpecConfig {
  const path = specConfigPath(workspaceRoot, featureName);
  if (!existsSync(path)) return DEFAULT_SPEC_CONFIG;

  let text: string;
  try {
    text = readFileSync(path, 'utf8');
  } catch {
    return DEFAULT_SPEC_CONFIG;
  }
  if (text.length > 64 * 1024) {
    // .config.kiro is a tiny manifest; anything bigger than 64 kB is
    // almost certainly not a config file. Treat as missing.
    return DEFAULT_SPEC_CONFIG;
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return DEFAULT_SPEC_CONFIG;
  }
  if (!parsed || typeof parsed !== 'object') return DEFAULT_SPEC_CONFIG;

  const obj = parsed as Record<string, unknown>;
  const workflowType =
    typeof obj['workflowType'] === 'string' &&
    (KNOWN_WORKFLOW_TYPES as readonly string[]).includes(obj['workflowType'])
      ? (obj['workflowType'] as WorkflowType)
      : DEFAULT_SPEC_CONFIG.workflowType;
  const specType =
    typeof obj['specType'] === 'string' &&
    (KNOWN_SPEC_TYPES as readonly string[]).includes(obj['specType'])
      ? (obj['specType'] as SpecType)
      : DEFAULT_SPEC_CONFIG.specType;

  return { workflowType, specType };
}

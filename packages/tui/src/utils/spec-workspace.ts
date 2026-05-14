/**
 * Helpers for discovering and resolving spec feature directories under
 * `.kiro/specs/` in the workspace.
 *
 * A spec feature is a directory under `.kiro/specs/<featureName>/` that
 * contains one or more of `requirements.md`, `design.md`, `tasks.md`, or
 * `bugfix.md` (the documents tracked by the KAS spec workflow — see the
 * `SpecInvokeBase` contract in @kiro/acp-type-covenant).
 *
 * These helpers are synchronous and keep no state so they're trivial to
 * unit-test from the command effect without mocking the session client.
 */

import { existsSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

/** Relative paths of spec documents the agent recognises, in the order
 *  they're reported in the spec workflow. */
const SPEC_DOCUMENT_NAMES = [
  'requirements.md',
  'design.md',
  'tasks.md',
  'bugfix.md',
] as const;

export type SpecDocumentName = (typeof SPEC_DOCUMENT_NAMES)[number];

export interface SpecFeatureSummary {
  /** Directory name under `.kiro/specs/`. */
  featureName: string;
  /** Absolute path to `.kiro/specs/<featureName>/`. */
  dirPath: string;
  /** Which of the well-known spec documents exist on disk. */
  documents: readonly SpecDocumentName[];
  /** Absolute path to `tasks.md` when it exists — used by `_kiro/spec/invoke`. */
  tasksFilePath?: string;
  /** Absolute paths of the documents that exist — the `specDocuments` field
   *  on `_kiro/spec/invoke`. */
  specDocumentPaths: string[];
}

/** Absolute path to `.kiro/specs/` under the given workspace root. */
export function specsRoot(workspaceRoot: string): string {
  return join(workspaceRoot, '.kiro', 'specs');
}

/**
 * Enumerate feature directories under `.kiro/specs/`, sorted alphabetically.
 *
 * Returns an empty array when the directory does not exist or cannot be
 * read (never throws); callers surface an empty-state message to the user.
 */
export function listSpecFeatures(workspaceRoot: string): SpecFeatureSummary[] {
  const root = specsRoot(workspaceRoot);
  if (!existsSync(root)) return [];

  let entries: string[];
  try {
    entries = readdirSync(root);
  } catch {
    return [];
  }

  const features: SpecFeatureSummary[] = [];
  for (const entry of entries) {
    const dirPath = join(root, entry);
    // Skip dotfiles and non-directories (e.g. a stray README). We stat
    // defensively because the fs layer can race with file operations.
    if (entry.startsWith('.')) continue;
    try {
      if (!statSync(dirPath).isDirectory()) continue;
    } catch {
      continue;
    }

    const documents: SpecDocumentName[] = [];
    const specDocumentPaths: string[] = [];
    let tasksFilePath: string | undefined;
    for (const name of SPEC_DOCUMENT_NAMES) {
      const filePath = join(dirPath, name);
      if (existsSync(filePath)) {
        documents.push(name);
        specDocumentPaths.push(filePath);
        if (name === 'tasks.md') tasksFilePath = filePath;
      }
    }

    // Skip directories that have no spec documents at all — they're
    // probably leftovers or in-progress scratch dirs.
    if (documents.length === 0) continue;

    features.push({
      featureName: entry,
      dirPath,
      documents,
      tasksFilePath,
      specDocumentPaths,
    });
  }

  features.sort((a, b) => a.featureName.localeCompare(b.featureName));
  return features;
}

/** Look up a single feature by name, or return undefined when not found. */
export function findSpecFeature(
  workspaceRoot: string,
  featureName: string
): SpecFeatureSummary | undefined {
  return listSpecFeatures(workspaceRoot).find(
    (f) => f.featureName === featureName
  );
}

/** Human-readable, comma-separated list of doc types present (e.g.
 *  "requirements, design, tasks"). Used in selection-menu descriptions. */
export function describeSpecDocuments(summary: SpecFeatureSummary): string {
  if (summary.documents.length === 0) return 'empty';
  return summary.documents.map((d) => d.replace(/\.md$/, '')).join(', ');
}

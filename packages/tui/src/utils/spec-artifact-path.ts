/**
 * Detection of spec-artifact paths in tool call arguments.
 *
 * A spec artifact path is one of:
 *   .kiro/specs/<feature>/requirements.md
 *   .kiro/specs/<feature>/design.md
 *   .kiro/specs/<feature>/tasks.md
 *
 * `<feature>` must not contain a path separator (single directory level).
 * The match is permissive at the start of the path so it works on absolute
 * paths emitted by the agent (`/Users/.../workspace/.kiro/specs/...`) and
 * on workspace-relative paths.
 */

import { resolve, isAbsolute } from 'node:path';
import type { ArtifactKind } from './spec-artifact-loader.js';

/** Match anywhere in the path, not just from the start. */
const SPEC_ARTIFACT_RE =
  /(?:^|[\\/])\.kiro[\\/]specs[\\/]([^\\/]+)[\\/](requirements|design|tasks)\.md$/;

export interface SpecArtifactPathMatch {
  /** Original path as provided in the tool call args. */
  originalPath: string;
  /** Absolute path resolved against `workspaceRoot`. Used as the dedup key. */
  absolutePath: string;
  featureName: string;
  artifact: ArtifactKind;
}

/**
 * If `path` looks like a spec artifact path, return the structured match;
 * otherwise return null. Pure / synchronous — does not touch the filesystem.
 */
export function matchSpecArtifactPath(
  path: string,
  workspaceRoot: string
): SpecArtifactPathMatch | null {
  if (typeof path !== 'string' || path.length === 0) return null;
  const m = SPEC_ARTIFACT_RE.exec(path);
  if (!m) return null;
  const featureName = m[1]!;
  const artifact = m[2] as ArtifactKind;
  const absolutePath = isAbsolute(path) ? path : resolve(workspaceRoot, path);
  return {
    originalPath: path,
    absolutePath,
    featureName,
    artifact,
  };
}

/**
 * Extract the `path` argument from a tool call's args object. The agent
 * uses `path` for both `fs_write` (V1 alias) and `Write` (KAS native).
 * Returns null when the args don't contain a string `path`.
 */
export function extractToolPath(
  args: Record<string, unknown> | undefined
): string | null {
  if (!args) return null;
  const p = args['path'];
  return typeof p === 'string' ? p : null;
}

/**
 * True for tool names that the agent uses to write file content. We
 * include both the V1 alias (`fs_write`) and the KAS native name
 * (`Write`) to stay engine-portable, even though this feature is
 * KAS-only — the gating check happens at the listener layer.
 */
export function isFileWriteToolName(name: string): boolean {
  return name === 'fs_write' || name === 'Write';
}
